//! `org.apache.velocity.tools.generic.AlternatorToolTests`.

use java_compat::lang::JValue;
use velocity::tools::alternator::{AUTO_ALTERNATE_DEFAULT_KEY, AlternatorTool};

use crate::base::{map, s};

#[test]
fn ctor_alternator_tool() {
    AlternatorTool::new();
}

#[test]
fn method_configure_map() {
    let tool = AlternatorTool::new();
    tool.configure(&map(&[(AUTO_ALTERNATE_DEFAULT_KEY, JValue::Bool(false))]));
    assert!(!tool.get_auto_alternate_default());
}

#[test]
fn methods_get_set_auto_alternate_default() {
    let tool = AlternatorTool::new();
    assert!(tool.get_auto_alternate_default());
    tool.set_auto_alternate_default(false);
    assert!(!tool.get_auto_alternate_default());
    tool.set_auto_alternate_default(true);
    assert!(tool.get_auto_alternate_default());
}

#[test]
fn method_make_object_var_args() {
    let tool = AlternatorTool::new();
    assert!(tool.make(Some(&[])).is_none());
    let result = tool.make(Some(&[JValue::Bool(true)])).unwrap();
    assert!(matches!(result.get_next(), JValue::Bool(true)));
    assert!(matches!(result.get_next(), JValue::Bool(true)));
    let result = tool.make(Some(&[s("hi"), JValue::Bool(true), JValue::Bool(false), JValue::Int(0)])).unwrap();
    assert_eq!(result.get_next().as_str(), Some("hi"));
    assert!(matches!(result.get_next(), JValue::Bool(true)));
    assert!(matches!(result.get_next(), JValue::Bool(false)));
    assert!(matches!(result.get_next(), JValue::Int(0)));
    assert_eq!(result.get_next().as_str(), Some("hi"));
    let result = tool.make(Some(&[s("red"), s("blue")])).unwrap();
    result.shift();
    assert_eq!(result.java_string_opt().as_deref(), Some("blue"));
}

#[test]
fn method_make_boolean_object_var_args() {
    let tool = AlternatorTool::new();
    let result = tool.make_with(false, Some(&[s("red"), s("blue")])).unwrap();
    assert_eq!(result.java_string_opt().as_deref(), Some("red"));
    assert_eq!(result.java_string_opt().as_deref(), Some("red"));
    result.shift();
    assert_eq!(result.java_string_opt().as_deref(), Some("blue"));
}

#[test]
fn method_auto_object_var_args() {
    let tool = AlternatorTool::new();
    let result = tool.auto(Some(&[JValue::Int(-1), JValue::Int(0), JValue::Null, JValue::Int(1)])).unwrap();
    assert_eq!(result.java_string_opt().as_deref(), Some("-1"));
    assert!(matches!(result.get_current(), JValue::Int(0)));
    assert_eq!(result.java_string_opt().as_deref(), Some("0"));
    assert_eq!(result.java_string_opt(), None);
    assert_eq!(result.java_string_opt().as_deref(), Some("1"));
    assert_eq!(result.java_string_opt().as_deref(), Some("-1"));
}

#[test]
fn method_manual_object_var_args() {
    let tool = AlternatorTool::new();
    let result = tool.manual(Some(&[JValue::Bool(true), JValue::Bool(false)])).unwrap();
    assert!(matches!(result.get_current(), JValue::Bool(true)));
    assert_eq!(result.java_string_opt().as_deref(), Some("true"));
    assert!(matches!(result.get_next(), JValue::Bool(true)));
    assert_eq!(result.java_string_opt().as_deref(), Some("false"));
    assert!(matches!(result.get_next(), JValue::Bool(false)));
    assert_eq!(result.java_string_opt().as_deref(), Some("true"));
}
