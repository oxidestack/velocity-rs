//! `org.apache.velocity.tools.generic.CollectionToolTests`.

use java_compat::lang::{JList, JValue, ListKind};
use velocity::tools::collection::CollectionTool;

use crate::base::s;

#[test]
fn test_null_elements() {
    let tool = CollectionTool::new();
    let lst = JValue::List(JList::new(ListKind::Fixed, vec![s("b"), JValue::Null, s("a")]));
    let JValue::List(sorted) = tool.sort(&lst).unwrap() else { panic!("a list") };
    assert_eq!(sorted.len(), 2);
    assert_eq!(sorted.get(0).unwrap().as_str(), Some("a"));
    assert_eq!(sorted.get(1).unwrap().as_str(), Some("b"));
    let arr = java_compat::lang::JArray::new(java_compat::reflect::JType::string(), vec![s("foo"), JValue::Null, s("bar")]);
    let JValue::List(sorted) = tool.sort(&JValue::Array(std::sync::Arc::new(arr))).unwrap() else { panic!("a list") };
    assert_eq!(sorted.len(), 2);
    assert_eq!(sorted.get(0).unwrap().as_str(), Some("bar"));
    assert_eq!(sorted.get(1).unwrap().as_str(), Some("foo"));
}
