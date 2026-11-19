//! `IfEmptyTestCase` and `IfEmptyNoEmptyCheckTestCase`.

use java_compat::jdk::StringBuilder;
use java_compat::lang::{JArray, JList, JMap, JValue, ListKind, MapKind};

use crate::base::*;

fn assert_empty(b: &mut Base, v: JValue) {
    b.put("obj", v);
    b.assert_eval_equals("", "#if( $obj )fail#end");
}

fn assert_not_empty(b: &mut Base, v: JValue) {
    b.put("obj", v);
    b.assert_eval_equals("", "#if( !$obj )fail#end");
}

pub fn null_as_string() -> JValue {
    class("org.apache.velocity.test.IfEmptyTestCase$NullAsString").method("getAsString", &[], Some(string()), |_, _| ret(JValue::Null)).build().new_instance()
}

pub fn empty_as_string() -> JValue {
    class("org.apache.velocity.test.IfEmptyTestCase$EmptyAsString").method("getAsString", &[], Some(string()), |_, _| ret(s(""))).build().new_instance()
}

pub fn null_as_number() -> JValue {
    class("org.apache.velocity.test.IfEmptyTestCase$NullAsNumber").method("getAsNumber", &[], Some(string()), |_, _| ret(JValue::Null)).build().new_instance()
}

#[test]
fn test_null() {
    let mut b = Base::new();
    assert_empty(&mut b, JValue::Null);
    assert_empty(&mut b, null_as_string());
    assert_empty(&mut b, null_as_number());
}

#[test]
fn test_data_structures() {
    let mut b = Base::new();
    assert_empty(&mut b, JValue::Map(JMap::new(MapKind::Fixed)));
    assert_empty(&mut b, JValue::List(JList::new(ListKind::Fixed, vec![])));
    assert_empty(&mut b, JArray::objects(vec![]));
    assert_not_empty(&mut b, JValue::List(JList::array_list(vec![JValue::Int(1)])));
    let map = JMap::new(MapKind::Linked);
    map.put(s("foo"), JValue::Int(1));
    assert_not_empty(&mut b, JValue::Map(map));
}

#[test]
fn test_string() {
    let mut b = Base::new();
    assert_empty(&mut b, s(""));
    assert_empty(&mut b, empty_as_string());
    assert_not_empty(&mut b, s("hello"));
}

#[test]
fn test_number() {
    let mut b = Base::new();
    for v in [JValue::Int(0), JValue::Long(0), JValue::Float(0.0), JValue::Double(0.0), big_integer(0), big_decimal("0")] {
        assert_empty(&mut b, v);
    }
    for v in [JValue::Int(1), JValue::Long(1), JValue::Float(1.0), JValue::Double(1.0), big_integer(1), big_decimal("1")] {
        assert_not_empty(&mut b, v);
    }
}

#[test]
fn test_string_builder() {
    let mut b = Base::new();
    let builder = StringBuilder::new("");
    assert_empty(&mut b, JValue::Object(builder.clone()));
    builder.append("yo");
    assert_not_empty(&mut b, JValue::Object(builder));
}

#[test]
fn test_literals() {
    let mut b = Base::new();
    for t in [
        "#if( 0 )fail#end",
        "#if( 0.0 )fail#end",
        "#if( '' )fail#end",
        "#if( \"\" )fail#end",
        "#if( [] )fail#end",
        "#if( {} )fail#end",
        "#if( !1 )fail#end",
        "#if( !1.0 )fail#end",
        "#if( !'foo' )fail#end",
        "#if( !\"foo\" )fail#end",
        "#if( ![ 'foo' ] )fail#end",
        "#if( !{ 'foo':'bar' } )fail#end",
    ] {
        b.assert_eval_equals("", t);
    }
}

/// `IfEmptyNoEmptyCheckTestCase`: with `directive.if.empty_check = false` only null is false.
mod no_empty_check {
    use super::*;

    const PREFIX: &str = "org.apache.velocity.test.IfEmptyNoEmptyCheckTestCase$";

    fn base() -> Base {
        let mut b = Base::new();
        b.set_property("directive.if.empty_check", "false");
        b
    }

    fn fixture(name: &str, method: &str, v: JValue) -> JValue {
        class(&format!("{PREFIX}{name}")).method(method, &[], Some(string()), move |_, _| ret(v.clone())).build().new_instance()
    }

    #[test]
    fn test_null() {
        let mut b = base();
        assert_empty(&mut b, JValue::Null);
        assert_not_empty(&mut b, fixture("NullAsString", "getAsString", JValue::Null));
        assert_not_empty(&mut b, fixture("NullAsNumber", "getAsNumber", JValue::Null));
    }

    #[test]
    fn test_data_structures() {
        let mut b = base();
        assert_not_empty(&mut b, JValue::Map(JMap::new(MapKind::Fixed)));
        assert_not_empty(&mut b, JValue::List(JList::new(ListKind::Fixed, vec![])));
        assert_not_empty(&mut b, JArray::objects(vec![]));
    }

    #[test]
    fn test_string() {
        let mut b = base();
        assert_not_empty(&mut b, s(""));
        assert_not_empty(&mut b, fixture("EmptyAsString", "getAsString", s("")));
    }

    #[test]
    fn test_number() {
        assert_not_empty(&mut base(), JValue::Int(0));
    }

    #[test]
    fn test_string_builder() {
        assert_not_empty(&mut base(), JValue::Object(StringBuilder::new("")));
    }

    #[test]
    fn test_literals() {
        let mut b = base();
        for t in ["#if( !0 )fail#end", "#if( !0.0 )fail#end", "#if( !'' )fail#end", "#if( !\"\" )fail#end", "#if( ![] )fail#end", "#if( !{} )fail#end"] {
            b.assert_eval_equals("", t);
        }
    }
}
