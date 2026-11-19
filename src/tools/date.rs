//! `org.apache.velocity.tools.generic.DateTool` (`$date`) and `ComparisonDateTool` (MockServer's `$date`):
//! formatting and parsing through the named styles or `SimpleDateFormat` patterns, calendar fields, and
//! human-readable comparisons of two dates (`whenIs`, `difference`, `timespan`).

use std::sync::{Arc, Weak};

use java_compat::lang::{JError, JMap, JObject, JValue};
use java_compat::reflect::{ClassDef, JType};
use java_compat::text::Locale;
use java_compat::text::objects::{as_time_zone, calendar_of};
use java_compat::util::calendar::{self as cal, GregorianCalendar};
use java_compat::util::{Date, TimeZone};
use parking_lot::Mutex;

use super::classes::{Annotation, JavaClass};
use super::conversion::{self, DateFmt};
use super::number::locale_arg;
use super::safe::{self, FormatConfig, HasFormat, HasLocale, LocaleConfig, SafeConfig, Tool};
use super::values::ValueParser;
use super::{c, o, s, tool_object, val};

/// The Java class name of `DateTool`.
pub const DATE_TOOL: &str = "org.apache.velocity.tools.generic.DateTool";
/// The Java class name of `ComparisonDateTool`.
pub const COMPARISON_DATE_TOOL: &str = "org.apache.velocity.tools.generic.ComparisonDateTool";
/// The Java class name of `ComparisonDateTool.Comparison`.
pub const COMPARISON: &str = "org.apache.velocity.tools.generic.ComparisonDateTool$Comparison";
/// The configuration key of the tool's time zone ID.
pub const TIMEZONE_KEY: &str = "timezone";

/// `Calendar`'s public `int` constants, by name (`getValue(field)` reads them reflectively).
pub fn calendar_constant(name: &str) -> Option<i32> {
    Some(match name {
        "ERA" | "JANUARY" | "AM" | "ALL_STYLES" => 0,
        "YEAR" | "SUNDAY" | "FEBRUARY" | "PM" | "SHORT" | "SHORT_FORMAT" => 1,
        "MONTH" | "MONDAY" | "MARCH" | "LONG" | "LONG_FORMAT" => 2,
        "WEEK_OF_YEAR" | "TUESDAY" | "APRIL" => 3,
        "WEEK_OF_MONTH" | "WEDNESDAY" | "MAY" | "NARROW_FORMAT" => 4,
        "DATE" | "DAY_OF_MONTH" | "THURSDAY" | "JUNE" => 5,
        "DAY_OF_YEAR" | "FRIDAY" | "JULY" => 6,
        "DAY_OF_WEEK" | "SATURDAY" | "AUGUST" => 7,
        "DAY_OF_WEEK_IN_MONTH" | "SEPTEMBER" => 8,
        "AM_PM" | "OCTOBER" => 9,
        "HOUR" | "NOVEMBER" => 10,
        "HOUR_OF_DAY" | "DECEMBER" => 11,
        "MINUTE" | "UNDECIMBER" => 12,
        "SECOND" => 13,
        "MILLISECOND" => 14,
        "ZONE_OFFSET" => 15,
        "DST_OFFSET" => 16,
        "FIELD_COUNT" => 17,
        "SHORT_STANDALONE" => 32769,
        "LONG_STANDALONE" => 32770,
        "NARROW_STANDALONE" => 32772,
        _ => return None,
    })
}

/// `DateTool`'s state and behaviour, which `ComparisonDateTool` inherits.
pub struct DateCore {
    config: FormatConfig,
    zone: Mutex<TimeZone>,
}

impl Default for DateCore {
    fn default() -> DateCore {
        DateCore { config: FormatConfig::default(), zone: Mutex::new(TimeZone::default_zone()) }
    }
}

fn now() -> i64 {
    super::now_millis()
}

impl DateCore {
    /// Reads the format, locale and `timezone` settings from the configuration.
    pub fn configure_values(&self, values: &ValueParser) {
        self.config.configure(values);
        if let Some(id) = values.get_string(TIMEZONE_KEY) {
            self.set_time_zone(TimeZone::get(&id));
        }
    }

    /// `getFormat()`: the configured default format, a style name or a pattern.
    pub fn get_format(&self) -> String {
        self.config.format()
    }

    /// `getLocale()`: the configured locale.
    pub fn get_locale(&self) -> Locale {
        self.config.locale()
    }

    /// `setLocale(locale)`.
    pub fn set_locale(&self, l: Locale) {
        self.config.locale.set_locale(l);
    }

    /// `setFormat(format)`: a style name (`default`, `short`, `medium`, …) or a `SimpleDateFormat` pattern.
    pub fn set_format(&self, f: &str) {
        self.config.set_format(f);
    }

    /// `getTimeZone()`: the configured zone, the system default unless set.
    pub fn get_time_zone(&self) -> TimeZone {
        self.zone.lock().clone()
    }

    /// `setTimeZone(zone)`.
    pub fn set_time_zone(&self, zone: TimeZone) {
        *self.zone.lock() = zone;
    }

    /// `getCalendar()`: now, in the tool's zone and locale.
    pub fn get_calendar(&self) -> GregorianCalendar {
        GregorianCalendar::new(now(), &self.get_time_zone(), &self.get_locale())
    }

    /// `getDate()`: now.
    pub fn get_date(&self) -> i64 {
        now()
    }

    /// `getValue(Object field, Object date)`: `field` names a `Calendar` constant (an `Integer` never does).
    pub fn get_value_named(&self, field: &JValue, date: &JValue) -> Result<Option<i32>, JError> {
        if field.is_null() {
            return Ok(None);
        }
        let Some(f) = calendar_constant(&field.to_java_string().to_uppercase()) else { return Ok(None) };
        self.get_value(f, date)
    }

    /// `getValue(int field, Object date)`.
    pub fn get_value(&self, field: i32, date: &JValue) -> Result<Option<i32>, JError> {
        let Some(mut c) = self.to_calendar(date) else { return Ok(None) };
        if !(0..cal::FIELD_COUNT as i32).contains(&field) {
            return Err(JError::new("java.lang.ArrayIndexOutOfBoundsException", format!("Index {field} out of bounds for length {}", cal::FIELD_COUNT)));
        }
        Ok(Some(c.get(field as usize)))
    }

    /// `getYear(date)` / `getMonth(date)` / `getDay(date)`.
    pub fn get_year(&self, date: &JValue) -> Option<i32> {
        self.get_value(cal::YEAR as i32, date).ok().flatten()
    }

    /// `getMonth(date)`: zero-based; `None` when `date` is not a date.
    pub fn get_month(&self, date: &JValue) -> Option<i32> {
        self.get_value(cal::MONTH as i32, date).ok().flatten()
    }

    /// `getDay(date)`: the day of the month; `None` when `date` is not a date.
    pub fn get_day(&self, date: &JValue) -> Option<i32> {
        self.get_value(cal::DAY_OF_MONTH as i32, date).ok().flatten()
    }

    /// `format(obj)`: in the tool's format.
    pub fn format(&self, obj: &JValue) -> Result<Option<String>, JError> {
        self.format_as(Some(&self.get_format()), obj)
    }

    /// `format(format, obj)`: in the tool's locale and zone; `None` when the value is not a date.
    pub fn format_as(&self, format: Option<&str>, obj: &JValue) -> Result<Option<String>, JError> {
        self.format_in(format, obj, &self.get_locale(), &self.get_time_zone())
    }

    /// `format(format, obj, locale, timezone)`: `null` when the value is not a date; bad patterns throw.
    pub fn format_in(&self, format: Option<&str>, obj: &JValue, locale: &Locale, zone: &TimeZone) -> Result<Option<String>, JError> {
        let date = self.to_date(obj);
        let df = conversion::date_format(format, locale, zone)?;
        Ok(match (date, df) {
            (Some(d), Some(mut f)) => Some(f.format(d)),
            _ => None,
        })
    }

    /// `format(dateStyle, timeStyle, obj, locale, timezone)`.
    pub fn format_styles(&self, date_style: Option<&str>, time_style: Option<&str>, obj: &JValue, locale: &Locale, zone: &TimeZone) -> Result<Option<String>, JError> {
        let date = self.to_date(obj);
        let df = conversion::date_format_named(date_style, time_style, locale, zone)?;
        Ok(match (date, df) {
            (Some(d), Some(mut f)) => Some(f.format(d)),
            _ => None,
        })
    }

    /// `toDate(obj)`: in the tool's format, locale and zone.
    pub fn to_date(&self, obj: &JValue) -> Option<i64> {
        self.to_date_in(Some(&self.get_format()), obj, &self.get_locale(), &self.get_time_zone())
    }

    /// `toDate(format, obj, locale, timezone)`: a date value as is, else parsed with `format`; the epoch millis, `None` when it does not parse.
    pub fn to_date_in(&self, format: Option<&str>, obj: &JValue, locale: &Locale, zone: &TimeZone) -> Option<i64> {
        if let Some(d) = conversion::to_date(obj) {
            return Some(d);
        }
        if obj.is_null() {
            return conversion::parse_date("null", format, locale, zone);
        }
        conversion::parse_date(&obj.to_java_string(), format, locale, zone)
    }

    /// `toCalendar(obj)`: a calendar as it is, anything else through `toDate` into the default zone.
    pub fn to_calendar(&self, obj: &JValue) -> Option<GregorianCalendar> {
        self.to_calendar_in(Some(&self.get_format()), obj, Some(&self.get_locale()), &self.get_time_zone())
    }

    /// `toCalendar(format, obj, locale, timezone)`: `None` for `null` or a value that is not a date.
    pub fn to_calendar_in(&self, format: Option<&str>, obj: &JValue, locale: Option<&Locale>, zone: &TimeZone) -> Option<GregorianCalendar> {
        if obj.is_null() {
            return None;
        }
        if let Some(c) = calendar_of(obj) {
            return Some(c);
        }
        let locale = locale.cloned().unwrap_or_else(|| self.get_locale());
        let date = self.to_date_in(format, obj, &locale, zone)?;
        Some(conversion::to_calendar(date, Some(&locale)))
    }

    /// The calendar value `toCalendar` returns: the argument itself when it is one.
    fn to_calendar_value(&self, format: Option<&str>, obj: &JValue, locale: Option<&Locale>, zone: &TimeZone) -> JValue {
        if calendar_of(obj).is_some() {
            return obj.clone();
        }
        self.to_calendar_in(format, obj, locale, zone).map(|c| c.into_value()).unwrap_or(JValue::Null)
    }

    /// `toLocalizedPattern(format, locale)`: `null` for formats that are not `SimpleDateFormat`s.
    pub fn to_localized_pattern(&self, format: Option<&str>, locale: &Locale) -> Result<Option<String>, JError> {
        let df = conversion::date_format(format, locale, &self.get_time_zone())?;
        match df {
            Some(DateFmt::Simple(f)) => f.to_localized_pattern().map(Some),
            _ => Ok(None),
        }
    }

    /// `getDateFormat(format, locale, timezone)` as a value.
    fn date_format_value(df: Option<DateFmt>) -> JValue {
        match df {
            Some(DateFmt::Simple(f)) => f.into_value(),
            Some(DateFmt::TzSuffix(f)) => f.into_value(),
            None => JValue::Null,
        }
    }

    fn java_string(&self) -> String {
        self.format_as(Some(&self.get_format()), &Date::value(now())).ok().flatten().unwrap_or_else(|| "null".into())
    }
}

/// Port of `org.apache.velocity.tools.generic.DateTool`; its methods are `DateCore`'s, through `Deref`.
pub struct DateTool {
    core: DateCore,
    me: Weak<DateTool>,
}

impl std::ops::Deref for DateTool {
    type Target = DateCore;
    fn deref(&self) -> &DateCore {
        &self.core
    }
}

impl DateTool {
    /// A tool in the default format, locale and zone.
    pub fn new() -> Arc<DateTool> {
        Arc::new_cyclic(|me| DateTool { core: DateCore::default(), me: me.clone() })
    }

    /// This tool as a template value, `Null` once it is dropped.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `configure(params)`: the safe-mode, locale, format and time zone settings.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    fn java_string(&self) -> String {
        self.core.java_string()
    }
}

impl Tool for DateTool {
    fn safe(&self) -> &SafeConfig {
        &self.core.config.locale.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        self.core.configure_values(values);
    }
}

impl HasLocale for DateTool {
    fn locale_config(&self) -> &LocaleConfig {
        &self.core.config.locale
    }
}

impl HasFormat for DateTool {
    fn format_config(&self) -> &FormatConfig {
        &self.core.config
    }
}

fn zone_arg(v: &JValue) -> Result<TimeZone, JError> {
    as_time_zone(v).ok_or_else(super::npe)
}

fn locale_or_npe(v: &JValue) -> Result<Locale, JError> {
    locale_arg(v)?.ok_or_else(super::npe)
}

fn opt_int(v: Option<i32>) -> super::Ret {
    val(v.map(JValue::Int).unwrap_or(JValue::Null))
}

fn opt_date(v: Option<i64>) -> super::Ret {
    val(v.map(Date::value).unwrap_or(JValue::Null))
}

/// `DateTool`'s public methods for a type built on [`DateCore`].
fn date_methods<T: std::ops::Deref<Target = DateCore> + HasFormat + JObject>(def: ClassDef) -> ClassDef {
    let date = || c("java.util.Date");
    let calendar = || c("java.util.Calendar");
    let locale = || c("java.util.Locale");
    let zone = || c("java.util.TimeZone");
    let integer = || c("java.lang.Integer");
    let d = DATE_TOOL;
    let def = def
        .raw_method(static_method(d, "getSystemTime", JType::Long, |_| val(JValue::Long(now()))))
        .raw_method(static_method(d, "getSystemDate", date(), |_| val(Date::value(now()))))
        .raw_method(static_method(d, "getSystemCalendar", calendar(), |_| val(GregorianCalendar::new(now(), &TimeZone::default_zone(), &super::default_locale()).into_value())))
        .method_of::<T>(d, "getTimeZone", &[], Some(zone()), |t, _, _| val(t.get_time_zone().value()))
        .method_of::<T>(d, "getDate", &[], Some(date()), |t, _, _| val(Date::value(t.get_date())))
        .method_of::<T>(d, "getCalendar", &[], Some(calendar()), |t, _, _| val(t.get_calendar().into_value()))
        .method_of::<T>(d, "getYear", &[], Some(integer()), |t, _, _| opt_int(t.get_value(cal::YEAR as i32, &t.get_calendar().into_value())?))
        .method_of::<T>(d, "getYear", &[o()], Some(integer()), |t, _, a| opt_int(t.get_value(cal::YEAR as i32, &a[0])?))
        .method_of::<T>(d, "getMonth", &[], Some(integer()), |t, _, _| opt_int(t.get_value(cal::MONTH as i32, &t.get_calendar().into_value())?))
        .method_of::<T>(d, "getMonth", &[o()], Some(integer()), |t, _, a| opt_int(t.get_value(cal::MONTH as i32, &a[0])?))
        .method_of::<T>(d, "getDay", &[], Some(integer()), |t, _, _| opt_int(t.get_value(cal::DAY_OF_MONTH as i32, &t.get_calendar().into_value())?))
        .method_of::<T>(d, "getDay", &[o()], Some(integer()), |t, _, a| opt_int(t.get_value(cal::DAY_OF_MONTH as i32, &a[0])?))
        .method_of::<T>(d, "getValue", &[o()], Some(integer()), |t, _, a| opt_int(t.get_value_named(&a[0], &t.get_calendar().into_value())?))
        .method_of::<T>(d, "getValue", &[o(), o()], Some(integer()), |t, _, a| opt_int(t.get_value_named(&a[0], &a[1])?))
        .method_of::<T>(d, "getValue", &[JType::Int, o()], Some(integer()), |t, _, a| opt_int(t.get_value(super::num::int_value(&a[0]), &a[1])?))
        .method_of::<T>(d, "get", &[s()], Some(s()), |t, _, a| super::text(t.format_as(a[0].as_str(), &Date::value(t.get_date()))?))
        .method_of::<T>(d, "get", &[s(), s()], Some(s()), |t, _, a| super::text(t.format_styles(a[0].as_str(), a[1].as_str(), &Date::value(t.get_date()), &t.get_locale(), &t.get_time_zone())?))
        .method_of::<T>(d, "format", &[o()], Some(s()), |t, _, a| super::text(t.format(&a[0])?))
        .method_of::<T>(d, "format", &[s(), o()], Some(s()), |t, _, a| super::text(t.format_as(a[0].as_str(), &a[1])?))
        .method_of::<T>(d, "format", &[s(), o(), locale()], Some(s()), |t, _, a| super::text(t.format_in(a[0].as_str(), &a[1], &locale_or_npe(&a[2])?, &t.get_time_zone())?))
        .method_of::<T>(d, "format", &[s(), o(), locale(), zone()], Some(s()), |t, _, a| super::text(t.format_in(a[0].as_str(), &a[1], &locale_or_npe(&a[2])?, &zone_arg(&a[3])?)?))
        .method_of::<T>(d, "format", &[s(), s(), o()], Some(s()), |t, _, a| super::text(t.format_styles(a[0].as_str(), a[1].as_str(), &a[2], &t.get_locale(), &t.get_time_zone())?))
        .method_of::<T>(d, "format", &[s(), s(), o(), locale()], Some(s()), |t, _, a| super::text(t.format_styles(a[0].as_str(), a[1].as_str(), &a[2], &locale_or_npe(&a[3])?, &t.get_time_zone())?))
        .method_of::<T>(d, "format", &[s(), s(), o(), locale(), zone()], Some(s()), |t, _, a| super::text(t.format_styles(a[0].as_str(), a[1].as_str(), &a[2], &locale_or_npe(&a[3])?, &zone_arg(&a[4])?)?))
        .method_of::<T>(d, "getDateFormat", &[s(), locale(), zone()], Some(c("java.text.DateFormat")), |_, _, a| {
            val(DateCore::date_format_value(conversion::date_format(a[0].as_str(), &locale_or_npe(&a[1])?, &zone_arg(&a[2])?)?))
        })
        .method_of::<T>(d, "getDateFormat", &[s(), s(), locale(), zone()], Some(c("java.text.DateFormat")), |_, _, a| {
            val(DateCore::date_format_value(conversion::date_format_named(a[0].as_str(), a[1].as_str(), &locale_or_npe(&a[2])?, &zone_arg(&a[3])?)?))
        })
        .method_of::<T>(d, "toDate", &[o()], Some(date()), |t, _, a| opt_date(t.to_date(&a[0])))
        .method_of::<T>(d, "toDate", &[o(), locale()], Some(date()), |t, _, a| opt_date(t.to_date_in(Some(&t.get_format()), &a[0], &locale_or_npe(&a[1])?, &t.get_time_zone())))
        .method_of::<T>(d, "toDate", &[o(), locale(), zone()], Some(date()), |t, _, a| opt_date(t.to_date_in(Some(&t.get_format()), &a[0], &locale_or_npe(&a[1])?, &t.get_time_zone())))
        .method_of::<T>(d, "toDate", &[s(), o()], Some(date()), |t, _, a| opt_date(t.to_date_in(a[0].as_str(), &a[1], &t.get_locale(), &t.get_time_zone())))
        .method_of::<T>(d, "toDate", &[s(), o(), locale()], Some(date()), |t, _, a| opt_date(t.to_date_in(a[0].as_str(), &a[1], &locale_or_npe(&a[2])?, &t.get_time_zone())))
        .method_of::<T>(d, "toDate", &[s(), o(), locale(), zone()], Some(date()), |t, _, a| opt_date(t.to_date_in(a[0].as_str(), &a[1], &locale_or_npe(&a[2])?, &zone_arg(&a[3])?)))
        .method_of::<T>(d, "toCalendar", &[o()], Some(calendar()), |t, _, a| val(t.to_calendar_value(Some(&t.get_format()), &a[0], Some(&t.get_locale()), &t.get_time_zone())))
        .method_of::<T>(d, "toCalendar", &[o(), locale()], Some(calendar()), |t, _, a| val(t.to_calendar_value(Some(&t.get_format()), &a[0], locale_arg(&a[1])?.as_ref(), &t.get_time_zone())))
        .method_of::<T>(d, "toCalendar", &[s(), o()], Some(calendar()), |t, _, a| val(t.to_calendar_value(a[0].as_str(), &a[1], Some(&t.get_locale()), &t.get_time_zone())))
        .method_of::<T>(d, "toCalendar", &[s(), o(), locale()], Some(calendar()), |t, _, a| val(t.to_calendar_value(a[0].as_str(), &a[1], locale_arg(&a[2])?.as_ref(), &t.get_time_zone())))
        .method_of::<T>(d, "toCalendar", &[s(), o(), locale(), zone()], Some(calendar()), |t, _, a| val(t.to_calendar_value(a[0].as_str(), &a[1], locale_arg(&a[2])?.as_ref(), &zone_arg(&a[3])?)))
        .method_of::<T>(d, "toLocalizedPattern", &[s(), locale()], Some(s()), |t, _, a| super::text(t.to_localized_pattern(a[0].as_str(), &locale_or_npe(&a[1])?)?))
        .method_of::<T>(d, "toString", &[], Some(s()), |t, _, _| val(JValue::str(t.java_string())));
    super::object_methods::<T>(safe::format_config_methods::<T>(def))
}

/// A public static method (callable on an instance, as Velocity allows).
fn static_method(declaring: &str, name: &str, returns: JType, f: impl Fn(&[JValue]) -> super::Ret + Send + Sync + 'static) -> java_compat::reflect::JMethod {
    java_compat::reflect::JMethod { name: name.into(), params: Vec::new(), returns: Some(returns), declaring_class: declaring.into(), is_static: true, varargs: false, invoke: Arc::new(move |_, a| f(a)) }
}

fn date_tool_def() -> Arc<ClassDef> {
    let def = ClassDef::new(DATE_TOOL).extends(&[safe::FORMAT_CONFIG, safe::LOCALE_CONFIG, safe::SAFE_CONFIG]).implements(&["java.io.Serializable"]);
    date_methods::<DateTool>(def).register()
}

tool_object!(DateTool, date_tool_def);

pub(crate) fn date_tool_class() -> JavaClass {
    JavaClass::tool(date_tool_def(), || DateTool::new().value()).annotated(Annotation::default_key("date"))
}

// ---------------------------------------------------------------------------------------------------------
// ComparisonDateTool

/// Milliseconds in a second.
pub const MILLIS_PER_SECOND: i64 = 1000;
/// Milliseconds in a minute.
pub const MILLIS_PER_MINUTE: i64 = 60 * MILLIS_PER_SECOND;
/// Milliseconds in an hour.
pub const MILLIS_PER_HOUR: i64 = 60 * MILLIS_PER_MINUTE;
/// Milliseconds in a day.
pub const MILLIS_PER_DAY: i64 = 24 * MILLIS_PER_HOUR;
/// Milliseconds in a week.
pub const MILLIS_PER_WEEK: i64 = 7 * MILLIS_PER_DAY;
/// Milliseconds in a 30-day month.
pub const MILLIS_PER_MONTH: i64 = 30 * MILLIS_PER_DAY;
/// Milliseconds in a 365-day year.
pub const MILLIS_PER_YEAR: i64 = 365 * MILLIS_PER_DAY;
/// The configuration key of the resource bundle the comparison texts come from.
pub const BUNDLE_NAME_KEY: &str = "bundle";
/// The configuration key of how many units a comparison prints (default 1).
pub const DEPTH_KEY: &str = "depth";
/// The configuration key of the time units comparisons leave out.
pub const SKIPPED_UNITS_KEY: &str = "skip";
/// The default resource bundle of the comparison texts.
pub const DEFAULT_BUNDLE_NAME: &str = "org.apache.velocity.tools.generic.times";

const TIME_UNITS: [(&str, i64); 8] = [
    ("millisecond", 1),
    ("second", MILLIS_PER_SECOND),
    ("minute", MILLIS_PER_MINUTE),
    ("hour", MILLIS_PER_HOUR),
    ("day", MILLIS_PER_DAY),
    ("week", MILLIS_PER_WEEK),
    ("month", MILLIS_PER_MONTH),
    ("year", MILLIS_PER_YEAR),
];

const CURRENT_TYPE: i32 = 0;
const RELATIVE_TYPE: i32 = 1;
const DIFF_TYPE: i32 = 2;
const EXACT_TYPE: i32 = 3;

const CALENDAR_FIELDS: [usize; 7] = [cal::YEAR, cal::MONTH, cal::DAY_OF_MONTH, cal::HOUR_OF_DAY, cal::MINUTE, cal::SECOND, cal::MILLISECOND];
const FIELD_MAXIMA: [i32; 7] = [0, 12, 0, 24, 60, 60, 1000];
const UNIT_KEYS: [&str; 7] = ["year", "month", "day", "hour", "minute", "second", "millisecond"];

struct ComparisonConfig {
    bundle_name: String,
    default_bundle: Option<Arc<super::classpath::Bundle>>,
    time_units: Vec<(String, i64)>,
    depth: i32,
}

/// Port of `org.apache.velocity.tools.generic.ComparisonDateTool`: `DateTool` (through `Deref` to `DateCore`) plus human-readable comparisons.
pub struct ComparisonDateTool {
    core: DateCore,
    cmp: Mutex<ComparisonConfig>,
    me: Weak<ComparisonDateTool>,
}

impl std::ops::Deref for ComparisonDateTool {
    type Target = DateCore;
    fn deref(&self) -> &DateCore {
        &self.core
    }
}

/// `toYears(ms)`: whole 365-day years.
pub fn to_years(ms: i64) -> i64 {
    ms / MILLIS_PER_YEAR
}

/// `toMonths(ms)`: whole 30-day months.
pub fn to_months(ms: i64) -> i64 {
    ms / MILLIS_PER_MONTH
}

/// `toWeeks(ms)`: whole weeks.
pub fn to_weeks(ms: i64) -> i64 {
    ms / MILLIS_PER_WEEK
}

/// `toDays(ms)`: whole days.
pub fn to_days(ms: i64) -> i64 {
    ms / MILLIS_PER_DAY
}

/// `toHours(ms)`: whole hours.
pub fn to_hours(ms: i64) -> i64 {
    ms / MILLIS_PER_HOUR
}

/// `toMinutes(ms)`: whole minutes.
pub fn to_minutes(ms: i64) -> i64 {
    ms / MILLIS_PER_MINUTE
}

/// `toSeconds(ms)`: whole seconds.
pub fn to_seconds(ms: i64) -> i64 {
    ms / MILLIS_PER_SECOND
}

impl ComparisonDateTool {
    /// A tool with the default bundle, every time unit and a depth of 1.
    pub fn new() -> Arc<ComparisonDateTool> {
        Arc::new_cyclic(|me| ComparisonDateTool {
            core: DateCore::default(),
            cmp: Mutex::new(ComparisonConfig {
                bundle_name: DEFAULT_BUNDLE_NAME.into(),
                default_bundle: None,
                time_units: TIME_UNITS.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
                depth: 1,
            }),
            me: me.clone(),
        })
    }

    /// This tool as a template value, `Null` once it is dropped.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `configure(params)`: the `DateTool` settings plus bundle, depth and skipped units.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    /// `getText(key, locale)`: from the tool's bundle, `???key???` when missing.
    fn get_text(&self, key: &str, locale: Option<&Locale>) -> String {
        let default_locale = self.get_locale();
        let bundle = match locale {
            Some(l) if *l != default_locale => {
                let name = self.cmp.lock().bundle_name.clone();
                super::classpath::get_bundle(&name, l).ok()
            }
            _ => {
                let mut c = self.cmp.lock();
                if c.default_bundle.is_none() {
                    c.default_bundle = super::classpath::get_bundle(&c.bundle_name, &default_locale).ok();
                }
                c.default_bundle.clone()
            }
        };
        bundle.and_then(|b| b.get_string(key)).unwrap_or_else(|| format!("???{key}???"))
    }

    /// `whenIs(then)`: from now.
    pub fn when_is(&self, then: &JValue) -> Option<Comparison> {
        self.compare(&self.get_calendar().into_value(), then, CURRENT_TYPE)
    }

    /// `whenIs(now, then)`: `then` relative to `now`; `None` when either is not a date.
    pub fn when_is_between(&self, now: &JValue, then: &JValue) -> Option<Comparison> {
        self.compare(now, then, RELATIVE_TYPE)
    }

    /// `difference(now, then)`: the difference without a direction word, negative when `then` is earlier; `None` when either is not a date.
    pub fn difference(&self, now: &JValue, then: &JValue) -> Option<Comparison> {
        self.compare(now, then, DIFF_TYPE)
    }

    /// `timespan(now, then)`: the exact calendar difference field by field; `None` when either is not a date.
    pub fn timespan(&self, now: &JValue, then: &JValue) -> Option<Comparison> {
        self.compare(now, then, EXACT_TYPE)
    }

    fn compare(&self, now: &JValue, then: &JValue, kind: i32) -> Option<Comparison> {
        let cal_then = self.to_calendar(then);
        let cal_now = self.to_calendar(now);
        match (cal_now, cal_then) {
            (Some(n), Some(t)) => {
                let depth = self.cmp.lock().depth;
                Some(Comparison::new(self.me.upgrade()?, n, t, kind, depth, false, None))
            }
            _ => {
                let name = |v: &JValue| if v.is_null() { "null".to_string() } else { v.class_name() };
                super::log::warn(COMPARISON_DATE_TOOL, &format!("cannot compare date objects {} and {}", name(now), name(then)));
                None
            }
        }
    }

    fn java_string(&self) -> String {
        self.core.java_string()
    }
}

impl Tool for ComparisonDateTool {
    fn safe(&self) -> &SafeConfig {
        &self.core.config.locale.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        self.core.configure_values(values);
        let mut c = self.cmp.lock();
        if let Some(b) = values.get_string(BUNDLE_NAME_KEY) {
            c.bundle_name = b;
        }
        c.depth = values.get_int(DEPTH_KEY, 1);
        if let Some(skip) = values.get_strings(SKIPPED_UNITS_KEY) {
            let mut units: Vec<(String, i64)> = TIME_UNITS.iter().map(|(k, v)| (k.to_string(), *v)).collect();
            for s in skip.into_iter().flatten() {
                units.retain(|(k, _)| *k != s);
            }
            c.time_units = units;
        }
    }
}

impl HasLocale for ComparisonDateTool {
    fn locale_config(&self) -> &LocaleConfig {
        &self.core.config.locale
    }
}

impl HasFormat for ComparisonDateTool {
    fn format_config(&self) -> &FormatConfig {
        &self.core.config
    }
}

/// `ComparisonDateTool.Comparison`.
#[derive(Clone)]
pub struct Comparison {
    tool: Arc<ComparisonDateTool>,
    now: GregorianCalendar,
    then: GregorianCalendar,
    milliseconds: i64,
    exact: Arc<Mutex<Option<[i32; 7]>>>,
    kind: i32,
    max_unit_depth: i32,
    abbreviate: bool,
    locale: Option<Locale>,
}

impl Comparison {
    fn new(tool: Arc<ComparisonDateTool>, mut now: GregorianCalendar, mut then: GregorianCalendar, kind: i32, depth: i32, abbr: bool, locale: Option<Locale>) -> Comparison {
        let milliseconds = then.time_in_millis() - now.time_in_millis();
        Comparison { tool, now, then, milliseconds, exact: Arc::new(Mutex::new(None)), kind, max_unit_depth: depth, abbreviate: abbr, locale }
    }

    fn with(&self, kind: i32, depth: i32, abbr: bool, locale: Option<Locale>) -> Comparison {
        Comparison::new(self.tool.clone(), self.now.clone(), self.then.clone(), kind, depth, abbr, locale)
    }

    /// `abbr(abbr)`: a copy printing abbreviated unit names or not.
    pub fn abbr(&self, abbr: bool) -> Comparison {
        self.with(self.kind, self.max_unit_depth, abbr, self.locale.clone())
    }

    /// `depth(depth)`: a copy with this maximum unit depth.
    pub fn depth(&self, depth: i32) -> Comparison {
        self.with(self.kind, depth, self.abbreviate, self.locale.clone())
    }

    /// `locale(locale)`: a copy printing in this locale, `None` for the tool's.
    pub fn locale(&self, locale: Option<Locale>) -> Comparison {
        self.with(self.kind, self.max_unit_depth, self.abbreviate, locale)
    }

    /// `getExactDifference()`: field by field, borrowing like a subtraction by hand.
    fn exact_difference(&self) -> [i32; 7] {
        let mut cached = self.exact.lock();
        if let Some(e) = *cached {
            return e;
        }
        let (mut from, mut to) = (self.now.clone(), self.then.clone());
        if self.milliseconds < 0 {
            std::mem::swap(&mut from, &mut to);
        }
        let mut maxima = FIELD_MAXIMA;
        let mut tmp = to.clone();
        let _ = tmp.add(cal::MONTH, -1);
        maxima[2] = tmp.actual_maximum(cal::DAY_OF_MONTH);
        let mut exact = [0i32; 7];
        let mut carry = 0;
        for i in (1..CALENDAR_FIELDS.len()).rev() {
            let start = from.get(CALENDAR_FIELDS[i]);
            let end = to.get(CALENDAR_FIELDS[i]);
            let mut diff = end - (start + carry);
            if diff < 0 {
                diff += maxima[i];
                carry = 1;
            } else {
                carry = 0;
            }
            exact[i] = diff;
        }
        exact[0] = to.get(cal::YEAR) - (from.get(cal::YEAR) + carry);
        *cached = Some(exact);
        exact
    }

    fn part(&self, i: usize, relative: fn(i64) -> i64) -> i64 {
        if self.kind == EXACT_TYPE { self.exact_difference()[i] as i64 } else { relative(self.milliseconds) }
    }

    /// `getYears()`: whole years between the dates (the year field difference in exact mode).
    pub fn get_years(&self) -> i64 {
        self.part(0, to_years)
    }

    /// `getMonths()`: whole months between the dates (the month field difference in exact mode).
    pub fn get_months(&self) -> i64 {
        self.part(1, to_months)
    }

    /// `getWeeks()`: always relative.
    pub fn get_weeks(&self) -> i64 {
        if self.kind == EXACT_TYPE {
            super::log::warn(COMPARISON_DATE_TOOL, "Requesting weeks difference in exact mode still returns the totall relative number of weeks");
        }
        to_weeks(self.milliseconds)
    }

    /// `getDays()`: whole days between the dates (the day field difference in exact mode).
    pub fn get_days(&self) -> i64 {
        self.part(2, to_days)
    }

    /// `getHours()`: whole hours between the dates (the hour field difference in exact mode).
    pub fn get_hours(&self) -> i64 {
        self.part(3, to_hours)
    }

    /// `getMinutes()`: whole minutes between the dates (the minute field difference in exact mode).
    pub fn get_minutes(&self) -> i64 {
        self.part(4, to_minutes)
    }

    /// `getSeconds()`: whole seconds between the dates (the second field difference in exact mode).
    pub fn get_seconds(&self) -> i64 {
        self.part(5, to_seconds)
    }

    /// `getMilliseconds()`: the milliseconds between the dates (the millisecond field difference in exact mode).
    pub fn get_milliseconds(&self) -> i64 {
        self.part(6, |ms| ms)
    }

    /// `getFull()`: a copy whose unit depth is the number of configured units.
    pub fn get_full(&self) -> Comparison {
        let n = self.tool.cmp.lock().time_units.len() as i32;
        self.depth(n)
    }

    /// `getDifference()`: a copy printing the difference without a direction word.
    pub fn get_difference(&self) -> Comparison {
        self.with(DIFF_TYPE, self.max_unit_depth, self.abbreviate, self.locale.clone())
    }

    /// `getRelative()`: a copy printing `then` relative to `now` (`before`/`after` texts).
    pub fn get_relative(&self) -> Comparison {
        self.with(RELATIVE_TYPE, self.max_unit_depth, self.abbreviate, self.locale.clone())
    }

    /// `getExact()`: a relative comparison, as the Java code has it.
    pub fn get_exact(&self) -> Comparison {
        self.with(RELATIVE_TYPE, self.max_unit_depth, self.abbreviate, self.locale.clone())
    }

    /// `getAbbr()`: a copy printing abbreviated unit names.
    pub fn get_abbr(&self) -> Comparison {
        self.abbr(true)
    }

    fn text(&self, key: &str) -> String {
        self.tool.get_text(key, self.locale.as_ref())
    }

    /// `toString(diff, maxUnitDepth)`.
    fn to_string_depth(&self, diff: i64, max_unit_depth: i32) -> Option<String> {
        let units = self.tool.cmp.lock().time_units.clone();
        let max_unit_depth = max_unit_depth.min(units.len() as i32);
        let mut value: i64 = 0;
        let mut remainder: i64;
        let mut unit_key: String;
        if self.kind == EXACT_TYPE {
            let exact = self.exact_difference();
            unit_key = String::new();
            remainder = diff;
            while remainder < CALENDAR_FIELDS.len() as i64 {
                let v = exact[remainder as usize] as i64;
                if value == 0 {
                    value = v;
                    unit_key = UNIT_KEYS[remainder as usize].to_string();
                } else if v > 0 {
                    break;
                }
                remainder += 1;
            }
            remainder %= CALENDAR_FIELDS.len() as i64;
            if value == 0 {
                return None;
            }
        } else {
            if diff <= 0 {
                return None;
            }
            remainder = 0;
            let mut it = units.iter();
            let (k, u) = it.next()?;
            unit_key = k.clone();
            let mut unit = *u;
            for (next_key, next_unit) in it {
                if diff < *next_unit {
                    value = diff / unit;
                    remainder = diff % unit;
                    break;
                }
                unit_key = next_key.clone();
                unit = *next_unit;
            }
            if unit_key == "year" {
                value = diff / unit;
                remainder = diff % unit;
            }
        }
        if value != 1 {
            unit_key.push('s');
        }
        if self.abbreviate {
            unit_key.push_str(".abbr");
        }
        let mut output = format!("{value} {}", self.text(&unit_key));
        if max_unit_depth > 1 && remainder > 0 {
            output.push(' ');
            output.push_str(&self.to_string_depth(remainder, max_unit_depth - 1).unwrap_or_else(|| "null".into()));
        }
        Some(output)
    }

    /// `toString()`: `1 day later`, `2 weeks ago`, `tomorrow`, `-3 hours`, ….
    pub fn java_string(&self) -> String {
        let mut ms = self.milliseconds;
        if ms == 0 {
            let abbr = if self.abbreviate { ".abbr" } else { "" };
            let key = match self.kind {
                CURRENT_TYPE => format!("current.equal{abbr}"),
                RELATIVE_TYPE => format!("equal{abbr}"),
                _ => format!("zero{abbr}"),
            };
            return self.text(&key);
        }
        let before = ms < 0;
        if before {
            ms = -ms;
        }
        // The enclosing tool's depth (`depth` in the inner class is `ComparisonDateTool.this.depth`).
        let depth = self.tool.cmp.lock().depth;
        let friendly = self.to_string_depth(if self.kind == EXACT_TYPE { 0 } else { ms }, depth);
        let friendly_s = friendly.clone().unwrap_or_else(|| "null".into());
        if self.kind == DIFF_TYPE {
            return if before { format!("-{friendly_s}") } else { friendly_s };
        }
        let mut direction = if before { "before".to_string() } else { "after".to_string() };
        if self.kind == CURRENT_TYPE {
            direction = format!("current.{direction}");
            if let Some(f) = &friendly {
                if f.starts_with('1') {
                    let day_key = if self.abbreviate { "day.abbr" } else { "day" };
                    if *f == format!("1 {}", self.text(day_key)) {
                        direction.push_str(".day");
                        return self.text(&direction);
                    }
                }
            }
        }
        if self.abbreviate {
            direction.push_str(".abbr");
        }
        format!("{friendly_s} {}", self.text(&direction))
    }
}

fn comparison_value(c: Option<Comparison>) -> super::Ret {
    val(c.map(|c| JValue::Object(Arc::new(c) as Arc<dyn JObject>)).unwrap_or(JValue::Null))
}

fn comparison_def() -> Arc<ClassDef> {
    type C = Comparison;
    let me = || c(COMPARISON);
    let long = || JType::Long;
    let def = ClassDef::new(COMPARISON)
        .method::<C>("abbr", &[JType::Boolean], Some(me()), |t, _, a| comparison_value(Some(t.abbr(matches!(a[0], JValue::Bool(true))))))
        .method::<C>("depth", &[JType::Int], Some(me()), |t, _, a| comparison_value(Some(t.depth(super::num::int_value(&a[0])))))
        .method::<C>("locale", &[c("java.util.Locale")], Some(me()), |t, _, a| comparison_value(Some(t.locale(locale_arg(&a[0])?))))
        .method::<C>("getYears", &[], Some(long()), |t, _, _| val(JValue::Long(t.get_years())))
        .method::<C>("getMonths", &[], Some(long()), |t, _, _| val(JValue::Long(t.get_months())))
        .method::<C>("getWeeks", &[], Some(long()), |t, _, _| val(JValue::Long(t.get_weeks())))
        .method::<C>("getDays", &[], Some(long()), |t, _, _| val(JValue::Long(t.get_days())))
        .method::<C>("getHours", &[], Some(long()), |t, _, _| val(JValue::Long(t.get_hours())))
        .method::<C>("getMinutes", &[], Some(long()), |t, _, _| val(JValue::Long(t.get_minutes())))
        .method::<C>("getSeconds", &[], Some(long()), |t, _, _| val(JValue::Long(t.get_seconds())))
        .method::<C>("getMilliseconds", &[], Some(long()), |t, _, _| val(JValue::Long(t.get_milliseconds())))
        .method::<C>("getFull", &[], Some(me()), |t, _, _| comparison_value(Some(t.get_full())))
        .method::<C>("getDifference", &[], Some(me()), |t, _, _| comparison_value(Some(t.get_difference())))
        .method::<C>("getRelative", &[], Some(me()), |t, _, _| comparison_value(Some(t.get_relative())))
        .method::<C>("getExact", &[], Some(me()), |t, _, _| comparison_value(Some(t.get_exact())))
        .method::<C>("getAbbr", &[], Some(me()), |t, _, _| comparison_value(Some(t.get_abbr())))
        .method::<C>("toString", &[], Some(s()), |t, _, _| val(JValue::str(t.java_string())));
    super::object_methods::<C>(def).register()
}

tool_object!(Comparison, comparison_def);

fn comparison_date_tool_def() -> Arc<ClassDef> {
    type T = ComparisonDateTool;
    let me = || c(COMPARISON);
    let long = JType::Long;
    let mut def = ClassDef::new(COMPARISON_DATE_TOOL).extends(&[DATE_TOOL, safe::FORMAT_CONFIG, safe::LOCALE_CONFIG, safe::SAFE_CONFIG]).implements(&["java.io.Serializable"]);
    type Conv = fn(i64) -> i64;
    let statics: [(&str, Conv); 7] = [("toYears", to_years), ("toMonths", to_months), ("toWeeks", to_weeks), ("toDays", to_days), ("toHours", to_hours), ("toMinutes", to_minutes), ("toSeconds", to_seconds)];
    for (name, f) in statics {
        def = def.raw_method(java_compat::reflect::JMethod {
            name: name.into(),
            params: vec![long.clone()],
            returns: Some(long.clone()),
            declaring_class: COMPARISON_DATE_TOOL.into(),
            is_static: true,
            varargs: false,
            invoke: Arc::new(move |_, a| val(JValue::Long(f(super::num::long_value(&a[0]))))),
        });
    }
    let def = def
        .method::<T>("whenIs", &[o()], Some(me()), |t, _, a| comparison_value(t.when_is(&a[0])))
        .method::<T>("whenIs", &[o(), o()], Some(me()), |t, _, a| comparison_value(t.when_is_between(&a[0], &a[1])))
        .method::<T>("difference", &[o(), o()], Some(me()), |t, _, a| comparison_value(t.difference(&a[0], &a[1])))
        .method::<T>("timespan", &[o(), o()], Some(me()), |t, _, a| comparison_value(t.timespan(&a[0], &a[1])));
    date_methods::<T>(def).register()
}

tool_object!(ComparisonDateTool, comparison_date_tool_def);

pub(crate) fn comparison_date_tool_class() -> JavaClass {
    JavaClass::tool(comparison_date_tool_def(), || ComparisonDateTool::new().value())
        .extends(Some(DATE_TOOL))
        .annotated(Annotation::valid_scope(&[super::scope::APPLICATION]))
}
