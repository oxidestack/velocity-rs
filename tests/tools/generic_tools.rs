//! `org.apache.velocity.tools.test.whitebox.GenericToolsTests`: the tools of `whiteboxtest-toolbox.xml`, from a
//! `ToolManager`.

use std::sync::{Arc, LazyLock};

use java_compat::lang::{JObject, JValue};
use java_compat::reflect::{JType, LazyClass};
use java_compat::text::Locale;
use java_compat::util::{GregorianCalendar, TimeZone};
use parking_lot::Mutex;
use velocity::tools::classes::{Field, JavaClass};
use velocity::tools::date::ComparisonDateTool;
use velocity::tools::escape::EscapeTool;
use velocity::tools::field::FieldTool;
use velocity::tools::math_tool::MathTool;
use velocity::tools::number::NumberTool;
use velocity::tools::resource_tool::ResourceTool;
use velocity::tools::toolbox::{ToolContext, ToolManager};

use crate::base::{classpath, i, s};

const TOOLBOX_PATH: &str = "whiteboxtest-toolbox.xml";
const TESTS: &str = "org.apache.velocity.tools.test.whitebox.GenericToolsTests";

/// `GenericToolsTests.MUTABLE_FIELD`.
static MUTABLE_FIELD: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new("foo".into()));

/// `@BeforeClass initGenericToolsTests`: one tool context for the whole class.
static TOOLBOX: LazyLock<Arc<ToolContext>> = LazyLock::new(|| {
    classpath();
    JavaClass::new(TESTS).field(Field::public_static("MUTABLE_FIELD", false, JType::string(), || JValue::str(MUTABLE_FIELD.lock().as_str()))).register();
    let manager = ToolManager::new(true, false).unwrap();
    manager.configure_path(TOOLBOX_PATH).unwrap();
    manager.create_context(None)
});

fn tool<T: 'static>(key: &str) -> JValue {
    let v = TOOLBOX.get(key).unwrap();
    assert!(java_compat::reflect::is::<T>(&v), "{key} is a {}", std::any::type_name::<T>());
    v
}

fn assert_string_equals(expected: &str, test_this: &JValue) {
    assert_eq!(expected, test_this.to_java_string());
}

/// A `GenericToolsTests` instance (`this`).
struct This;

impl JObject for This {
    fn class_name(&self) -> String {
        TESTS.into()
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<Result<Option<JValue>, java_compat::lang::JError>> {
        None
    }

    fn to_java_string(&self) -> String {
        java_compat::reflect::identity_string(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<java_compat::reflect::ClassDef>> {
        static DEF: LazyClass = LazyClass::new(|| java_compat::reflect::ClassDef::new(TESTS).register());
        Some(DEF.get())
    }
}

/// `new GregorianCalendar(y, m, d)`: the default zone and locale.
fn calendar(y: i32, m: i32, d: i32) -> JValue {
    let mut c = GregorianCalendar::from_fields(y, m, d, 0, 0, 0, &TimeZone::default_zone(), &Locale::default_locale());
    c.time_in_millis();
    c.into_value()
}

#[test]
fn test_date_tool() {
    let v = tool::<ComparisonDateTool>("date");
    let date_tool = java_compat::reflect::downcast::<ComparisonDateTool>(&v).unwrap();
    let date = velocity::tools::conversion::date_value({
        let mut c = GregorianCalendar::from_fields(2007, 0, 2, 0, 0, 0, &TimeZone::default_zone(), &Locale::default_locale());
        c.time_in_millis()
    });
    let disp = "2007-01-02";
    let disp2 = "2007/01/02";
    assert_eq!(date_tool.get_format(), "yyyy-MM-dd");
    assert_eq!(date_tool.format(&date).unwrap().as_deref(), Some(disp));
    assert_eq!(date_tool.format_as(Some("yyyy/MM/dd"), &date).unwrap().as_deref(), Some(disp2));
    assert_eq!(date_tool.get_year(&s(disp)), Some(2007));
    assert_eq!(date_tool.get_month(&s(disp)), Some(0));
    assert_eq!(date_tool.get_day(&s(disp)), Some(2));
}

#[test]
fn test_escape_tool() {
    let v = tool::<EscapeTool>("esc");
    let esc = java_compat::reflect::downcast::<EscapeTool>(&v).unwrap();
    assert_eq!(esc.velocity(&s("$foo #bar()")).as_deref(), Some("${esc.d}foo ${esc.h}bar()"));
    assert_eq!(esc.property_key(&s(" C:\\Program Files")).as_deref(), Some("\\ C\\:\\\\Program\\ Files"));
    assert_eq!(esc.property_value(&s(" C:\\Program Files")).as_deref(), Some("\\ C\\:\\\\Program Files"));
    assert_eq!(esc.java(&s("\u{FFFF}\u{8}\n\t\u{c}\r\"\\")).as_deref(), Some("\\uFFFF\\b\\n\\t\\f\\r\\\"\\\\"));
    assert_eq!(esc.javascript(&s("\u{FFFF}\u{8}\n\t\u{c}\r\"'\\")).as_deref(), Some("\\uFFFF\\b\\n\\t\\f\\r\\\"\\'\\\\"));
    assert_eq!(esc.html(&s("\"&<>\u{a0}")).as_deref(), Some("&quot;&amp;&lt;&gt;&nbsp;"));
    assert_eq!(esc.url(&s("@/?= &")).as_deref(), Some("%40%2F%3F%3D+%26"));
    assert_eq!(esc.sql(&s("'")).as_deref(), Some("''"));
    assert_eq!(esc.html(&s("\"&<>")).as_deref(), Some("&quot;&amp;&lt;&gt;"));
    assert_eq!(esc.unicode(&s("f00b")).unwrap().as_deref(), Some("\u{f00b}"));
    assert_eq!(esc.unicode(&s("\\u1010")).unwrap().as_deref(), Some("\u{1010}"));
    assert_eq!(esc.unicode(&i(1111)).unwrap().as_deref(), Some("\u{1111}"));
}

#[test]
fn test_field_tool() {
    let v = tool::<FieldTool>("field");
    let field = java_compat::reflect::downcast::<FieldTool>(&v).unwrap();
    assert!(matches!(field.get("MAX_VALUE"), JValue::Int(i32::MAX)));
    assert!(matches!(field.in_class_named("java.lang.Boolean").unwrap().get("TRUE"), JValue::Bool(true)));
    assert!(field.in_class_named("no.such.Class").is_none());
    assert!(field.in_object(&JValue::Object(Arc::new(This))).is_some());
    assert_eq!(field.get("MUTABLE_FIELD").as_str().map(str::to_string), Some(MUTABLE_FIELD.lock().clone()));
    let foo = MUTABLE_FIELD.lock().clone();
    let doubled = format!("{foo}{foo}");
    *MUTABLE_FIELD.lock() = doubled.clone();
    assert_ne!(foo, doubled);
    assert_eq!(field.get("MUTABLE_FIELD").as_str(), Some(doubled.as_str()));
    assert!(matches!(field.get("java.lang.Long.MIN_VALUE"), JValue::Long(i64::MIN)));
}

fn int(v: Option<JValue>) -> i32 {
    match v {
        Some(JValue::Int(i)) => i,
        other => panic!("an Integer, not {other:?}"),
    }
}

#[test]
fn test_math_tool() {
    let v = tool::<MathTool>("math");
    let math = java_compat::reflect::downcast::<MathTool>(&v).unwrap();
    assert_eq!(int(math.abs(&i(-1))), 1);
    assert_eq!(int(math.add(&[i(1), i(1)])), 2);
    assert_eq!(math.ceil(&JValue::Double(2.5)), Some(3));
    assert_eq!(int(math.div(&[i(8), i(2)])), 4);
    assert_eq!(math.floor(&JValue::Double(5.1)), Some(5));
    assert_eq!(int(Some(math.get_average_longs(&[5, 6, 7]))), 6);
    // getTotal() watches the type of its first argument, so assertEquals needs a long
    assert_eq!(math.get_total_longs(&[2, 2, 3]), 7i64);
    assert_eq!(math.idiv(&i(130), &i(16)), Some(8));
    assert_eq!(int(math.max(&[i(9), i(-10)])), 9);
    assert_eq!(int(math.min(&[i(10), i(20)])), 10);
    assert_eq!(math.modulo(&i(37), &i(13)), Some(11));
    assert_eq!(int(math.mul(&[i(3), i(4)])), 12);
    assert_eq!(math.round(&JValue::Double(12.8)), Some(13));
    assert_eq!(math.round_to(&i(1), &JValue::Double(14.18)), Some(14.2));
    assert_eq!(math.round_to(&i(2), &JValue::Double(-4.999)), Some(-5.0));
    assert_eq!(int(math.sub(&[i(30), i(15)])), 15);
    assert_eq!(int(math.pow(&i(4), &i(2))), 16);
    assert_eq!(math.to_integer(&s("17")), Some(17));
    assert_eq!(math.to_double(&s("18.1")), Some(18.1));
}

#[test]
fn test_number_tool() {
    tool::<NumberTool>("number");
}

#[test]
fn test_resource_tool() {
    let v = tool::<ResourceTool>("text");
    let text = java_compat::reflect::downcast::<ResourceTool>(&v).unwrap();
    let keys = text.get_keys().unwrap().unwrap();
    assert!(keys.iter().any(|k| k == "foo"));
    assert!(keys.iter().any(|k| k == "hello.whoever"));
    assert!(keys.iter().any(|k| k == "world"));
    let keys = text.get(Some("hello")).get_keys().unwrap().unwrap();
    assert!(keys.iter().any(|k| k == "whoever"));
    assert!(!keys.iter().any(|k| k == "foo"));
    let foo = text.get(Some("foo"));
    assert_string_equals("bar", &foo.clone().value());
    let french_foo = foo.locale(Locale::french().value());
    assert_string_equals("barre", &french_foo.value());
    let other_foo = foo.bundle("resources2");
    assert_string_equals("woogie", &other_foo.value());
    let hello_whoever = text.get(Some("hello")).get("whoever");
    assert_string_equals("Hello {0}!", &hello_whoever.clone().value());
    let hello_world = hello_whoever.insert(vec![text.get(Some("world")).value()]);
    assert_string_equals("Hello World!", &hello_world.clone().value());
    let half_french = hello_world.locale(Locale::french().value());
    assert_string_equals("Bonjour World!", &half_french.value());
    let french_tool = text.locale(s("fr"));
    let french_hello_world = french_tool.get("hello.whoever").insert(vec![french_tool.get("world").value()]);
    assert_string_equals("Bonjour Monde!", &french_hello_world.value());
}

#[test]
fn test_comparison_date_tool() {
    let v = tool::<ComparisonDateTool>("date");
    let date_tool = java_compat::reflect::downcast::<ComparisonDateTool>(&v).unwrap();
    let when_is = date_tool.when_is_between(&calendar(2007, 0, 2), &calendar(2007, 1, 15)).unwrap();
    assert_eq!(when_is.get_years(), 0);
    assert_eq!(when_is.get_months(), 1);
    assert_eq!(when_is.get_days(), 44);
    // the toolbox config says to skip months, so this should be in weeks
    assert_eq!(when_is.java_string(), "6 weeks later");
}

#[test]
fn test_comparison_date_tool2() {
    let v = tool::<ComparisonDateTool>("date");
    let date_tool = java_compat::reflect::downcast::<ComparisonDateTool>(&v).unwrap();
    let when_is = date_tool.timespan(&calendar(1954, 5, 26), &calendar(2019, 5, 20)).unwrap();
    assert_eq!(when_is.get_years(), 64);
    assert_eq!(when_is.get_months(), 11);
    assert_eq!(when_is.get_days(), 25);
}
