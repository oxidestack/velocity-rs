//! `org.apache.velocity.tools.generic.XmlToolTests`.

use java_compat::lang::JValue;
use velocity::tools::safe::Tool;
use velocity::tools::values::ValueParser;
use velocity::tools::xml_tool::XmlTool;

use crate::base::{classpath, i, map, s};
use std::sync::Arc;

const XML_FILE: &str = "file.xml";
const XML_STRING: &str = "<foo>\n  <bar name=\"a\"/>\n  <baz>woogie</baz>\n  <baz>wiggie</baz>\n</foo>";

fn string_based() -> Arc<XmlTool> {
    let xml = XmlTool::new();
    xml.configure_values(&ValueParser::new(None));
    xml.parse(Some(XML_STRING))
}

fn file_based() -> Arc<XmlTool> {
    classpath();
    let xml = XmlTool::new();
    xml.configure_values(&ValueParser::new(None));
    xml.read(Some(XML_FILE))
}

fn string(x: &XmlTool) -> String {
    java_compat::lang::JObject::to_java_string(x)
}

#[test]
fn ctor_xml_tool() {
    XmlTool::new();
}

#[test]
fn test_string_file_equals() {
    assert_eq!(string(&string_based()), string(&file_based()));
}

#[test]
fn method_attr_object() {
    let xml = string_based();
    assert_eq!(xml.attr(&s("href")), None);
    let xml = xml.find(&s("bar")).unwrap();
    assert!(xml.attr(&s("name")).is_some());
    assert_eq!(xml.attr(&s("name")).as_deref(), Some("a"));
}

#[test]
fn method_attributes() {
    let xml = string_based();
    assert!(xml.attributes().unwrap().is_empty());
    let xml = xml.find(&s("bar")).unwrap();
    let result = xml.attributes().unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result.get_str("name").unwrap().as_str(), Some("a"));
}

#[test]
fn method_children() {
    let xml = string_based();
    assert_eq!(xml.size(), 1);
    assert_eq!(xml.children().unwrap().size(), 3);
}

#[test]
fn method_get_parent() {
    let xml = string_based().find(&s("bar")).unwrap();
    let foo = xml.get_parent().unwrap();
    assert_eq!(foo.size(), 1);
    assert!(foo.get_parent().is_none());
}

#[test]
fn method_parents() {
    let foo = string_based();
    let xml = foo.find(&s("bar")).unwrap();
    assert_eq!(string(&foo), string(&xml.parents().unwrap().unwrap()));
    let xml = foo.children().unwrap();
    assert_eq!(xml.size(), 3);
    let foo = xml.parents().unwrap().unwrap();
    assert_eq!(foo.size(), 1);
}

#[test]
fn method_configure_value_parser() {
    classpath();
    let xml = XmlTool::new();
    assert_eq!(velocity::tools::import::RESOURCE_KEY, "resource");
    xml.configure(&map(&[(velocity::tools::import::RESOURCE_KEY, s(XML_FILE))]));
    assert_eq!(xml.size(), 1);
    assert_eq!(xml.get_name().as_str(), Some("foo"));
}

#[test]
fn method_find_object() {
    let xml = string_based();
    assert!(xml.find(&JValue::Null).is_none());
    assert_eq!(string(&xml.find(&s("bar")).unwrap()), string(&xml.find(&s("//bar")).unwrap()));
}

#[test]
fn method_get_first() {
    let xml = string_based();
    assert!(Arc::ptr_eq(&xml, &xml.get_first().unwrap()));
    let xml = xml.children().unwrap();
    assert_eq!(xml.size(), 3);
    let xml = xml.get_first().unwrap();
    assert_eq!(xml.size(), 1);
    assert_eq!(xml.get_name().as_str(), Some("a"));
}

#[test]
fn method_get_last() {
    let xml = string_based();
    assert!(Arc::ptr_eq(&xml, &xml.get_last().unwrap()));
    let xml = xml.children().unwrap();
    assert_eq!(xml.size(), 3);
    let xml = xml.get_last().unwrap();
    assert_eq!(xml.size(), 1);
    assert_eq!(xml.get_name().as_str(), Some("baz"));
    assert_eq!(xml.get_text().as_deref(), Some("wiggie"));
}

#[test]
fn method_get_name() {
    let xml = string_based();
    assert_eq!(xml.size(), 1);
    assert_eq!(xml.get_name().as_str(), Some("foo"));
    let xml = xml.find(&s("bar")).unwrap();
    assert_eq!(xml.get_name().as_str(), Some("a"));
}

#[test]
fn method_get_node_name() {
    let xml = string_based();
    assert_eq!(xml.get_node_name().as_deref(), Some("foo"));
    let xml = xml.find(&s("baz")).unwrap();
    assert_eq!(xml.get_node_name().as_deref(), Some("baz"));
}

#[test]
fn method_get_text() {
    assert_eq!(string_based().get_text().as_deref(), Some("woogie\n  wiggie"));
}

#[test]
fn method_get_number() {
    let xml = string_based();
    assert_eq!(string(&xml), string(&xml.get_index(Some(0)).unwrap()));
    let xml = xml.children().unwrap();
    assert_eq!(xml.get_index(Some(0)).unwrap().get_node_name().as_deref(), Some("bar"));
    assert_eq!(xml.get_index(Some(1)).unwrap().get_name().as_str(), Some("baz"));
    assert_eq!(xml.get_index(Some(2)).unwrap().get_name().as_str(), Some("baz"));
    assert!(xml.get_index(Some(3)).is_none());
    assert!(xml.get_index(Some(-1)).is_none());
}

#[test]
fn method_get_object() {
    let xml = string_based();
    assert!(xml.get(&JValue::Null).is_null());
    assert!(xml.get(&s("")).is_null());
    assert!(xml.get(&s("null")).is_null());
    let result = xml.get(&s("bar"));
    let xml = velocity::tools::xml_tool::as_xml_tool(&result).expect("an XmlTool");
    let result = xml.get(&s("0"));
    assert!(!result.is_null());
    assert_eq!(result.to_java_string(), string(&xml));
    let result = xml.get(&s("name"));
    assert_eq!(result.as_str(), Some("a"));
    let _ = i(0);
}

#[test]
fn method_is_empty() {
    let xml = XmlTool::new();
    assert!(xml.is_empty());
    xml.parse(Some(XML_STRING));
    assert!(!xml.is_empty());
}

#[test]
fn method_iterator() {
    let xml = XmlTool::new();
    assert!(xml.iterator().is_none());
    xml.parse(Some(XML_STRING));
    let mut i = xml.iterator().unwrap().into_iter();
    let foo = i.next().unwrap();
    assert_eq!(string(&foo), string(&xml));
    let xml = xml.children().unwrap();
    let mut i = xml.iterator().unwrap().into_iter();
    assert_eq!(i.next().unwrap().attr(&s("name")).as_deref(), Some("a"));
    assert_eq!(i.next().unwrap().get_name().as_str(), Some("baz"));
    assert_eq!(i.next().unwrap().get_text().as_deref(), Some("wiggie"));
    assert!(i.next().is_none());
}

#[test]
fn method_node() {
    let xml = XmlTool::new();
    assert!(xml.node().is_none());
    xml.parse(Some(XML_STRING));
    assert!(xml.node().is_some());
}

#[test]
fn method_parse_object() {
    let xml = XmlTool::new();
    xml.parse(None);
    assert!(xml.is_empty());
    xml.parse(Some("><S asdf8 ~$"));
    assert!(xml.is_empty());
    xml.parse(Some(XML_STRING));
    assert!(!xml.is_empty());
}

#[test]
fn method_size() {
    let xml = XmlTool::new();
    assert_eq!(xml.size(), 0);
    xml.parse(Some(XML_STRING));
    assert_eq!(xml.size(), 1);
    let xml = xml.children().unwrap();
    assert_eq!(xml.size(), 3);
    let xml = xml.get_last().unwrap();
    assert_eq!(xml.size(), 1);
}

#[test]
fn method_to_string() {
    classpath();
    let xml = XmlTool::new();
    xml.configure_values(&ValueParser::new(None));
    assert!(string(&xml).starts_with(velocity::tools::xml_tool::CLASS));
    xml.read(Some(XML_FILE));
    assert!(string(&xml).starts_with("<foo>"));
    assert!(string(&xml).ends_with("</foo>"));
    let bar = xml.find(&s("bar")).unwrap();
    assert_eq!(string(&bar), "<bar name=\"a\"/>");
    let baz = velocity::tools::xml_tool::as_xml_tool(&xml.get(&s("baz"))).unwrap();
    assert_eq!(string(&baz), "<baz>woogie</baz><baz>wiggie</baz>");
}
