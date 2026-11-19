//! `GetAsTestCase`, `MiscTestCase`.

use java_compat::lang::JValue;
use java_compat::reflect::JType;
use velocity::props::{ExtProperties, PropValue};

use crate::base::*;

const PREFIX: &str = "org.apache.velocity.test.GetAsTestCase$";

fn custom_string(template: bool, v: Option<&str>) -> JValue {
    let c = class(&format!("{PREFIX}{}", if template { "CustomTemplateString" } else { "CustomString" }));
    let c = if template { c.extends(&[&format!("{PREFIX}CustomString")]).implements(&["org.apache.velocity.util.TemplateString"]) } else { c };
    c.method("getAsString", &[], Some(string()), |b, _| ret(b.field("string"))).build().with_fields(&[("string", v.map(s).unwrap_or(JValue::Null))])
}

fn custom_boolean(template: bool, v: bool) -> JValue {
    let c = class(&format!("{PREFIX}{}", if template { "CustomTemplateBoolean" } else { "CustomBoolean" }));
    let c = if template { c.extends(&[&format!("{PREFIX}CustomBoolean")]).implements(&["org.apache.velocity.util.TemplateBoolean"]) } else { c };
    c.method("getAsBoolean", &[], Some(JType::Boolean), |b, _| ret(b.field("bool"))).build().with_fields(&[("bool", JValue::Bool(v))])
}

fn custom_number(template: bool, v: i32) -> JValue {
    let c = class(&format!("{PREFIX}{}", if template { "CustomTemplateNumber" } else { "CustomNumber" }));
    let c = if template { c.extends(&[&format!("{PREFIX}CustomNumber")]).implements(&["org.apache.velocity.util.TemplateNumber"]) } else { c };
    c.method("getAsNumber", &[], Some(JType::class("java.lang.Number")), |b, _| ret(b.field("num"))).build().with_fields(&[("num", JValue::Int(v))])
}

#[test]
fn test_custom_string() {
    let mut b = Base::new();
    b.put("foo", custom_string(false, Some("getAsString")));
    b.assert_eval_equals("getAsString", "$foo");
    b.put("bar", custom_string(false, None));
    b.assert_eval_equals("", "$!bar");
    b.put("half", custom_string(false, Some("half")));
    b.assert_eval_equals("1half", "#set( $out = 1 + $half )$out");
}

#[test]
fn test_custom_boolean() {
    let mut b = Base::new();
    b.put("foo", custom_boolean(false, false));
    b.assert_eval_equals("right", "#if( !$foo )right#end");
}

#[test]
fn test_custom_number() {
    let mut b = Base::new();
    b.put("foo", custom_number(false, 7));
    b.assert_eval_equals("14", "#set( $bar = $foo * 2 )$bar");
}

#[test]
fn test_template_string() {
    let mut b = Base::new();
    b.put("foo", custom_string(true, Some("getAsString")));
    b.assert_eval_equals("getAsString", "$foo");
}

#[test]
fn test_template_boolean() {
    let mut b = Base::new();
    b.put("foo", custom_boolean(true, false));
    b.assert_eval_equals("right", "#if( !$foo )right#end");
}

#[test]
fn test_template_number() {
    let mut b = Base::new();
    b.put("foo", custom_number(true, 5));
    b.assert_eval_equals("25", "#set( $foo = $foo * $foo )$foo");
}

#[test]
fn test_runtime_instance_properties() {
    // Before `init()` the overriding properties answer, after it the configuration.
    let mut props = ExtProperties::new();
    props.set_property("baabaa.test", "the answer".into());
    assert!(matches!(props.get("baabaa.test"), Some(PropValue::Str(v)) if v == "the answer"));
    let engine = velocity::Engine::with_properties(&props).expect("initialises");
    assert!(matches!(engine.property("baabaa.test"), Some(PropValue::Str(v)) if v == "the answer"));
}

#[test]
fn test_string_utils() {
    use java_compat::lang::string_utils_trim as trim;
    let arg: Option<&str> = None;
    assert_eq!(trim(arg), None);
    assert_eq!(trim(Some(" test ")), Some("test"));
    assert_eq!(trim(Some("test")), Some("test"));
}
