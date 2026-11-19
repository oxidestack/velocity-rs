//! `SafeConfig`, `LocaleConfig` and `FormatConfig`: the configuration state the generic tools share, and the
//! `configure(Map)` protocol (a `ValueParser` over the map, then safe mode and the config lock, both defaulting to
//! true once configured).

use std::sync::atomic::{AtomicBool, Ordering};

use java_compat::lang::{JError, JMap, JValue};
use java_compat::reflect::{ClassDef, JType};
use java_compat::text::Locale;
use parking_lot::Mutex;

use super::values::ValueParser;
use super::{c, o, s, val};

/// `SafeConfig.LOCK_CONFIG_KEY`: whether `configure(Map)` locks the configuration (default true).
pub const LOCK_CONFIG_KEY: &str = "lockConfig";
/// `SafeConfig.SAFE_MODE_KEY`: whether the tool hides its unsafe methods (default true).
pub const SAFE_MODE_KEY: &str = "safeMode";
/// `SafeConfig.LOGGER_NAME_KEY`: the configuration key naming the tool's logger.
pub const LOGGER_NAME_KEY: &str = "loggerName";
/// `SafeConfig.USE_CLASS_LOGGER_KEY`: the configuration key asking for a logger named after the tool's class.
pub const USE_CLASS_LOGGER_KEY: &str = "useClassLogger";
/// `FormatConfig.DEFAULT_FORMAT`.
pub const DEFAULT_FORMAT: &str = "default";
/// `FormatConfig.FORMAT_KEY`: the configuration key for the default format.
pub const FORMAT_KEY: &str = "format";

/// `SafeConfig`'s state: both flags start false and `configure(Map)` sets them.
#[derive(Default)]
pub struct SafeConfig {
    locked: AtomicBool,
    safe_mode: AtomicBool,
}

impl SafeConfig {
    /// `isConfigLocked()`: whether `configure(Map)` is now ignored.
    pub fn is_config_locked(&self) -> bool {
        self.locked.load(Ordering::SeqCst)
    }

    /// `isSafeMode()`: whether the tool hides its unsafe methods.
    pub fn is_safe_mode(&self) -> bool {
        self.safe_mode.load(Ordering::SeqCst)
    }

    /// `setLockConfig(lock)`.
    pub fn set_lock_config(&self, lock: bool) {
        self.locked.store(lock, Ordering::SeqCst);
    }

    /// `setSafeMode(safe)`.
    pub fn set_safe_mode(&self, safe: bool) {
        self.safe_mode.store(safe, Ordering::SeqCst);
    }
}

/// A tool built on `SafeConfig`: its state and its `configure(ValueParser)`.
pub trait Tool: Send + Sync + 'static {
    /// The tool's `SafeConfig` state.
    fn safe(&self) -> &SafeConfig;

    /// `configure(ValueParser)`, the protected hook `configure(Map)` calls.
    fn configure_values(&self, _values: &ValueParser) {}

    /// `configure(ValueParser)` for tools whose configuration can throw.
    fn try_configure_values(&self, values: &ValueParser) -> Result<(), JError> {
        self.configure_values(values);
        Ok(())
    }
}

/// `SafeConfig.configure(Map)`: nothing once locked; otherwise the tool's own configuration, then safe mode and
/// the lock from the same values.
pub fn configure(tool: &dyn Tool, params: &JMap) {
    let _ = try_configure(tool, params);
}

/// `SafeConfig.configure(Map)`, reporting what the tool's configuration throws.
pub fn try_configure(tool: &dyn Tool, params: &JMap) -> Result<(), JError> {
    if tool.safe().is_config_locked() {
        return Ok(());
    }
    let values = ValueParser::new(Some(params.clone()));
    tool.try_configure_values(&values)?;
    tool.safe().set_safe_mode(values.get_boolean_or(SAFE_MODE_KEY, true));
    tool.safe().set_lock_config(values.get_boolean_or(LOCK_CONFIG_KEY, true));
    Ok(())
}

/// `LocaleConfig`'s state: the tool's default locale, `Locale.getDefault()` unless configured.
pub struct LocaleConfig {
    /// The `SafeConfig` state `LocaleConfig` extends.
    pub safe: SafeConfig,
    locale: Mutex<Locale>,
}

impl Default for LocaleConfig {
    fn default() -> LocaleConfig {
        LocaleConfig { safe: SafeConfig::default(), locale: Mutex::new(super::default_locale()) }
    }
}

impl LocaleConfig {
    /// `getLocale()`: the tool's default locale.
    pub fn locale(&self) -> Locale {
        self.locale.lock().clone()
    }

    /// `setLocale(locale)`.
    pub fn set_locale(&self, locale: Locale) {
        *self.locale.lock() = locale;
    }

    /// `LocaleConfig.configure(values)`.
    pub fn configure(&self, values: &ValueParser) {
        if let Some(l) = values.get_locale(super::toolbox::LOCALE_KEY) {
            self.set_locale(l);
        }
    }
}

/// `LocaleConfig.toLocale(value)`: a `Locale` as is, anything else through `ConversionUtils.toLocale`.
pub fn to_locale(value: &JValue) -> Option<Locale> {
    if value.is_null() {
        return None;
    }
    if let Some(l) = java_compat::text::objects::as_locale(value) {
        return Some(l);
    }
    super::conversion::to_locale(&value.to_java_string())
}

/// `FormatConfig`'s state: a locale and the default format name.
pub struct FormatConfig {
    /// The `LocaleConfig` state `FormatConfig` extends.
    pub locale: LocaleConfig,
    format: Mutex<String>,
}

impl Default for FormatConfig {
    fn default() -> FormatConfig {
        FormatConfig { locale: LocaleConfig::default(), format: Mutex::new(DEFAULT_FORMAT.into()) }
    }
}

impl FormatConfig {
    /// `getFormat()`: the default format name, `default` unless configured.
    pub fn format(&self) -> String {
        self.format.lock().clone()
    }

    /// `setFormat(format)`.
    pub fn set_format(&self, format: &str) {
        *self.format.lock() = format.to_string();
    }

    /// `getLocale()`: the tool's default locale.
    pub fn locale(&self) -> Locale {
        self.locale.locale()
    }

    /// `FormatConfig.configure(values)`.
    pub fn configure(&self, values: &ValueParser) {
        self.locale.configure(values);
        if let Some(f) = values.get_string(FORMAT_KEY) {
            self.set_format(&f);
        }
    }
}

/// Tools whose configuration includes a locale.
pub trait HasLocale: Tool {
    /// The tool's `LocaleConfig` state.
    fn locale_config(&self) -> &LocaleConfig;
}

/// Tools whose configuration includes a default format.
pub trait HasFormat: HasLocale {
    /// The tool's `FormatConfig` state.
    fn format_config(&self) -> &FormatConfig;
}

/// The class name of `org.apache.velocity.tools.generic.SafeConfig`.
pub const SAFE_CONFIG: &str = "org.apache.velocity.tools.generic.SafeConfig";
/// The class name of `org.apache.velocity.tools.generic.LocaleConfig`.
pub const LOCALE_CONFIG: &str = "org.apache.velocity.tools.generic.LocaleConfig";
/// The class name of `org.apache.velocity.tools.generic.FormatConfig`.
pub const FORMAT_CONFIG: &str = "org.apache.velocity.tools.generic.FormatConfig";

/// `SafeConfig`'s public methods.
pub(crate) fn safe_config_methods<T: Tool>(def: ClassDef) -> ClassDef {
    def.method_of::<T>(SAFE_CONFIG, "configure", &[c("java.util.Map")], None, |t, _, a| {
        match &a[0] {
            JValue::Map(m) => try_configure(t, m)?,
            JValue::Null => return Err(super::npe()),
            other => match super::values::as_value_parser(other) {
                Some(vp) => try_configure(t, &vp.source_map())?,
                None => return Err(super::npe()),
            },
        }
        Ok(None)
    })
    .method_of::<T>(SAFE_CONFIG, "isConfigLocked", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.safe().is_config_locked())))
    .method_of::<T>(SAFE_CONFIG, "isSafeMode", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.safe().is_safe_mode())))
}

/// `LocaleConfig`'s public methods, then `SafeConfig`'s.
pub(crate) fn locale_config_methods<T: HasLocale>(def: ClassDef) -> ClassDef {
    let def = def
        .method_of::<T>(LOCALE_CONFIG, "getLocale", &[], Some(c("java.util.Locale")), |t, _, _| val(t.locale_config().locale().value()))
        .method_of::<T>(LOCALE_CONFIG, "toLocale", &[o()], Some(c("java.util.Locale")), |_, _, a| val(to_locale(&a[0]).map(|l| l.value()).unwrap_or(JValue::Null)));
    safe_config_methods::<T>(def)
}

/// `FormatConfig`'s public methods, then `LocaleConfig`'s.
pub(crate) fn format_config_methods<T: HasFormat>(def: ClassDef) -> ClassDef {
    let def = def.method_of::<T>(FORMAT_CONFIG, "getFormat", &[], Some(s()), |t, _, _| val(JValue::str(t.format_config().format())));
    locale_config_methods::<T>(def)
}
