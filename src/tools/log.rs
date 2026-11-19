//! Where the tools' SLF4J logging goes: a process-wide sink an application may install (nothing is logged
//! otherwise). Messages are what the Java tools log, formatted.

use std::sync::{Arc, OnceLock};

/// An SLF4J log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// `ERROR`.
    Error,
    /// `WARN`.
    Warn,
    /// `INFO`.
    Info,
    /// `DEBUG`.
    Debug,
    /// `TRACE`.
    Trace,
}

/// Where log messages go: called with the level, the logger name and the formatted message.
pub type Sink = Arc<dyn Fn(Level, &str, &str) + Send + Sync>;

static SINK: OnceLock<Sink> = OnceLock::new();

/// Installs the log sink (once per process): level, logger name, message.
pub fn set_sink(sink: Sink) {
    let _ = SINK.set(sink);
}

/// Sends `message` at `level` under `logger` to the installed sink; nothing without one.
pub fn log(level: Level, logger: &str, message: &str) {
    if let Some(s) = SINK.get() {
        s(level, logger, message);
    }
}

/// `logger.error(message)`: see `log`.
pub fn error(logger: &str, message: &str) {
    log(Level::Error, logger, message);
}

/// `logger.warn(message)`: see `log`.
pub fn warn(logger: &str, message: &str) {
    log(Level::Warn, logger, message);
}

/// `logger.info(message)`: see `log`.
pub fn info(logger: &str, message: &str) {
    log(Level::Info, logger, message);
}

/// `logger.debug(message)`: see `log`.
pub fn debug(logger: &str, message: &str) {
    log(Level::Debug, logger, message);
}

/// `logger.trace(message)`: see `log`.
pub fn trace(logger: &str, message: &str) {
    log(Level::Trace, logger, message);
}
