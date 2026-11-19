//! The deprecated `org.apache.velocity.tools.generic.ConversionTool` (`$convert`): values to strings,
//! booleans, numbers, locales, dates, calendars and arrays of them.

use std::sync::{Arc, Weak};

use java_compat::lang::{JArray, JError, JMap, JObject, JValue};
use java_compat::reflect::{ClassDef, JType};
use java_compat::text::Locale;
use java_compat::util::TimeZone;
use parking_lot::Mutex;

use super::classes::{Annotation, JavaClass};
use super::conversion;
use super::num;
use super::safe::{self, HasLocale, LocaleConfig, SafeConfig, Tool};
use super::values::ValueParser;
use super::{c, identity, o, s, tool_object, val};

/// The class name of `org.apache.velocity.tools.generic.ConversionTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.ConversionTool";
/// `ConversionTool.STRINGS_DELIMITER_FORMAT_KEY`: the configuration key for the delimiter `toStrings` splits around.
pub const STRINGS_DELIMITER_FORMAT_KEY: &str = "stringsDelimiter";
/// `ConversionTool.DATE_FORMAT_KEY`: the configuration key for the date format strings parse with.
pub const DATE_FORMAT_KEY: &str = "dateFormat";
/// `ConversionTool.NUMBER_FORMAT_KEY`: the configuration key for the number format strings parse with.
pub const NUMBER_FORMAT_KEY: &str = "numberFormat";

struct Settings {
    delimiter: String,
    trim: bool,
    number_format: String,
    date_format: String,
}

/// Port of `org.apache.velocity.tools.generic.ConversionTool` (`$convert`, deprecated): values converted to strings,
/// booleans, numbers, locales, dates and their arrays.
pub struct ConversionTool {
    locale: LocaleConfig,
    st: Mutex<Settings>,
    me: Weak<ConversionTool>,
}

fn is_collection(v: &JValue) -> bool {
    matches!(v, JValue::List(_)) || (matches!(v, JValue::Object(_)) && java_compat::reflect::class_def(v).is_some_and(|d| d.interfaces.iter().any(|i| &**i == "java.util.Collection")))
}

impl ConversionTool {
    /// `new ConversionTool()`: delimiter `,`, trimming, the `default` number and date formats.
    pub fn new() -> Arc<ConversionTool> {
        Arc::new_cyclic(|me| ConversionTool {
            locale: LocaleConfig::default(),
            st: Mutex::new(Settings { delimiter: ",".into(), trim: true, number_format: "default".into(), date_format: "default".into() }),
            me: me.clone(),
        })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: reads the locale, delimiter, trimming and formats; nothing once locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    /// `getLocale()`: the locale numbers and dates parse with.
    pub fn get_locale(&self) -> Locale {
        self.locale.locale()
    }

    /// `getStringsDelimiter()`: the regular expression `toStrings` splits around, `,` unless configured.
    pub fn get_strings_delimiter(&self) -> String {
        self.st.lock().delimiter.clone()
    }

    /// `getStringsTrim()`: whether `toStrings` trims the parts, `true` unless configured.
    pub fn get_strings_trim(&self) -> bool {
        self.st.lock().trim
    }

    /// `getNumberFormat()`: the format strings parse as numbers with, `default` unless configured.
    pub fn get_number_format(&self) -> String {
        self.st.lock().number_format.clone()
    }

    /// `getDateFormat()`: the format strings parse as dates with, `default` unless configured.
    pub fn get_date_format(&self) -> String {
        self.st.lock().date_format.clone()
    }

    /// `toString(value)`: `ConversionUtils.toString`, `None` for `null`.
    pub fn to_string(&self, v: &JValue) -> Option<String> {
        conversion::to_string(v)
    }

    /// `toBoolean(value)`: `Boolean.valueOf` of its string.
    pub fn to_boolean(&self, v: &JValue) -> Option<bool> {
        conversion::to_boolean(v)
    }

    /// `toInteger(value)`: `NullPointerException` when it is not a number.
    pub fn to_integer(&self, v: &JValue) -> Result<Option<i32>, JError> {
        match v {
            JValue::Null => Ok(None),
            JValue::Int(i) => Ok(Some(*i)),
            other => self.to_number(other).map(|n| Some(num::int_value(&n))).ok_or_else(super::npe),
        }
    }

    /// `toDouble(value)`: `NullPointerException` when it is not a number.
    pub fn to_double(&self, v: &JValue) -> Result<Option<f64>, JError> {
        match v {
            JValue::Null => Ok(None),
            JValue::Double(d) => Ok(Some(*d)),
            other => self.to_number(other).map(|n| Some(num::double_value(&n))).ok_or_else(super::npe),
        }
    }

    /// `toNumber(value)`: a number as is, else parsed in the tool's number format.
    pub fn to_number(&self, v: &JValue) -> Option<JValue> {
        if let Some(n) = conversion::to_number_handling(v, false) {
            return Some(n);
        }
        let s = self.to_string(v).filter(|s| !s.is_empty())?;
        self.parse_number(&s, &self.get_number_format(), &JValue::Null)
    }

    /// `toLocale(value)`: a `Locale` as is, else its string as `language_COUNTRY_VARIANT`; `None` for `null` or an
    /// empty string.
    pub fn to_locale(&self, v: &JValue) -> Option<Locale> {
        if let Some(l) = java_compat::text::objects::as_locale(v) {
            return Some(l);
        }
        let s = self.to_string(v).filter(|s| !s.is_empty())?;
        conversion::to_locale(&s)
    }

    /// `toDate(value)`: a date, calendar or number, else parsed in the tool's date format.
    pub fn to_date(&self, v: &JValue) -> Option<i64> {
        if let Some(d) = conversion::to_date(v) {
            return Some(d);
        }
        let s = self.to_string(v).filter(|s| !s.is_empty())?;
        self.parse_date(&s, &self.get_date_format(), &JValue::Null, &TimeZone::default_zone())
    }

    /// `toCalendar(value)`: a calendar as is (the same object), else its date in the default zone.
    pub fn to_calendar(&self, v: &JValue) -> JValue {
        if v.is_null() {
            return JValue::Null;
        }
        if java_compat::text::objects::calendar_of(v).is_some() {
            return v.clone();
        }
        match self.to_date(v) {
            Some(d) => conversion::to_calendar(d, Some(&self.get_locale())).into_value(),
            None => JValue::Null,
        }
    }

    /// `toStrings(value)`: arrays and collections element by element, anything else split.
    pub fn to_strings(&self, v: &JValue) -> Option<Vec<Option<String>>> {
        match v {
            JValue::Null => None,
            JValue::Array(a) => Some(a.items().iter().map(|x| self.to_string(x)).collect()),
            other if is_collection(other) => {
                let items = other.iterate().unwrap_or_default();
                (!items.is_empty()).then(|| items.iter().map(|x| self.to_string(x)).collect())
            }
            other => Some(self.parse_string_list(&self.to_string(other).unwrap_or_default()).into_iter().map(Some).collect()),
        }
    }

    fn elements(&self, v: &JValue) -> Option<Vec<JValue>> {
        match v {
            JValue::Array(a) => Some(a.items()),
            JValue::Null => None,
            other => self.to_strings(other).map(|s| s.into_iter().map(|x| x.map(JValue::str).unwrap_or(JValue::Null)).collect()),
        }
    }

    /// `toBooleans(value)`: each element (or split part) as `to_boolean` converts it.
    pub fn to_booleans(&self, v: &JValue) -> Option<Vec<Option<bool>>> {
        self.elements(v).map(|e| e.iter().map(|x| self.to_boolean(x)).collect())
    }

    /// `toNumbers(value)`: each element (or split part) as `to_number` converts it, `None` where it does not.
    pub fn to_numbers(&self, v: &JValue) -> Option<Vec<Option<JValue>>> {
        self.elements(v).map(|e| e.iter().map(|x| self.to_number(x)).collect())
    }

    /// `toInts(value)`: 0 for elements that are not numbers.
    pub fn to_ints(&self, v: &JValue) -> Option<Vec<i32>> {
        self.to_numbers(v).map(|n| n.iter().map(|x| x.as_ref().map(num::int_value).unwrap_or(0)).collect())
    }

    /// `toDoubles(value)`: 0 for elements that are not numbers.
    pub fn to_doubles(&self, v: &JValue) -> Option<Vec<f64>> {
        self.to_numbers(v).map(|n| n.iter().map(|x| x.as_ref().map(num::double_value).unwrap_or(0.0)).collect())
    }

    /// `toLocales(value)`: each element (or split part) as `to_locale` converts it.
    pub fn to_locales(&self, v: &JValue) -> Option<Vec<Option<Locale>>> {
        self.elements(v).map(|e| e.iter().map(|x| self.to_locale(x)).collect())
    }

    /// `toDates(value)`: each element (or split part) as `to_date` converts it, in milliseconds since the epoch.
    pub fn to_dates(&self, v: &JValue) -> Option<Vec<Option<i64>>> {
        self.elements(v).map(|e| e.iter().map(|x| self.to_date(x)).collect())
    }

    /// `toCalendars(value)`: each element (or split part) as `to_calendar` converts it.
    pub fn to_calendars(&self, v: &JValue) -> Option<Vec<JValue>> {
        self.elements(v).map(|e| e.iter().map(|x| self.to_calendar(x)).collect())
    }

    /// `toList(value)`.
    pub fn to_list(&self, v: &JValue) -> Option<JValue> {
        (!v.is_null()).then(|| conversion::as_list(v))
    }

    /// `parseStringList(value)`: split around the delimiter (a regular expression), trimmed.
    fn parse_string_list(&self, value: &str) -> Vec<String> {
        let (d, trim) = {
            let st = self.st.lock();
            (st.delimiter.clone(), st.trim)
        };
        let values = if !value.contains(&d) { vec![value.to_string()] } else { java_compat::methods::split(value, &d, 0).unwrap_or_else(|_| vec![value.to_string()]) };
        if trim { values.into_iter().map(|v| v.trim_matches(|c: char| c <= ' ').to_string()).collect() } else { values }
    }

    /// `parseNumber(value, format, locale)`: `null` for a locale that does not convert.
    pub fn parse_number(&self, value: &str, format: &str, locale: &JValue) -> Option<JValue> {
        let l = if locale.is_null() { self.get_locale() } else { self.to_locale(locale)? };
        conversion::parse_number(value, format, &l)
    }

    /// `parseDate(value, format, locale, timezone)`.
    pub fn parse_date(&self, value: &str, format: &str, locale: &JValue, zone: &TimeZone) -> Option<i64> {
        let l = if locale.is_null() { self.get_locale() } else { self.to_locale(locale)? };
        conversion::parse_date(value, Some(format), &l, zone)
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for ConversionTool {
    fn safe(&self) -> &SafeConfig {
        &self.locale.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        self.locale.configure(values);
        let mut st = self.st.lock();
        if let Some(d) = values.get_string(STRINGS_DELIMITER_FORMAT_KEY) {
            st.delimiter = d;
        }
        if let Some(d) = values.get_string(DATE_FORMAT_KEY) {
            st.date_format = d;
        }
        if let Some(n) = values.get_string(NUMBER_FORMAT_KEY) {
            st.number_format = n;
        }
    }
}

impl HasLocale for ConversionTool {
    fn locale_config(&self) -> &LocaleConfig {
        &self.locale
    }
}

fn array(component: JType, items: Vec<JValue>) -> JValue {
    JValue::Array(Arc::new(JArray::new(component, items)))
}

fn opt<T>(v: Option<Vec<T>>, component: JType, f: impl Fn(T) -> JValue) -> super::Ret {
    val(v.map(|v| array(component, v.into_iter().map(f).collect())).unwrap_or(JValue::Null))
}

fn date(d: Option<i64>) -> JValue {
    d.map(conversion::date_value).unwrap_or(JValue::Null)
}

fn locale_value(l: Option<Locale>) -> JValue {
    l.map(|l| l.value()).unwrap_or(JValue::Null)
}

/// The `…s(Collection)` overloads: `null` unless the collection is empty (as the Java code has it), and then an
/// empty array.
fn empty_collection_only(v: &JValue, component: JType) -> super::Ret {
    let items = v.iterate().unwrap_or_default();
    if v.is_null() || !items.is_empty() {
        return val(JValue::Null);
    }
    val(array(component, Vec::new()))
}

fn def() -> Arc<ClassDef> {
    type T = ConversionTool;
    fn collection() -> JType {
        c("java.util.Collection")
    }
    fn locale() -> JType {
        c("java.util.Locale")
    }
    fn date_t() -> JType {
        c("java.util.Date")
    }
    fn cal() -> JType {
        c("java.util.Calendar")
    }
    fn number() -> JType {
        c("java.lang.Number")
    }
    fn boolean() -> JType {
        c("java.lang.Boolean")
    }
    let def = ClassDef::new(CLASS)
        .extends(&[safe::LOCALE_CONFIG, safe::SAFE_CONFIG])
        .implements(&["java.io.Serializable"])
        .method::<T>("getStringsDelimiter", &[], Some(s()), |t, _, _| val(JValue::str(t.get_strings_delimiter())))
        .method::<T>("getStringsTrim", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.get_strings_trim())))
        .method::<T>("getNumberFormat", &[], Some(s()), |t, _, _| val(JValue::str(t.get_number_format())))
        .method::<T>("getDateFormat", &[], Some(s()), |t, _, _| val(JValue::str(t.get_date_format())))
        .method::<T>("toString", &[o()], Some(s()), |t, _, a| super::text(t.to_string(&a[0])))
        .method::<T>("toBoolean", &[o()], Some(boolean()), |t, _, a| val(t.to_boolean(&a[0]).map(JValue::Bool).unwrap_or(JValue::Null)))
        .method::<T>("toInteger", &[o()], Some(c("java.lang.Integer")), |t, _, a| val(t.to_integer(&a[0])?.map(JValue::Int).unwrap_or(JValue::Null)))
        .method::<T>("toDouble", &[o()], Some(c("java.lang.Double")), |t, _, a| val(t.to_double(&a[0])?.map(JValue::Double).unwrap_or(JValue::Null)))
        .method::<T>("toNumber", &[o()], Some(number()), |t, _, a| val(t.to_number(&a[0]).unwrap_or(JValue::Null)))
        .method::<T>("toLocale", &[o()], Some(locale()), |t, _, a| val(locale_value(t.to_locale(&a[0]))))
        .method::<T>("toDate", &[o()], Some(date_t()), |t, _, a| val(date(t.to_date(&a[0]))))
        .method::<T>("toCalendar", &[o()], Some(cal()), |t, _, a| val(t.to_calendar(&a[0])))
        .method::<T>("toStrings", &[o()], Some(JType::array(s())), |t, _, a| opt(t.to_strings(&a[0]), s(), |x| x.map(JValue::str).unwrap_or(JValue::Null)))
        .method::<T>("toBooleans", &[o()], Some(JType::array(boolean())), |t, _, a| opt(t.to_booleans(&a[0]), boolean(), |x| x.map(JValue::Bool).unwrap_or(JValue::Null)))
        .method::<T>("toBooleans", &[collection()], Some(JType::array(boolean())), |_, _, a| empty_collection_only(&a[0], boolean()))
        .method::<T>("toNumbers", &[o()], Some(JType::array(number())), |t, _, a| opt(t.to_numbers(&a[0]), number(), |x| x.unwrap_or(JValue::Null)))
        .method::<T>("toNumbers", &[collection()], Some(JType::array(number())), |_, _, a| empty_collection_only(&a[0], number()))
        .method::<T>("toInts", &[o()], Some(JType::array(JType::Int)), |t, _, a| opt(t.to_ints(&a[0]), JType::Int, JValue::Int))
        .method::<T>("toIntegers", &[o()], Some(JType::array(JType::Int)), |t, _, a| opt(t.to_ints(&a[0]), JType::Int, JValue::Int))
        .method::<T>("toDoubles", &[o()], Some(JType::array(JType::Double)), |t, _, a| opt(t.to_doubles(&a[0]), JType::Double, JValue::Double))
        .method::<T>("toLocales", &[o()], Some(JType::array(locale())), |t, _, a| opt(t.to_locales(&a[0]), locale(), locale_value))
        .method::<T>("toLocales", &[collection()], Some(JType::array(locale())), |_, _, a| empty_collection_only(&a[0], locale()))
        .method::<T>("toDates", &[o()], Some(JType::array(date_t())), |t, _, a| opt(t.to_dates(&a[0]), date_t(), date))
        .method::<T>("toDates", &[collection()], Some(JType::array(date_t())), |_, _, a| empty_collection_only(&a[0], date_t()))
        .method::<T>("toCalendars", &[o()], Some(JType::array(cal())), |t, _, a| opt(t.to_calendars(&a[0]), cal(), |x| x))
        .method::<T>("toCalendars", &[collection()], Some(JType::array(cal())), |_, _, a| empty_collection_only(&a[0], cal()))
        .method::<T>("toList", &[o()], Some(c("java.util.List")), |t, _, a| val(t.to_list(&a[0]).unwrap_or(JValue::Null)))
        .method::<T>("parseNumber", &[s()], Some(number()), |t, _, a| val(t.parse_number(&a[0].to_java_string(), &t.get_number_format(), &JValue::Null).unwrap_or(JValue::Null)))
        .method::<T>("parseNumber", &[s(), s()], Some(number()), |t, _, a| val(t.parse_number(&a[0].to_java_string(), &a[1].to_java_string(), &JValue::Null).unwrap_or(JValue::Null)))
        .method::<T>("parseNumber", &[s(), o()], Some(number()), |t, _, a| val(t.parse_number(&a[0].to_java_string(), &t.get_number_format(), &a[1]).unwrap_or(JValue::Null)))
        .method::<T>("parseNumber", &[s(), s(), o()], Some(number()), |t, _, a| val(t.parse_number(&a[0].to_java_string(), &a[1].to_java_string(), &a[2]).unwrap_or(JValue::Null)))
        .method::<T>("parseDate", &[s()], Some(date_t()), |t, _, a| val(date(t.parse_date(&a[0].to_java_string(), &t.get_date_format(), &JValue::Null, &TimeZone::default_zone()))))
        .method::<T>("parseDate", &[s(), s()], Some(date_t()), |t, _, a| val(date(t.parse_date(&a[0].to_java_string(), &a[1].to_java_string(), &JValue::Null, &TimeZone::default_zone()))))
        .method::<T>("parseDate", &[s(), o()], Some(date_t()), |t, _, a| val(date(t.parse_date(&a[0].to_java_string(), &t.get_date_format(), &a[1], &TimeZone::default_zone()))))
        .method::<T>("parseDate", &[s(), s(), o()], Some(date_t()), |t, _, a| val(date(t.parse_date(&a[0].to_java_string(), &a[1].to_java_string(), &a[2], &TimeZone::default_zone()))))
        .method::<T>("parseDate", &[s(), s(), o(), c("java.util.TimeZone")], Some(date_t()), |t, _, a| {
            let zone = java_compat::text::objects::as_time_zone(&a[3]).ok_or_else(super::npe)?;
            val(date(t.parse_date(&a[0].to_java_string(), &a[1].to_java_string(), &a[2], &zone)))
        });
    super::object_methods::<T>(safe::locale_config_methods::<T>(def)).register()
}

tool_object!(ConversionTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || ConversionTool::new().value())
        .annotated(Annotation::default_key("convert"))
        .annotated(Annotation::skip_setters())
        .annotated(Annotation::deprecated())
}
