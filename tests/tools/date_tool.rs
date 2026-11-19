//! `org.apache.velocity.tools.generic.DateToolTests`. The suite runs on a full JDK, so `Locale.FRANCE` has its
//! French data ([`LocaleData::Full`](java_compat::text::LocaleData::Full), the default).

use java_compat::text::symbols::SHORT;
use java_compat::text::{DateFormat, Locale, SimpleDateFormat};
use java_compat::util::TimeZone;
use velocity::tools::date::DateTool;

fn test_locale() -> Locale {
    Locale::france()
}

fn test_zone() -> TimeZone {
    TimeZone::get("Europe/Paris")
}

#[test]
fn to_localized_pattern_simple_pattern() {
    let dt = DateTool::new();
    let expected = SimpleDateFormat::new("yyyy-MM-dd", &test_locale(), &TimeZone::default_zone()).unwrap().to_localized_pattern().unwrap();
    assert_eq!(dt.to_localized_pattern(Some("yyyy-MM-dd"), &test_locale()).unwrap(), Some(expected), "DateTool incorrectly localizes date format pattern");
}

#[test]
fn to_localized_pattern_predefined_date_time_pattern() {
    let dt = DateTool::new();
    let expected = DateFormat::date_time_instance(SHORT, SHORT, &test_locale(), &TimeZone::default_zone()).unwrap().to_localized_pattern().unwrap();
    assert_eq!(dt.to_localized_pattern(Some("short"), &test_locale()).unwrap(), Some(expected));
}

#[test]
fn to_localized_pattern_predefined_date_pattern() {
    let dt = DateTool::new();
    let expected = DateFormat::date_instance(SHORT, &test_locale(), &TimeZone::default_zone()).unwrap().to_localized_pattern().unwrap();
    assert_eq!(dt.to_localized_pattern(Some("short_date"), &test_locale()).unwrap(), Some(expected));
}

#[test]
fn to_localized_pattern_predefined_time_pattern() {
    let dt = DateTool::new();
    let expected = DateFormat::time_instance(SHORT, &test_locale(), &TimeZone::default_zone()).unwrap().to_localized_pattern().unwrap();
    assert_eq!(dt.to_localized_pattern(Some("short_time"), &test_locale()).unwrap(), Some(expected));
}

#[test]
fn to_iso_format() {
    let dt = DateTool::new();
    dt.set_locale(test_locale());
    dt.set_time_zone(test_zone());
    let now = velocity::tools::now_millis();
    let date = velocity::tools::conversion::date_value(now);
    // `new SimpleDateFormat()` (the default locale) with the Paris calendar.
    let mut format = SimpleDateFormat::new("yyyy-MM-dd'T'HH:mm:ss", &Locale::default_locale(), &test_zone()).unwrap();
    let mut expect = |pattern: &str, suffix: &str, style: &str| {
        format.apply_pattern(pattern).unwrap();
        let expected = format!("{}{suffix}", format.format(now));
        assert_eq!(dt.format_as(Some(style), &date).unwrap(), Some(expected), "DateTool incorrectly formatted iso format");
    };
    let zone = format!(" {}", test_zone().id());
    expect("yyyy-MM-dd'T'HH:mm:ss", "", "iso");
    expect("yyyy-MM-dd'T'HH:mm:ssXXX", "", "iso_tz");
    expect("yyyy-MM-dd HH:mm:ss", "", "intl");
    expect("yyyy-MM-dd HH:mm:ss", &zone, "intl_tz");
    expect("yyyy-MM-dd", "", "iso_date");
    expect("yyyy-MM-dd", "", "intl_date");
    expect("HH:mm:ss", "", "iso_time");
    expect("HH:mm:ss", "", "intl_time");
    expect("HH:mm:ssXXX", "", "iso_tz_time");
    expect("HH:mm:ss", &zone, "intl_tz_time");
}
