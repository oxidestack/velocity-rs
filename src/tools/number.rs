//! `org.apache.velocity.tools.generic.NumberTool` (`$number`): formatting and parsing through the named styles
//! or `DecimalFormat` patterns, in the tool's (or a given) locale.

use std::sync::{Arc, Weak};

use java_compat::lang::{JError, JMap, JObject, JValue};
use java_compat::reflect::ClassDef;
use java_compat::text::Locale;

use super::classes::{Annotation, JavaClass};
use super::conversion;
use super::safe::{self, FormatConfig, HasFormat, HasLocale, LocaleConfig, SafeConfig, Tool};
use super::values::ValueParser;
use super::{c, identity, o, s, tool_object, val};

/// The class name of `org.apache.velocity.tools.generic.NumberTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.NumberTool";

/// Port of `org.apache.velocity.tools.generic.NumberTool` (`$number`): numbers formatted and parsed with `NumberFormat`
/// styles or patterns in the tool's locale.
pub struct NumberTool {
    config: FormatConfig,
    me: Weak<NumberTool>,
}

impl NumberTool {
    /// `new NumberTool()`: the `default` format in the default locale.
    pub fn new() -> Arc<NumberTool> {
        Arc::new_cyclic(|me| NumberTool { config: FormatConfig::default(), me: me.clone() })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: reads the locale and format; nothing once locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    /// `getFormat()`: the format `format(obj)` uses, `default` unless configured.
    pub fn get_format(&self) -> String {
        self.config.format()
    }

    /// `getLocale()`: the locale numbers format in.
    pub fn get_locale(&self) -> Locale {
        self.config.locale()
    }

    /// `format(obj)`: with the tool's default format.
    pub fn format(&self, obj: &JValue) -> Result<Option<String>, JError> {
        self.format_as(Some(&self.get_format()), obj)
    }

    /// `currency(obj)`: formatted as currency in the tool's locale; `None` when not a number.
    pub fn currency(&self, obj: &JValue) -> Result<Option<String>, JError> {
        self.format_as(Some("currency"), obj)
    }

    /// `integer(obj)`: formatted as an integer in the tool's locale; `None` when not a number.
    pub fn integer(&self, obj: &JValue) -> Result<Option<String>, JError> {
        self.format_as(Some("integer"), obj)
    }

    /// `number(obj)`: formatted with the locale's general number format; `None` when not a number.
    pub fn number(&self, obj: &JValue) -> Result<Option<String>, JError> {
        self.format_as(Some("number"), obj)
    }

    /// `percent(obj)`: formatted as a percentage in the tool's locale; `None` when not a number.
    pub fn percent(&self, obj: &JValue) -> Result<Option<String>, JError> {
        self.format_as(Some("percent"), obj)
    }

    /// `format(format, obj)`.
    pub fn format_as(&self, format: Option<&str>, obj: &JValue) -> Result<Option<String>, JError> {
        self.format_in(format, obj, Some(&self.get_locale()))
    }

    /// `format(format, obj, locale)`: `null` when the value is not a number (or no format / locale); an invalid
    /// pattern throws.
    pub fn format_in(&self, format: Option<&str>, obj: &JValue, locale: Option<&Locale>) -> Result<Option<String>, JError> {
        let number = self.to_number(obj);
        let nf = self.get_number_format(format, locale)?;
        match (number, nf) {
            (Some(n), Some(f)) => f.format(&n).map(Some),
            _ => Ok(None),
        }
    }

    /// `getNumberFormat(format, locale)`.
    pub fn get_number_format(&self, format: Option<&str>, locale: Option<&Locale>) -> Result<Option<java_compat::text::DecimalFormat>, JError> {
        match (format, locale) {
            (Some(f), Some(l)) => conversion::number_format(f, l).map(Some),
            _ => Ok(None),
        }
    }

    /// `toNumber(obj)`.
    pub fn to_number(&self, obj: &JValue) -> Option<JValue> {
        self.to_number_in(Some(&self.get_format()), obj, Some(&self.get_locale()))
    }

    /// `toNumber(format, obj, locale)`: `ConversionUtils.toNumber(obj, format, locale)`.
    pub fn to_number_in(&self, format: Option<&str>, obj: &JValue, locale: Option<&Locale>) -> Option<JValue> {
        if let Some(n) = conversion::to_number_handling(obj, false) {
            return Some(n);
        }
        match (format, locale) {
            (Some(f), Some(l)) => conversion::parse_number(&obj.to_java_string(), f, l),
            _ => None,
        }
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for NumberTool {
    fn safe(&self) -> &SafeConfig {
        &self.config.locale.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        self.config.configure(values);
    }
}

impl HasLocale for NumberTool {
    fn locale_config(&self) -> &LocaleConfig {
        &self.config.locale
    }
}

impl HasFormat for NumberTool {
    fn format_config(&self) -> &FormatConfig {
        &self.config
    }
}

fn text(r: Result<Option<String>, JError>) -> super::Ret {
    super::text(r?)
}

/// A `java.util.Locale` argument (`null` stays `None`).
pub(crate) fn locale_arg(v: &JValue) -> Result<Option<Locale>, JError> {
    match v {
        JValue::Null => Ok(None),
        other => java_compat::text::objects::as_locale(other)
            .map(Some)
            .ok_or_else(|| JError::new("java.lang.IllegalArgumentException", "argument type mismatch")),
    }
}

fn def() -> Arc<ClassDef> {
    type N = NumberTool;
    let locale = || c("java.util.Locale");
    let number = || c("java.lang.Number");
    let def = ClassDef::new(CLASS)
        .extends(&[safe::FORMAT_CONFIG, safe::LOCALE_CONFIG, safe::SAFE_CONFIG])
        .implements(&["java.io.Serializable"])
        .method::<N>("format", &[o()], Some(s()), |t, _, a| text(t.format(&a[0])))
        .method::<N>("currency", &[o()], Some(s()), |t, _, a| text(t.currency(&a[0])))
        .method::<N>("integer", &[o()], Some(s()), |t, _, a| text(t.integer(&a[0])))
        .method::<N>("number", &[o()], Some(s()), |t, _, a| text(t.number(&a[0])))
        .method::<N>("percent", &[o()], Some(s()), |t, _, a| text(t.percent(&a[0])))
        .method::<N>("format", &[s(), o()], Some(s()), |t, _, a| text(t.format_as(a[0].as_str(), &a[1])))
        .method::<N>("format", &[s(), o(), locale()], Some(s()), |t, _, a| text(t.format_in(a[0].as_str(), &a[1], locale_arg(&a[2])?.as_ref())))
        .method::<N>("getNumberFormat", &[s(), locale()], Some(c("java.text.NumberFormat")), |t, _, a| {
            val(t.get_number_format(a[0].as_str(), locale_arg(&a[1])?.as_ref())?.map(|f| f.into_value()).unwrap_or(JValue::Null))
        })
        .method::<N>("toNumber", &[o()], Some(number()), |t, _, a| val(t.to_number(&a[0]).unwrap_or(JValue::Null)))
        .method::<N>("toNumber", &[s(), o()], Some(number()), |t, _, a| val(t.to_number_in(a[0].as_str(), &a[1], Some(&t.get_locale())).unwrap_or(JValue::Null)))
        .method::<N>("toNumber", &[s(), o(), locale()], Some(number()), |t, _, a| val(t.to_number_in(a[0].as_str(), &a[1], locale_arg(&a[2])?.as_ref()).unwrap_or(JValue::Null)));
    super::object_methods::<N>(safe::format_config_methods::<N>(def)).register()
}

tool_object!(NumberTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || NumberTool::new().value()).annotated(Annotation::default_key("number"))
}
