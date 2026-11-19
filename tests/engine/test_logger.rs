//! `org.apache.velocity.test.misc.TestLogger`: a logger that is suppressed, captures into a buffer, or writes to
//! stderr. Its prefix is the one of its enabled level, whatever the message's.

#![allow(dead_code)]

use parking_lot::Mutex;
use velocity::log::{Level, Logger, Throwable};

pub const LOG_LEVEL_TRACE: Level = Level::Trace;
pub const LOG_LEVEL_DEBUG: Level = Level::Debug;
pub const LOG_LEVEL_INFO: Level = Level::Info;
pub const LOG_LEVEL_WARN: Level = Level::Warn;
pub const LOG_LEVEL_ERROR: Level = Level::Error;

struct State {
    log: Option<String>,
    suppress: bool,
    capture: bool,
    enabled_level: Level,
}

pub struct TestLogger(Mutex<State>);

fn prefix(level: Level) -> &'static str {
    match level {
        Level::Trace => " [trace] ",
        Level::Debug => " [debug] ",
        Level::Info => "  [info] ",
        Level::Warn => "  [warn] ",
        Level::Error => " [error]",
    }
}

impl Default for TestLogger {
    fn default() -> TestLogger {
        TestLogger::new(true, true)
    }
}

impl TestLogger {
    pub fn new(suppress: bool, capture: bool) -> TestLogger {
        let l = TestLogger(Mutex::new(State { log: None, suppress, capture, enabled_level: Level::Info }));
        if suppress {
            l.off();
        } else if capture {
            l.start_capture();
        }
        l
    }

    pub fn on(&self) {
        let capture = {
            let mut s = self.0.lock();
            if !s.suppress {
                return;
            }
            s.suppress = false;
            s.capture
        };
        if capture {
            self.start_capture();
        }
    }

    pub fn off(&self) {
        self.0.lock().suppress = true;
    }

    pub fn start_capture(&self) {
        let mut s = self.0.lock();
        s.capture = true;
        if !s.suppress {
            s.log = Some(String::new());
        }
    }

    pub fn stop_capture(&self) {
        self.0.lock().capture = false;
    }

    pub fn set_enabled_level(&self, level: Level) {
        self.0.lock().enabled_level = level;
    }

    pub fn is_level_enabled(&self, level: Level) -> bool {
        let s = self.0.lock();
        !s.suppress && level >= s.enabled_level
    }

    /// `getLog()`.
    pub fn get_log(&self) -> String {
        self.0.lock().log.clone().expect("the log was captured")
    }
}

impl Logger for TestLogger {
    fn name(&self) -> String {
        String::new()
    }

    fn is_enabled(&self, level: Level) -> bool {
        self.is_level_enabled(level)
    }

    fn log(&self, level: Level, message: &str, throwable: Option<&Throwable>) {
        let mut s = self.0.lock();
        if s.suppress || level < s.enabled_level {
            return;
        }
        let mut text = format!("{}{message}\n", prefix(s.enabled_level));
        if let Some(t) = throwable {
            let m = t.message.clone().unwrap_or_else(|| "null".into());
            // `getMessage()`, then `printStackTrace` (its first line; there are no Java frames).
            text.push_str(&format!("{m}\n{}: {m}\n", t.class));
        }
        if s.capture {
            if let Some(log) = s.log.as_mut() {
                log.push_str(&text);
            }
        } else {
            eprint!("{text}");
        }
    }
}

/// Templates call `$log.startCapture()` on it.
impl java_compat::lang::JObject for TestLogger {
    fn class_name(&self) -> String {
        "org.apache.velocity.test.misc.TestLogger".into()
    }

    fn invoke(&self, _: &str, _: &[java_compat::lang::JValue]) -> Option<Result<Option<java_compat::lang::JValue>, java_compat::lang::JError>> {
        None
    }

    fn to_java_string(&self) -> String {
        java_compat::reflect::identity_string(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<std::sync::Arc<java_compat::reflect::ClassDef>> {
        static DEF: std::sync::LazyLock<std::sync::Arc<java_compat::reflect::ClassDef>> = std::sync::LazyLock::new(|| {
            java_compat::reflect::ClassDef::new("org.apache.velocity.test.misc.TestLogger")
                .method::<TestLogger>("on", &[], None, |l, _, _| {
                    l.on();
                    Ok(None)
                })
                .method::<TestLogger>("off", &[], None, |l, _, _| {
                    l.off();
                    Ok(None)
                })
                .method::<TestLogger>("startCapture", &[], None, |l, _, _| {
                    l.start_capture();
                    Ok(None)
                })
                .method::<TestLogger>("stopCapture", &[], None, |l, _, _| {
                    l.stop_capture();
                    Ok(None)
                })
                .method::<TestLogger>("getLog", &[], Some(java_compat::reflect::JType::string()), |l, _, _| Ok(Some(java_compat::lang::JValue::str(l.get_log()))))
                .register()
        });
        Some(DEF.clone())
    }
}
