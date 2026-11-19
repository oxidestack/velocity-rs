//! `org.apache.velocity.tools.generic.JsonToolTests`.

use velocity::tools::json::JsonTool;

use crate::base::{classpath, map, s};

#[test]
fn test_json() {
    classpath();
    let json = JsonTool::new();
    json.configure(&map(&[("resource", s("foo.json"))]));
    assert_eq!(json.get("foo").unwrap().as_str(), Some("bar"));
}
