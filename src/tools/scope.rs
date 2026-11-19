//! `org.apache.velocity.tools.Scope`: the known toolbox scopes (`request`, `session`, `application`, plus any
//! added).

use std::sync::LazyLock;

use java_compat::lang::JError;
use parking_lot::Mutex;

/// `Scope.REQUEST`.
pub const REQUEST: &str = "request";
/// `Scope.SESSION`.
pub const SESSION: &str = "session";
/// `Scope.APPLICATION`.
pub const APPLICATION: &str = "application";

static VALUES: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(vec![REQUEST.into(), SESSION.into(), APPLICATION.into()]));

/// `Scope.add(newScope)`.
pub fn add(new_scope: &str) -> Result<(), JError> {
    let s = new_scope.to_lowercase();
    let mut v = VALUES.lock();
    if v.contains(&s) {
        return Err(JError::new("java.lang.IllegalArgumentException", format!("Scope '{s}' has already been registered.")));
    }
    v.push(s);
    Ok(())
}

/// `Scope.exists(scope)`.
pub fn exists(scope: &str) -> bool {
    VALUES.lock().contains(&scope.to_lowercase())
}

/// `Scope.values()`.
pub fn values() -> Vec<String> {
    VALUES.lock().clone()
}
