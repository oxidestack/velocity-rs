//! A deadline for the render running on this thread, checked by every construct that can repeat work: `#foreach`
//! iterations, macro calls, `#parse`, `#evaluate` and renders of a `#define`d block. A render cannot be interrupted
//! from outside, so it fails at the first check after the deadline with the error its embedder chose (MockServer's
//! `RenderDeadline` and `TimeLimited*` directives). The clock is read once every [`CHECKS_PER_CLOCK_READ`] checks.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use java_compat::lang::JError;

use crate::error::VelocityError;

/// `RenderDeadline.CHECKS_PER_CLOCK_READ`.
pub const CHECKS_PER_CLOCK_READ: u32 = 64;

struct Deadline {
    at: Instant,
    /// What the render fails with, passing through Velocity as a Java `RuntimeException`.
    error: JError,
    checks_until_clock_read: u32,
}

thread_local! {
    static CURRENT: RefCell<Option<Deadline>> = const { RefCell::new(None) };
}

/// Restores the enclosing deadline when dropped.
pub struct Scope {
    previous: Option<Deadline>,
}

impl Drop for Scope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT.with(|c| *c.borrow_mut() = previous);
    }
}

/// `RenderDeadline.start`: the renders on this thread, until the scope is dropped, fail with `error` once `timeout`
/// has passed; `None` means no deadline.
pub fn start(timeout: Option<Duration>, error: JError) -> Scope {
    let next = timeout.map(|t| Deadline { at: Instant::now() + t, error, checks_until_clock_read: CHECKS_PER_CLOCK_READ });
    Scope { previous: CURRENT.with(|c| std::mem::replace(&mut *c.borrow_mut(), next)) }
}

/// `RenderDeadline.check`: fails the render on this thread if its deadline has passed.
pub(crate) fn check() -> Result<(), VelocityError> {
    CURRENT.with(|c| {
        let mut current = c.borrow_mut();
        let Some(deadline) = current.as_mut() else { return Ok(()) };
        deadline.checks_until_clock_read -= 1;
        if deadline.checks_until_clock_read > 0 {
            return Ok(());
        }
        deadline.checks_until_clock_read = CHECKS_PER_CLOCK_READ;
        if Instant::now() > deadline.at {
            return Err(VelocityError::runtime(deadline.error.clone()));
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fails_after_the_deadline_and_restores_the_enclosing_one() {
        let error = JError::new("example.Timeout", "too slow");
        let _outer = start(None, error.clone());
        {
            let _inner = start(Some(Duration::ZERO), error.clone());
            let failed = (0..CHECKS_PER_CLOCK_READ).map(|_| check()).find(Result::is_err);
            assert_eq!(failed.map(|e| e.unwrap_err().message.clone()), Some("too slow".to_string()));
        }
        assert!((0..2 * CHECKS_PER_CLOCK_READ).all(|_| check().is_ok()));
    }
}
