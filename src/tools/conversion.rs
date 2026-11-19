//! `org.apache.velocity.tools.ConversionUtils`: numbers, dates, calendars, locales and strings from the values
//! templates pass, with the named number styles (`number`, `currency`, `percent`, `integer`) and date styles
//! (`full` … `short`, `iso`, `iso_tz`, `intl`, `intl_tz`, suffixed `_date` / `_time`).

use java_compat::lang::{JError, JList, JValue};
use java_compat::text::objects::{calendar_of, date_millis};
use java_compat::text::{DateFormat, DecimalFormat, DecimalFormatSymbols, Locale, NumberFormat, SimpleDateFormat};
use java_compat::util::{Date, GregorianCalendar, TimeZone};

use super::num;

const STYLE_NUMBER: i32 = 0;
const STYLE_CURRENCY: i32 = 1;
const STYLE_PERCENT: i32 = 2;
const STYLE_INTEGER: i32 = 4;

const STYLE_ISO: i32 = 5;
const STYLE_ISO_TZ: i32 = 6;
const STYLE_INTL: i32 = 7;
const STYLE_INTL_TZ: i32 = 8;

/// `getNumberStyleAsInt`.
pub fn number_style_as_int(style: &str) -> i32 {
    let n = style.encode_utf16().count();
    if !(6..=8).contains(&n) {
        return -1;
    }
    match style.to_ascii_lowercase().as_str() {
        "default" | "number" => STYLE_NUMBER,
        "currency" => STYLE_CURRENCY,
        "percent" => STYLE_PERCENT,
        "integer" => STYLE_INTEGER,
        _ => -1,
    }
}

/// `getNumberFormat(format, locale)`: a named style, or a `DecimalFormat` pattern (whose syntax errors escape as
/// `IllegalArgumentException`).
pub fn number_format(format: &str, locale: &Locale) -> Result<DecimalFormat, JError> {
    let style = number_style_as_int(format);
    if style < 0 {
        return DecimalFormat::new(format, &DecimalFormatSymbols::new(locale));
    }
    Ok(number_format_style(style, locale).expect("a known style"))
}

/// `getNumberFormat(style, locale)`.
pub fn number_format_style(style: i32, locale: &Locale) -> Option<DecimalFormat> {
    Some(match style {
        STYLE_NUMBER => NumberFormat::number_instance(locale),
        STYLE_CURRENCY => NumberFormat::currency_instance(locale),
        STYLE_PERCENT => NumberFormat::percent_instance(locale),
        STYLE_INTEGER => NumberFormat::integer_instance(locale),
        _ => return None,
    })
}

/// `toNumber(obj)`: numbers as they are, dates and calendars as their milliseconds, anything else parsed
/// with the default number format of the default locale.
pub fn to_number(obj: &JValue) -> Option<JValue> {
    to_number_handling(obj, true)
}

/// `toNumber(obj, handleStrings)`.
pub fn to_number_handling(obj: &JValue, handle_strings: bool) -> Option<JValue> {
    if obj.is_null() {
        return None;
    }
    if num::is_number(obj) {
        return Some(obj.clone());
    }
    if let Some(ms) = date_millis(obj) {
        return Some(JValue::Long(ms));
    }
    if let Some(mut cal) = calendar_of(obj) {
        return Some(JValue::Long(cal.time_in_millis()));
    }
    if handle_strings {
        return parse_number(&obj.to_java_string(), "default", &super::default_locale());
    }
    None
}

/// `toNumber(String value, format, locale)`: `null` when the format is invalid or nothing parses.
pub fn parse_number(value: &str, format: &str, locale: &Locale) -> Option<JValue> {
    let parser = number_format(format, locale).ok()?;
    parser.parse_checked(value).ok()
}

/// `toNumber(Object value, format, locale)`.
pub fn to_number_in(value: &JValue, format: &str, locale: &Locale) -> Option<JValue> {
    if let Some(n) = to_number_handling(value, false) {
        return Some(n);
    }
    parse_number(&value.to_java_string(), format, locale)
}

/// A `DateFormat` the tools hand out: a `SimpleDateFormat`, or `TimeZoneIDSuffixFormat` around one (which
/// appends a space and the zone's ID, and cannot parse).
#[derive(Clone)]
pub enum DateFmt {
    /// A plain `SimpleDateFormat`.
    Simple(SimpleDateFormat),
    /// `TimeZoneIDSuffixFormat`: the wrapped format's output, a space and the zone's ID.
    TzSuffix(SimpleDateFormat),
}

impl DateFmt {
    /// `DateFormat.format(Date)` of `millis` since the epoch.
    pub fn format(&mut self, millis: i64) -> String {
        match self {
            DateFmt::Simple(f) => f.format(millis),
            DateFmt::TzSuffix(f) => {
                let out = f.format(millis);
                format!("{out} {}", f.time_zone().id())
            }
        }
    }

    /// `DateFormat.parse(String)`.
    pub fn parse(&mut self, text: &str) -> Result<i64, JError> {
        match self {
            DateFmt::Simple(f) => f.parse_checked(text),
            DateFmt::TzSuffix(_) => Err(JError::new("java.lang.UnsupportedOperationException", "intl_tz date/time formats cannot be used to parse dates")),
        }
    }

    /// The underlying `SimpleDateFormat`, `None` for the time-zone-suffixed format.
    pub fn simple(&self) -> Option<&SimpleDateFormat> {
        match self {
            DateFmt::Simple(f) => Some(f),
            DateFmt::TzSuffix(_) => None,
        }
    }

    /// `DateFormat.setTimeZone(zone)`, on the wrapped format for the suffixed one.
    pub fn set_time_zone(&mut self, zone: &TimeZone) {
        match self {
            DateFmt::Simple(f) | DateFmt::TzSuffix(f) => f.set_time_zone(zone),
        }
    }
}

/// `getDateStyleAsInt`: `NullPointerException` for `null` (it upper-cases the style first).
pub fn date_style_as_int(style: Option<&str>) -> Result<i32, JError> {
    let Some(style) = style else { return Err(super::npe()) };
    Ok(match style.to_uppercase().as_str() {
        "FULL" => 0,
        "LONG" => 1,
        "MEDIUM" | "DEFAULT" => 2,
        "SHORT" => 3,
        "ISO" => STYLE_ISO,
        "ISO_TZ" => STYLE_ISO_TZ,
        "INTL" => STYLE_INTL,
        "INTL_TZ" => STYLE_INTL_TZ,
        _ => -1,
    })
}

/// A `SimpleDateFormat` the class initialiser created: default locale and zone.
fn static_format(pattern: &str) -> SimpleDateFormat {
    SimpleDateFormat::new(pattern, &super::default_locale(), &TimeZone::default_zone()).expect("a valid pattern")
}

/// `getDateFormat(String format, locale, timezone)`: a named style (`*_date` / `*_time` for one part), else a
/// `SimpleDateFormat` pattern, whose syntax errors escape.
pub fn date_format(format: Option<&str>, locale: &Locale, zone: &TimeZone) -> Result<Option<DateFmt>, JError> {
    let Some(format) = format else { return Ok(None) };
    if let Some(fmt) = format.strip_suffix("_date") {
        let style = date_style_as_int(Some(fmt))?;
        return Ok(date_format_styles(style, -1, locale, zone));
    }
    if let Some(fmt) = format.strip_suffix("_time") {
        let style = date_style_as_int(Some(fmt))?;
        return Ok(date_format_styles(-1, style, locale, zone));
    }
    let style = date_style_as_int(Some(format))?;
    if style < 0 {
        let mut df = SimpleDateFormat::new(format, locale, zone)?;
        df.set_time_zone(zone);
        return Ok(Some(DateFmt::Simple(df)));
    }
    Ok(date_format_styles(style, style, locale, zone))
}

/// `getDateFormat(String dateStyle, String timeStyle, locale, timezone)`.
pub fn date_format_named(date_style: Option<&str>, time_style: Option<&str>, locale: &Locale, zone: &TimeZone) -> Result<Option<DateFmt>, JError> {
    let ds = date_style_as_int(date_style)?;
    let ts = date_style_as_int(time_style)?;
    Ok(date_format_styles(ds, ts, locale, zone))
}

/// `getDateFormat(int dateStyle, int timeStyle, locale, timezone)`: `null` (logged) when the JDK refuses.
pub fn date_format_styles(date_style: i32, time_style: i32, locale: &Locale, zone: &TimeZone) -> Option<DateFmt> {
    let jdk = |d: Option<i32>, t: Option<i32>, l: &Locale| DateFormat::instance(d, t, l, &TimeZone::default_zone()).ok().map(DateFmt::Simple);
    let mut df = if date_style < 0 && time_style < 0 {
        jdk(Some(2), None, &super::default_locale())?
    } else if time_style < 0 {
        match date_style {
            STYLE_ISO | STYLE_ISO_TZ | STYLE_INTL | STYLE_INTL_TZ => DateFmt::Simple(static_format("yyyy-MM-dd")),
            _ => jdk(Some(date_style), None, locale)?,
        }
    } else if date_style < 0 {
        match time_style {
            STYLE_ISO | STYLE_INTL => DateFmt::Simple(static_format("HH:mm:ss")),
            STYLE_ISO_TZ => DateFmt::Simple(static_format("HH:mm:ssXXX")),
            STYLE_INTL_TZ => DateFmt::TzSuffix(static_format("HH:mm:ss")),
            _ => jdk(None, Some(time_style), locale)?,
        }
    } else {
        match date_style {
            STYLE_ISO => DateFmt::Simple(static_format("yyyy-MM-dd'T'HH:mm:ss")),
            STYLE_ISO_TZ => DateFmt::Simple(static_format("yyyy-MM-dd'T'HH:mm:ssXXX")),
            STYLE_INTL => DateFmt::Simple(static_format("yyyy-MM-dd HH:mm:ss")),
            STYLE_INTL_TZ => DateFmt::TzSuffix(static_format("yyyy-MM-dd HH:mm:ss")),
            _ => jdk(Some(date_style), Some(time_style), locale)?,
        }
    };
    df.set_time_zone(zone);
    Some(df)
}

/// `toDate(obj)`: a `Date`'s milliseconds, a calendar's, or a number's `longValue()`.
pub fn to_date(obj: &JValue) -> Option<i64> {
    if obj.is_null() {
        return None;
    }
    if let Some(ms) = date_millis(obj) {
        return Some(ms);
    }
    if let Some(mut cal) = calendar_of(obj) {
        return Some(cal.time_in_millis());
    }
    if num::is_number(obj) {
        return Some(num::long_value(obj));
    }
    None
}

/// `toDate(obj, format, locale, timezone)`.
pub fn to_date_in(obj: &JValue, format: Option<&str>, locale: &Locale, zone: &TimeZone) -> Option<i64> {
    if let Some(d) = to_date(obj) {
        return Some(d);
    }
    parse_date(&obj.to_java_string(), format, locale, zone)
}

/// `toDate(String str, format, locale, timezone)`: `null` on any failure.
pub fn parse_date(s: &str, format: Option<&str>, locale: &Locale, zone: &TimeZone) -> Option<i64> {
    let mut parser = date_format(format, locale, zone).ok()??;
    parser.parse(s).ok()
}

/// `toCalendar(date, locale)`: a calendar in the default time zone.
pub fn to_calendar(date: i64, locale: Option<&Locale>) -> GregorianCalendar {
    let locale = locale.cloned().unwrap_or_else(super::default_locale);
    GregorianCalendar::new(date, &TimeZone::default_zone(), &locale)
}

/// `ConversionUtils.toString(Object)`: an array's first element, `String.valueOf` otherwise.
pub fn to_string(value: &JValue) -> Option<String> {
    match value {
        JValue::Str(s) => Some(s.to_string()),
        JValue::Null => None,
        JValue::Array(a) => a.get(0).and_then(|v| to_string(&v)),
        other => Some(other.to_java_string()),
    }
}

/// `ConversionUtils.toString(Collection)`: the first element's.
pub fn collection_to_string(values: &JValue) -> Option<String> {
    values.iterate().and_then(|v| v.first().and_then(to_string))
}

/// `ConversionUtils.toBoolean(Object)`: `Boolean.valueOf(toString(value))`.
pub fn to_boolean(value: &JValue) -> Option<bool> {
    if let JValue::Bool(b) = value {
        return Some(*b);
    }
    to_string(value).map(|s| s.eq_ignore_ascii_case("true"))
}

/// `ConversionUtils.toLocale(String)`: `language`, `language_COUNTRY` or `language_COUNTRY_VARIANT`.
pub fn to_locale(value: &str) -> Option<Locale> {
    let data = super::locale_data();
    if !value.contains('_') {
        return Some(Locale::new(value, "", "").with_data(data));
    }
    let params = java_compat::methods::split(value, "_", 0).ok()?;
    match params.len() {
        2 => Some(Locale::new(&params[0], &params[1], "").with_data(data)),
        3 => Some(Locale::new(&params[0], &params[1], &params[2]).with_data(data)),
        _ => None,
    }
}

/// `ConversionUtils.asList(Object)`: a list as is, an array's elements in a new `ArrayList`, else a singleton.
pub fn as_list(value: &JValue) -> JValue {
    match value {
        JValue::List(_) => value.clone(),
        JValue::Array(a) => JValue::List(JList::array_list(a.items())),
        other => JValue::List(JList::array_list(vec![other.clone()])),
    }
}

/// A `java.util.Date` value.
pub fn date_value(millis: i64) -> JValue {
    Date::value(millis)
}
