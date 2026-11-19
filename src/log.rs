//! Velocity's logging (SLF4J): the runtime's `Logger`, `runtime.log.instance` / `runtime.log.name`, and
//! `MessageFormatter`'s `{}` substitution. Without a configured instance nothing is logged.

use std::sync::Arc;

use java_compat::lang::JError;

use crate::VelocityError;

/// `org.slf4j.event.Level`, ordered by `LocationAwareLogger`'s integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// `TRACE`.
    Trace = 0,
    /// `DEBUG`.
    Debug = 10,
    /// `INFO`.
    Info = 20,
    /// `WARN`.
    Warn = 30,
    /// `ERROR`.
    Error = 40,
}

/// What a log call passes as its `Throwable`.
#[derive(Debug, Clone, PartialEq)]
pub struct Throwable {
    /// The exception's Java class name.
    pub class: String,
    /// Its message, if any.
    pub message: Option<String>,
}

impl From<&JError> for Throwable {
    fn from(e: &JError) -> Throwable {
        Throwable { class: e.class.clone(), message: e.message.clone() }
    }
}

impl From<&VelocityError> for Throwable {
    fn from(e: &VelocityError) -> Throwable {
        Throwable { class: e.class_name(), message: Some(e.message()) }
    }
}

/// `org.slf4j.Logger`, as far as Velocity uses it.
pub trait Logger: Send + Sync {
    /// `getName()`: the logger's name.
    fn name(&self) -> String;

    /// Whether messages at `level` are logged (`isTraceEnabled()` and the like).
    fn is_enabled(&self, level: Level) -> bool;

    /// Logs `message` at `level`, with the exception that caused it, if any.
    fn log(&self, level: Level, message: &str, throwable: Option<&Throwable>);
}

/// The logger when none is configured: SLF4J without a binding (`NOPLogger`).
pub struct NopLogger(pub String);

impl Logger for NopLogger {
    fn name(&self) -> String {
        self.0.clone()
    }

    fn is_enabled(&self, _: Level) -> bool {
        false
    }

    fn log(&self, _: Level, _: &str, _: Option<&Throwable>) {}
}

/// Convenience calls on any logger; a message is only built when its level is enabled.
pub trait LogExt {
    /// Logs the message `message` builds at `level`, building it only when the level is enabled.
    fn at(&self, level: Level, message: impl FnOnce() -> String);
    /// [`LogExt::at`] at `TRACE`.
    fn trace(&self, message: impl FnOnce() -> String) {
        self.at(Level::Trace, message)
    }
    /// [`LogExt::at`] at `DEBUG`.
    fn debug(&self, message: impl FnOnce() -> String) {
        self.at(Level::Debug, message)
    }
    /// [`LogExt::at`] at `INFO`.
    fn info(&self, message: impl FnOnce() -> String) {
        self.at(Level::Info, message)
    }
    /// [`LogExt::at`] at `WARN`.
    fn warn(&self, message: impl FnOnce() -> String) {
        self.at(Level::Warn, message)
    }
    /// [`LogExt::at`] at `ERROR`.
    fn error(&self, message: impl FnOnce() -> String) {
        self.at(Level::Error, message)
    }
    /// A message with its `Throwable`.
    fn thrown(&self, level: Level, message: impl FnOnce() -> String, throwable: &Throwable);
}

impl<L: Logger + ?Sized> LogExt for L {
    fn at(&self, level: Level, message: impl FnOnce() -> String) {
        if self.is_enabled(level) {
            self.log(level, &message(), None);
        }
    }

    fn thrown(&self, level: Level, message: impl FnOnce() -> String, throwable: &Throwable) {
        if self.is_enabled(level) {
            self.log(level, &message(), Some(throwable));
        }
    }
}

/// `MessageFormatter.arrayFormat`: each `{}` takes the next argument; `\{}` is a literal `{}`, `\\{}` a backslash
/// followed by the argument.
pub fn format(pattern: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut rest = pattern;
    let mut next = 0;
    while next < args.len() {
        let Some(i) = rest.find("{}") else { break };
        let escaped = i >= 1 && rest.as_bytes()[i - 1] == b'\\';
        let double_escaped = escaped && i >= 2 && rest.as_bytes()[i - 2] == b'\\';
        if escaped && !double_escaped {
            out.push_str(&rest[..i - 1]);
            out.push_str("{}");
        } else {
            out.push_str(&rest[..if double_escaped { i - 1 } else { i }]);
            out.push_str(&args[next]);
            next += 1;
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

/// `RuntimeConstants.RUNTIME_LOG_INSTANCE` (`runtime.log.instance`): an `Arc<dyn Logger>` object to log to.
pub const RUNTIME_LOG_INSTANCE: &str = "runtime.log.instance";
/// `RuntimeConstants.RUNTIME_LOG_NAME` (`runtime.log.name`): the name of the logger used when no instance is set.
pub const RUNTIME_LOG_NAME: &str = "runtime.log.name";
/// `RuntimeConstants.DEFAULT_RUNTIME_LOG_NAME`: the logger name when `runtime.log.name` is not set.
pub const DEFAULT_RUNTIME_LOG_NAME: &str = "org.apache.velocity";

/// `RuntimeInstance.initializeLog`: the configured instance (an `Arc<dyn Logger>`), else the named logger.
pub(crate) fn configured(props: &crate::props::ExtProperties) -> Result<(Arc<dyn Logger>, bool), VelocityError> {
    use crate::props::PropValue;
    match props.get(RUNTIME_LOG_INSTANCE) {
        Some(PropValue::Object(o)) => match o.downcast_ref::<Arc<dyn Logger>>() {
            Some(l) => Ok((l.clone(), true)),
            None => Err(VelocityError::velocity("java.lang.Object object set as runtime.log.instance is not a valid org.slf4j.Logger implementation.".to_string())),
        },
        Some(_) => Err(VelocityError::velocity("java.lang.String object set as runtime.log.instance is not a valid org.slf4j.Logger implementation.".to_string())),
        None => {
            let name = props.string(RUNTIME_LOG_NAME).unwrap_or_else(|| DEFAULT_RUNTIME_LOG_NAME.to_string());
            Ok((Arc::new(NopLogger(name)), false))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::format;

    #[test]
    fn message_formatter() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(format("a {} b {}", &a(&["1", "2"])), "a 1 b 2");
        assert_eq!(format("a {} b {}", &a(&["1"])), "a 1 b {}");
        assert_eq!(format("a \\{} b {}", &a(&["1"])), "a {} b 1");
        assert_eq!(format("a \\\\{} b", &a(&["1"])), "a \\1 b");
        assert_eq!(format("no args {}", &[]), "no args {}");
    }
}
