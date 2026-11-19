//! `org.apache.velocity.tools.generic.LinkToolTests`.

use std::sync::Arc;

use java_compat::lang::{JMap, JValue, MapKind};
use velocity::tools::link::*;
use velocity::tools::uri::Uri;

use crate::base::{i, map, s};

/// `DEFAULT_PROPS`: configuration not locked, for testing.
fn default_props() -> JMap {
    map(&[(SAFE_MODE_KEY, JValue::Bool(false)), (LOCK_CONFIG_KEY, JValue::Bool(false))])
}

fn new_instance() -> Arc<LinkTool> {
    let link = LinkTool::new();
    link.configure(&default_props());
    link
}

fn new_instance_uri(uri: &str) -> Arc<LinkTool> {
    new_instance_with(URI_KEY, s(uri))
}

fn new_instance_with(key: &str, value: JValue) -> Arc<LinkTool> {
    let link = LinkTool::new();
    let props = default_props();
    props.put_str(key, value);
    link.configure(&props);
    link
}

fn some(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn string(l: &LinkTool) -> Option<String> {
    l.java_string_opt()
}

fn c(ch: char) -> JValue {
    JValue::Char(ch)
}

fn b(v: bool) -> JValue {
    JValue::Bool(v)
}

const NULL: JValue = JValue::Null;

#[test]
fn ctor_link_tool() {
    LinkTool::new();
}

#[test]
fn method_configure_value_parser() {
    let link = new_instance_uri("mailto:nbubna@apache.org");
    assert_eq!(link.get_scheme(), some("mailto"));
    assert_eq!(string(&link), some("mailto:nbubna@apache.org"));
    assert!(link.get_uri().unwrap().is_opaque());
    assert!(link.is_absolute());
}

#[test]
fn method_duplicate() {
    let link = new_instance_uri("http://apache.org/foo.html");
    let result = link.duplicate(false);
    assert!(!Arc::ptr_eq(&link, &result));
    assert_eq!(link.get_scheme(), result.get_scheme());
    assert_eq!(link.get_user(), result.get_user());
    assert_eq!(link.get_host(), result.get_host());
    assert_eq!(link.get_path(), result.get_path());
    assert_eq!(link.query_map().is_none(), result.query_map().is_none());
    assert_eq!(link.get_anchor(), result.get_anchor());
    assert!(Arc::ptr_eq(&link.get_self(), &result.get_self()));
}

#[test]
fn method_duplicate_boolean() {
    let link = new_instance_uri("http://apache.org/foo.html?foo=bar");
    let result = link.duplicate(true);
    assert!(!Arc::ptr_eq(&link, &result));
    assert_eq!(link.get_path(), result.get_path());
    assert!(!link.query_map().unwrap().same(&result.query_map().unwrap()));
    assert_eq!(link.get_query(), result.get_query());
    assert!(Arc::ptr_eq(&link.get_self(), &result.get_self()));
    assert!(result.query_map().unwrap().same(&result.duplicate(false).query_map().unwrap()));
}

#[test]
fn method_encode_object() {
    let link = new_instance();
    assert_eq!(link.encode(&s("this has spaces")), some("this+has+spaces"));
    assert_eq!(link.encode(&s("@/ ")), some("%40%2F+"));
}

#[test]
fn method_decode_object() {
    let link = new_instance();
    assert_eq!(link.decode(&s("this+has+spaces")).unwrap(), some("this has spaces"));
    assert_eq!(link.decode(&s("%40%2F+")).unwrap(), some("@/ "));
}

#[test]
fn method_set_scheme_object() {
    let link = new_instance_with(SCHEME_KEY, s(DEFAULT_SCHEME));
    assert_eq!(link.get_scheme(), some(DEFAULT_SCHEME));
    link.set_scheme(&NULL);
    assert_eq!(link.get_scheme(), None);
    link.set_scheme(&s("foo:"));
    assert_eq!(link.get_scheme(), some("foo"));
}

#[test]
fn method_scheme_object() {
    let link = new_instance();
    let result = link.scheme(&NULL);
    assert_eq!(result.get_scheme(), None);
    let link = new_instance_uri("https://apache.org");
    assert_eq!(link.get_path(), None);
    assert_eq!(string(&link), some("https://apache.org"));
    assert_eq!(link.get_scheme(), some(SECURE_SCHEME));
    assert!(link.is_secure());
    let result = link.scheme(&s(DEFAULT_SCHEME));
    assert_eq!(string(&result), some("http://apache.org"));
    assert!(!result.is_secure());
}

#[test]
fn method_get_scheme() {
    let link = new_instance();
    assert_eq!(link.get_scheme(), None);
    assert_eq!(link.scheme(&s("mailto")).get_scheme(), some("mailto"));
}

#[test]
fn method_secure() {
    let link = new_instance();
    assert!(!link.is_secure());
    assert!(link.secure().is_secure());
}

#[test]
fn method_insecure() {
    let link = new_instance_uri("https://apache.org");
    assert!(link.is_secure());
    assert!(!link.insecure().is_secure());
}

#[test]
fn method_is_absolute() {
    let link = new_instance();
    assert!(!link.is_absolute());
    assert!(link.absolute_to(&s("http://apache.org")).unwrap().is_absolute());
}

#[test]
fn method_set_user_info_object() {
    let link = new_instance_with(USER_KEY, s("nbubna"));
    assert_eq!(link.get_user(), some("nbubna"));
    link.set_user_info(&NULL);
    assert_eq!(link.get_user(), None);
    link.set_user_info(&s("@#$ /@!"));
    assert_eq!(link.get_user(), some("@#$ /@!"));
}

#[test]
fn method_get_user() {
    let link = new_instance();
    assert_eq!(link.get_user(), None);
    let link = new_instance_uri("http://nbubna@apache.org");
    assert_eq!(link.get_user(), some("nbubna"));
}

#[test]
fn method_user_object() {
    let link = new_instance_uri("http://nbubna@apache.org");
    assert_eq!(link.user(&NULL).get_user(), None);
    assert_eq!(link.user(&s("nbubna")).get_user(), some("nbubna"));
    assert_eq!(link.user(&s("@#$ /!")).get_user(), some("@#$ /!"));
    assert_eq!(string(&link.user(&s("@#$ /!"))), some("http://%40%23$%20%2F!@apache.org"));
}

#[test]
fn method_get_host() {
    let link = new_instance_uri("http://apache.org");
    assert_eq!(link.get_host(), some("apache.org"));
    link.set_from_uri(&s("http://velocity.apache.org/tools/devel/"));
    assert_eq!(link.get_host(), some("velocity.apache.org"));
}

#[test]
fn method_host_object() {
    let link = new_instance();
    assert_eq!(link.host(&s("apache.org")).get_host(), some("apache.org"));
    let link = new_instance_uri("https://nbubna@www.apache.org");
    assert_eq!(string(&link.host(&s("people.apache.org"))), some("https://nbubna@people.apache.org"));
}

#[test]
fn method_set_host_object() {
    let link = new_instance();
    link.set_host(&s("foo.com"));
    assert_eq!(link.get_host(), some("foo.com"));
}

#[test]
fn method_get_port() {
    let link = new_instance();
    assert_eq!(link.get_port(), None);
    let link = new_instance_with(PORT_KEY, i(42));
    assert_eq!(link.get_port(), Some(42));
}

#[test]
fn method_port_object() {
    let link = new_instance();
    assert_eq!(link.port(&NULL).get_port(), None);
    assert_eq!(link.port(&s(":asd")).get_port(), None);
    assert_eq!(link.port(&i(1)).get_port(), Some(1));
    assert_eq!(link.port(&s("42")).get_port(), Some(42));
}

#[test]
fn method_set_port_object() {
    let link = new_instance();
    link.set_port(&i(42));
    assert_eq!(link.get_port(), Some(42));
}

#[test]
fn method_get_path() {
    let link = new_instance();
    assert_eq!(link.get_path(), None);
    let link = new_instance_uri("http://velocity.apache.org/tools/devel");
    assert_eq!(link.get_path(), some("/tools/devel"));
}

#[test]
fn method_set_path_object() {
    let link = new_instance();
    assert_eq!(link.get_path(), None);
    link.set_path(&s("foo"));
    assert_eq!(link.get_path(), some("/foo"));
    link.set_path(&s("/foo"));
    assert_eq!(link.get_path(), some("/foo"));
    link.set_path(&s("/foo/"));
    assert_eq!(link.get_path(), some("/foo/"));
    link.set_path(&s("/foo/"));
    assert_eq!(link.get_path(), some("/foo/"));
}

#[test]
fn method_path_object() {
    let link = new_instance();
    assert_eq!(link.get_path(), None);
    assert_eq!(link.path(&s("bar")).get_path(), some("/bar"));
    assert_eq!(link.path(&s("/bar")).get_path(), some("/bar"));
    assert_eq!(link.path(&s("bar/")).get_path(), some("/bar/"));
    assert_eq!(link.path(&s("/bar/")).get_path(), some("/bar/"));
    let link = new_instance_uri("http://foo.com/this/that.vm");
    assert_eq!(string(&link), some("http://foo.com/this/that.vm"));
    assert_eq!(string(&link.path(&s("bar.vm"))), some("http://foo.com/bar.vm"));
}

#[test]
fn method_combine_path_string_string() {
    let link = new_instance();
    let (none, empty, test, starts, ends, both) = (None, Some(""), Some("test"), Some("/this"), Some("that/"), Some("/these/"));
    assert_eq!(link.combine_path(none, none), None);
    assert_eq!(link.combine_path(none, empty), some(""));
    assert_eq!(link.combine_path(empty, none), some(""));
    assert_eq!(link.combine_path(empty, test), some("/test"));
    assert_eq!(link.combine_path(test, empty), some("test/"));
    assert_eq!(link.combine_path(starts, starts), some("/this/this"));
    assert_eq!(link.combine_path(ends, ends), some("that/that/"));
    assert_eq!(link.combine_path(starts, ends), some("/this/that/"));
    assert_eq!(link.combine_path(ends, starts), some("that/this"));
    assert_eq!(link.combine_path(both, both), some("/these/these/"));
}

#[test]
fn method_append_path_object() {
    let link = new_instance_with(PATH_KEY, s("/foo"));
    assert_eq!(link.get_path(), some("/foo"));
    link.append_path(&s("bar"));
    assert_eq!(link.get_path(), some("/foo/bar"));
    link.append_path(&s("/bar"));
    assert_eq!(link.get_path(), some("/foo/bar/bar"));
    link.set_path(&s("/foo/"));
    link.append_path(&s("bar/"));
    assert_eq!(link.get_path(), some("/foo/bar/"));
    link.append_path(&s("/bar"));
    assert_eq!(link.get_path(), some("/foo/bar/bar"));
}

#[test]
fn method_append_object() {
    let link = new_instance_with(PATH_KEY, s("/foo"));
    assert_eq!(link.append(&NULL).get_path(), some("/foo"));
    link.set_path(&NULL);
    assert_eq!(link.get_path(), None);
    let link = link.append(&s("bar"));
    assert_eq!(link.get_path(), some("/bar"));
    assert_eq!(link.append(&s("foo")).get_path(), some("/bar/foo"));
}

#[test]
fn method_get_directory() {
    let link = new_instance_uri("http://foo.com/ctx/request.vm?this=that#anc");
    assert_eq!(link.get_directory(), some("/ctx/"));
    assert_eq!(new_instance_uri("http://foo.com").get_directory(), None);
    assert_eq!(new_instance_uri("http://foo.com/bar").get_directory(), some("/"));
    assert_eq!(new_instance_uri("http://foo.com/bar/foo/bar/foo").get_directory(), some("/bar/foo/bar/"));
}

#[test]
fn method_get_file() {
    assert_eq!(new_instance_uri("http://foo.com/ctx/request.vm?this=that#anc").get_file(), some("request.vm"));
    assert_eq!(new_instance_uri("http://foo.com/foo/bar/request.vm?this=that#anc").get_file(), some("request.vm"));
    assert_eq!(new_instance_uri("http://foo.com/bar/").get_file(), some(""));
}

#[test]
fn method_get_root() {
    let link = new_instance_uri("http://foo.com/ctx/request.vm?this=that#anc");
    assert_eq!(link.get_root(), some("http://foo.com"));
    link.set_host(&s("apache.org"));
    assert_eq!(link.secure().get_root(), some("https://apache.org"));
}

#[test]
fn method_directory() {
    assert_eq!(string(&new_instance_uri("http://foo.com/ctx/request.vm?this=that#anc").directory()), some("http://foo.com/ctx/"));
    assert_eq!(string(&new_instance_uri("http://foo.com").directory()), some("http://foo.com"));
}

#[test]
fn method_root() {
    assert_eq!(string(&new_instance_uri("http://foo.com/ctx/request.vm?this=that#anc").root().unwrap()), some("http://foo.com"));
    assert_eq!(string(&new_instance_uri("http://foo.com").root().unwrap()), some("http://foo.com"));
    assert!(new_instance_uri("dev@velocity.apache.org").root().is_none());
}

#[test]
fn method_set_force_relative_boolean() {
    let link = new_instance_uri("http://apache.org/bar");
    assert_eq!(string(&link), some("http://apache.org/bar"));
    link.set_force_relative(true);
    assert_eq!(string(&link), some("/bar"));
}

#[test]
fn method_relative_object() {
    let link = new_instance_uri("/ctx/request.vm?this=that#anc");
    assert_eq!(link.get_path(), some("/ctx/request.vm"));
    assert_eq!(link.get_directory(), some("/ctx/"));
    assert_eq!(link.get_query(), some("this=that"));
    assert_eq!(link.get_anchor(), some("anc"));
    assert_eq!(string(&link), some("/ctx/request.vm?this=that#anc"));
    assert_eq!(string(&link.relative_to(&NULL).unwrap()), some("/ctx/?this=that#anc"));
    assert_eq!(string(&link.relative_to(&s("other.vm")).unwrap()), some("/ctx/other.vm?this=that#anc"));
    let link = new_instance_uri("http://foo.com/bar/");
    assert_eq!(string(&link.relative_to(&s("woogie.vm")).unwrap()), some("/bar/woogie.vm"));
    let link = new_instance_uri("/bar/");
    assert_eq!(string(&link.relative_to(&s("foo/woogie.vm")).unwrap()), some("/bar/foo/woogie.vm"));
    assert_eq!(string(&link.relative_to(&s("yo")).unwrap()), some("/bar/yo"));
}

#[test]
fn method_is_relative() {
    assert!(new_instance_uri("/ctx/request.vm?this=that#anc").is_relative());
    let link = new_instance_uri("http://foo.com/bar.vm?q=woogie");
    assert!(!link.is_relative());
    assert!(link.relative().is_relative());
    assert!(new_instance_uri("http://apache.org").relative_to(&s("foo.vm")).unwrap().is_relative());
}

#[test]
fn method_absolute_object() {
    let link = new_instance();
    let _ = link.absolute_to(&NULL);
    let result = link.absolute_to(&s("http://apache.org")).unwrap();
    assert!(result.equals(&link.uri(&s("http://apache.org")).unwrap()));
    assert!(result.is_absolute());
    assert_eq!(string(&result), some("http://apache.org"));
    assert_eq!(result.get_scheme(), some(DEFAULT_SCHEME));
    assert_eq!(result.get_host(), some("apache.org"));
    assert!(!link.is_absolute());
    let result = link.absolute_to(&s("/test/foo.vm")).unwrap();
    assert!(result.is_absolute());
    assert_eq!(result.get_path(), some("/test/foo.vm"));
    assert_eq!(result.get_host(), None);
    assert_eq!(string(&result), some("http:/test/foo.vm"));
    let result = result.host(&s("apache.org"));
    assert_eq!(string(&result), some("http://apache.org/test/foo.vm"));
    let result = result.absolute_to(&s("bar.vm")).unwrap();
    assert_eq!(string(&result), some("http://apache.org/test/bar.vm"));
    let result = result.absolute_to(&s("/woogie.vm")).unwrap();
    assert_eq!(string(&result), some("http://apache.org/woogie.vm"));
}

#[test]
fn method_get_base_ref() {
    assert_eq!(new_instance_uri("http://foo.com/ctx/request.vm?this=that#anc").get_base_ref(), some("http://foo.com/ctx/request.vm"));
    assert_eq!(new_instance().get_base_ref(), None);
}

#[test]
fn method_get_uri() {
    let link = new_instance_with(SAFE_MODE_KEY, b(true));
    link.set_from_uri(&s("http://velocity.apache.org"));
    assert!(link.get_uri().is_none());
    let link = new_instance();
    assert!(link.get_uri().is_none());
    let link = link.secure().user(&s("nbubna")).host(&s("people.apache.org"));
    assert_eq!(link.get_uri().unwrap().get_user_info(), some("nbubna"));
}

#[test]
fn method_set_from_uri_object() {
    let link = new_instance();
    assert_eq!(string(&link), None);
    link.set_from_uri(&s("*%&$^%#$*&^!"));
    assert_eq!(string(&link), None);
    link.set_from_uri(&s("http://velocity.apache.org"));
    assert!(string(&link).is_some());
    assert_eq!(link.get_host(), some("velocity.apache.org"));
}

/// `new Object() { toString() { "http://google.com" } }`.
struct Stringy;

impl java_compat::lang::JObject for Stringy {
    fn class_name(&self) -> String {
        "org.apache.velocity.tools.generic.LinkToolTests$1".into()
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<Result<Option<JValue>, java_compat::lang::JError>> {
        None
    }

    fn to_java_string(&self) -> String {
        "http://google.com".into()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn method_to_uri_object() {
    let link = new_instance();
    let uri = Uri::parse("http://apache.org").unwrap().value();
    let same = link.to_uri(&uri).unwrap();
    assert!(Arc::ptr_eq(&same, &velocity::tools::uri::as_uri(&uri).unwrap()));
    assert!(link.to_uri(&s("*%&$^%#$*&^!")).is_none());
    assert!(link.to_uri(&s("http://velocity.apache.org")).is_some());
    assert!(link.to_uri(&JValue::Object(Arc::new(Stringy))).is_some());
}

#[test]
fn method_create_uri() {
    let link = new_instance();
    assert!(link.create_uri().is_none());
    link.set_from_uri(&s("http://velocity.apache.org"));
    assert!(link.create_uri().is_some());
    link.set_port(&s("foo"));
    assert!(link.create_uri().is_none());
    link.set_port(&NULL);
    assert!(link.set_from_uri(&s("mailto:nbubna@apache.org")));
    assert!(link.is_opaque());
    assert!(link.create_uri().is_some());
    assert!(link.create_uri().unwrap().is_opaque());
    assert_eq!(link.create_uri(), link.create_uri());
}

#[test]
fn method_uri_object() {
    let link = new_instance();
    assert_eq!(string(&link.uri(&NULL).unwrap()), None);
    assert_eq!(string(&link.uri(&s("http://apache.org?a=b#c")).unwrap()), some("http://apache.org?a=b#c"));
    link.set_from_uri(&s("https://nbubna@people.apache.org"));
    assert_eq!(link.get_host(), some("people.apache.org"));
    assert_eq!(string(&link.uri(&link.create_uri().unwrap().value()).unwrap()), some("https://nbubna@people.apache.org"));
    let uri = Uri::parse("mailto:nbubna@apache.org").unwrap().value();
    assert_eq!(string(&link.uri(&uri).unwrap()), some("mailto:nbubna@apache.org"));
}

#[test]
fn method_set_append_params_boolean() {
    let link = new_instance_uri("/bar?a=b");
    assert_eq!(link.param(&s("a"), &s("c")).get_query(), some("a=b&amp;a=c"));
    link.set_append_params(false);
    assert_eq!(link.param(&s("a"), &s("d")).get_query(), some("a=d"));
}

#[test]
fn method_set_query_object() {
    let link = new_instance_uri("/bar?a=b");
    assert_eq!(link.get_query(), some("a=b"));
    link.set_query(&s("c=d&e=f"));
    assert_eq!(link.get_query(), some("c=d&amp;e=f"));
    link.set_xhtml(false);
    link.set_query(&s("x=1&amp;y=2"));
    assert_eq!(link.get_query(), some("x=1&y=2"));
    link.set_query(&NULL);
    assert_eq!(link.get_query(), None);
}

#[test]
fn method_normalize_query_string() {
    let link = LinkTool::new();
    assert_eq!(link.normalize_query("a=b"), "a=b");
    assert_eq!(link.normalize_query("a=b&b=c"), "a=b&amp;b=c");
    assert_eq!(link.normalize_query("a=b&amp;b=c"), "a=b&amp;b=c");
    assert_eq!(link.normalize_query("a=b&amp;b=c&t=f"), "a=b&amp;b=c&amp;t=f");
}

#[test]
fn method_get_query() {
    let link = new_instance();
    assert_eq!(link.get_query(), None);
    assert_eq!(link.query(&s("this=that")).get_query(), some("this=that"));
}

#[test]
fn method_query_object() {
    let link = new_instance_uri("https://gmail.com");
    assert_eq!(string(&link.query(&NULL)), some("https://gmail.com"));
    assert_eq!(string(&link.query(&s("v=2"))), some("https://gmail.com?v=2"));
    let link = new_instance_uri("http://go.com?foo=bar");
    assert_eq!(link.query(&s("foo=wog")).get_query(), some("foo=wog"));
    assert_eq!(string(&link.query(&NULL)), some("http://go.com"));
}

#[test]
fn method_combine_query_string_string() {
    let link = new_instance();
    let (none, empty, test, test2) = (None, Some(""), Some("test=1"), Some("a=b"));
    assert_eq!(link.combine_query(none, none), None);
    assert_eq!(link.combine_query(none, empty), None);
    assert_eq!(link.combine_query(test, none), some("test=1"));
    assert_eq!(link.combine_query(test, empty), some("test=1"));
    assert_eq!(link.combine_query(test, test2), some("test=1&amp;a=b"));
    link.set_xhtml(false);
    assert_eq!(link.combine_query(test2, test), some("a=b&test=1"));
}

#[test]
fn method_append_query_object() {
    let link = new_instance_uri("/foo?bar=woogie");
    link.append_query(&s("x=1"));
    assert_eq!(link.get_query(), some("bar=woogie&amp;x=1"));
    link.append_query(&s("y=2"));
    assert_eq!(link.get_query(), some("bar=woogie&amp;x=1&amp;y=2"));
    link.set_query(&NULL);
    assert_eq!(link.get_query(), None);
    link.append_query(&s("z=3"));
    assert_eq!(link.get_query(), some("z=3"));
}

#[test]
fn method_to_query_map() {
    let link = LinkTool::new();
    let test = JMap::new(MapKind::Linked);
    test.put_str("a", s("b"));
    assert_eq!(link.to_query_map(Some(&test)), some("a=b"));
    test.put_str("b", s("c"));
    assert_eq!(link.to_query_map(Some(&test)), some("a=b&amp;b=c"));
    test.put_str("b", booleans(&[true, false]));
    assert_eq!(link.to_query_map(Some(&test)), some("a=b&amp;b=true&amp;b=false"));
}

#[test]
fn method_to_query_object_object() {
    let link = new_instance();
    assert_eq!(link.to_query(&NULL, &NULL), "null=");
    assert_eq!(link.to_query(&s("a b"), &s("c")), "a+b=c");
    assert_eq!(link.to_query(&c('x'), &i(1)), "x=1");
    assert_eq!(link.to_query(&b(true), &b(false)), "true=false");
    assert_eq!(link.to_query(&s("path"), &s("/foo bar/new")), "path=%2Ffoo+bar%2Fnew");
    assert_eq!(link.to_query(&c('x'), &s(",;:$&+=?/[]@")), "x=%2C%3B%3A%24%26%2B%3D%3F%2F%5B%5D%40");
}

#[test]
fn method_set_param_objectobjectboolean() {
    let link = new_instance();
    assert!(link.query_map().is_none());
    link.set_param(&s("a"), &s("b"), true);
    assert!(link.query_map().is_some());
    assert_eq!(link.get_query(), some("a=b"));
    link.set_param(&s("a"), &s("c"), true);
    assert_eq!(link.get_query(), some("a=b&amp;a=c"));
    link.set_param(&s("a"), &s("foo"), false);
    assert_eq!(link.get_query(), some("a=foo"));
}

#[test]
fn method_set_params_objectboolean() {
    let link = new_instance();
    assert!(link.query_map().is_none());
    link.set_params(&s("a=b"), true);
    assert!(link.query_map().is_some());
    assert_eq!(link.get_query(), some("a=b"));
    link.set_params(&s("a=c&a=d"), true);
    assert_eq!(link.get_query(), some("a=b&amp;a=c&amp;a=d"));
    let test = JMap::new(MapKind::Linked);
    test.put_str("a", s("foo"));
    link.set_params(&JValue::Map(test), false);
    assert_eq!(link.get_query(), some("a=foo"));
}

#[test]
fn method_params_object() {
    let link = new_instance();
    let test = JMap::new(MapKind::Linked);
    test.put_str("a", s("foo"));
    assert_eq!(link.params(&JValue::Map(test)).get_query(), some("a=foo"));
    assert_eq!(link.params(&s("a=b&b=c")).get_query(), some("a=b&amp;b=c"));
    let link = new_instance_uri("/foo?q=a&a=q");
    assert_eq!(string(&link.params(&b(false))), some("/foo"));
    assert_eq!(string(&link.params(&b(true))), some("/foo?q=a&amp;a=q"));
}

#[test]
fn method_param_object_object() {
    let link = new_instance();
    assert_eq!(link.param(&NULL, &NULL).get_query(), some("null="));
    assert_eq!(link.param(&s("x"), &i(1)).get_query(), some("x=1"));
    assert_eq!(link.param(&s("x"), &i(1)).param(&s("y"), &i(2)).get_query(), some("x=1&amp;y=2"));
    let link = new_instance_uri("/hee/haa.vm?a=b");
    assert_eq!(string(&link.param(&c('b'), &b(true))), some("/hee/haa.vm?a=b&amp;b=true"));
}

#[test]
fn method_append_object_object() {
    let link = new_instance();
    link.set_append_params(false);
    assert_eq!(link.append_param_copy(&s("x"), &i(1)).get_query(), some("x=1"));
    assert_eq!(link.append_param_copy(&s("x"), &i(1)).append_param_copy(&s("x"), &i(2)).get_query(), some("x=1&amp;x=2"));
}

#[test]
fn method_set_object_object() {
    let link = new_instance();
    link.set_append_params(true);
    assert_eq!(link.set(&s("x"), &i(1)).get_query(), some("x=1"));
    assert_eq!(link.set(&s("x"), &i(1)).set(&s("x"), &i(2)).get_query(), some("x=2"));
}

#[test]
fn method_remove_object() {
    let link = new_instance_uri("/foo?q=bar");
    assert_eq!(link.get_query(), some("q=bar"));
    assert_eq!(link.remove(&s("q")).get_query(), some(""));
    assert_eq!(link.set(&s("x"), &i(1)).remove(&s("x")).get_query(), some("q=bar"));
}

#[test]
fn method_remove_param_object() {
    let link = new_instance();
    assert!(link.remove_param(&s("a")).is_null());
    link.set_param(&s("a"), &s("b"), true);
    assert_eq!(link.get_query(), some("a=b"));
    assert_eq!(link.remove_param(&s("a")).as_str(), Some("b"));
    assert!(link.remove_param(&s("a")).is_null());
}

#[test]
fn method_handle_params_boolean_boolean() {
    let link = new_instance();
    assert!(link.query_map().is_none());
    link.set_param(&s("a"), &s("b"), true);
    let q = link.query_map().unwrap();
    link.handle_params_boolean(true);
    assert!(q.same(&link.query_map().unwrap()));
    assert_eq!(link.get_query(), some("a=b"));
    link.handle_params_boolean(false);
    assert!(link.query_map().is_none());
}

#[test]
fn method_params_map() {
    let link = new_instance_uri("http://go.com");
    let params = JMap::new(MapKind::Linked);
    params.put_str("this", s("that"));
    params.put(c('x'), i(1));
    params.put(b(true), b(false));
    assert_eq!(string(&link.params(&JValue::Map(params))), some("http://go.com?this=that&amp;x=1&amp;true=false"));
    assert_eq!(string(&link.params(&NULL)), some("http://go.com"));
    assert_eq!(string(&link.params(&JValue::Map(JMap::new(MapKind::Hash)))), some("http://go.com"));
}

#[test]
fn method_parse_query_string() {
    let link = new_instance();
    let result = link.parse_query("a=b&x=1").unwrap();
    assert_eq!(result.get_str("a").unwrap().as_str(), Some("b"));
    assert_eq!(result.get_str("x").unwrap().as_str(), Some("1"));
    link.set_xhtml(false);
    let result = link.parse_query("true=false&amp;false=true&black=white").unwrap();
    assert_eq!(result.get_str("true").unwrap().as_str(), Some("false"));
    assert_eq!(result.get_str("false").unwrap().as_str(), Some("true"));
    assert_eq!(result.get_str("black").unwrap().as_str(), Some("white"));
}

#[test]
fn method_get_params() {
    let link = new_instance_uri("/foo?a=b&amp;x=true");
    let result = link.get_params().unwrap();
    assert_eq!(result.get_str("a").unwrap().as_str(), Some("b"));
    assert_eq!(result.get_str("x").unwrap().as_str(), Some("true"));
    let newresult = link.param(&c('y'), &b(false)).get_params().unwrap();
    assert!(!java_compat::lang::java_equals(&JValue::Map(result), &JValue::Map(newresult.clone())));
    assert_eq!(newresult.get_str("a").unwrap().as_str(), Some("b"));
    assert!(matches!(newresult.get_str("y"), Some(JValue::Bool(false))));
}

#[test]
fn method_set_fragment_object() {
    let link = new_instance();
    link.set_fragment(&s("foo"));
    assert_eq!(string(&link), some("#foo"));
    link.set_fragment(&NULL);
    assert_eq!(string(&link), None);
    let link = new_instance_uri("/foo#bar");
    link.set_fragment(&s("woo gie"));
    assert_eq!(string(&link), some("/foo#woo%20gie"));
}

#[test]
fn method_get_anchor() {
    let link = new_instance();
    assert_eq!(link.get_anchor(), None);
    link.set_fragment(&s("foo"));
    assert_eq!(link.get_anchor(), some("foo"));
    assert_eq!(new_instance_uri("http://go.com#espn").get_anchor(), some("espn"));
    assert_eq!(new_instance_with(FRAGMENT_KEY, s("foo")).get_anchor(), some("foo"));
}

#[test]
fn method_anchor_object() {
    let link = new_instance();
    assert_eq!(link.anchor(&NULL).get_anchor(), None);
    assert_eq!(link.anchor(&s("")).get_anchor(), None);
    assert_eq!(link.anchor(&s("test")).get_anchor(), some("test"));
    assert_eq!(link.anchor(&s("a b")).get_anchor(), some("a b"));
    let link = new_instance_uri("http://go.com#foo");
    assert_eq!(string(&link.anchor(&b(true))), some("http://go.com#true"));
    assert_eq!(string(&link.anchor(&s("a b"))), some("http://go.com#a%20b"));
}

#[test]
fn method_get_self() {
    let link = new_instance();
    assert!(Arc::ptr_eq(&link, &link.get_self()));
    assert!(Arc::ptr_eq(&link, &link.uri(&s("http://go.com")).unwrap().get_self()));
    assert!(Arc::ptr_eq(&link, &link.path(&s("foo")).param(&i(1), &b(true)).anchor(&c('a')).get_self()));
}

#[test]
fn method_to_string() {
    let link = new_instance();
    assert_eq!(string(&link), None);
    assert_eq!(string(&link.secure()), None);
    assert_eq!(string(&link.host(&s("go.com"))), some("http://go.com"));
    assert_eq!(string(&link.port(&i(42))), None);
    assert_eq!(string(&link.path(&s("foo"))), some("/foo"));
    assert_eq!(string(&link.param(&c('a'), &i(1))), some("?a=1"));
    assert_eq!(string(&link.anchor(&i(42))), some("#42"));
}

#[test]
fn method_no_double_encode() {
    let link = new_instance().relative_to(&s("/foo")).unwrap();
    assert_eq!(string(&link), some("/foo"));
    let link = link.param(&s("q"), &s("a:b c"));
    assert_eq!(string(&link), some("/foo?q=a%3Ab+c"));
    let link = link.anchor(&s("a(b, c)"));
    assert_eq!(string(&link), some("/foo?q=a%3Ab+c#a(b,%20c)"));
    let link = link.param(&s("evil"), &s("%25%24%")).anchor(&NULL);
    assert_eq!(string(&link), some("/foo?q=a%3Ab+c&amp;evil=%2525%2524%25"));
}
