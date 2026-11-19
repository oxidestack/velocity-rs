//! `IfNullTestCase`.

use java_compat::lang::JValue;

use crate::base::*;

fn base() -> Base {
    let mut b = Base::new();
    let null_to_string = class("org.apache.velocity.test.IfNullTestCase$NullToString").method("getAsString", &[], Some(string()), |_, _| ret(JValue::Null)).build();
    b.put("nullToString", null_to_string.new_instance());
    b.put("notnull", new_object());
    b
}

#[test]
fn test_if_equals() {
    let mut b = base();
    b.assert_eval_equals("foo", "#if( $null == $otherNull )foo#{else}bar#end");
    b.assert_eval_equals("foo", "#if( $null == $nullToString )foo#{else}bar#end");
    b.assert_eval_equals("foo", "#if( $nullToString == $null )foo#{else}bar#end");
    b.assert_eval_equals("bar", "#if( $nullToString == $notnull )foo#{else}bar#end");
    b.assert_eval_equals("bar", "#if( $null == $notnull )foo#{else}bar#end");
    b.assert_eval_equals("bar", "#if( $notnull == $nullToString )foo#{else}bar#end");
    b.assert_eval_equals("bar", "#if( $notnull == $null )foo#{else}bar#end");
}

#[test]
fn test_if_not_equals() {
    let mut b = base();
    b.assert_eval_equals("bar", "#if( $null != $otherNull )foo#{else}bar#end");
    b.assert_eval_equals("bar", "#if( $null != $nullToString )foo#{else}bar#end");
    b.assert_eval_equals("bar", "#if( $nullToString != $null )foo#{else}bar#end");
    b.assert_eval_equals("foo", "#if( $nullToString != $notnull )foo#{else}bar#end");
    b.assert_eval_equals("foo", "#if( $null != $notnull )foo#{else}bar#end");
    b.assert_eval_equals("foo", "#if( $notnull != $nullToString )foo#{else}bar#end");
    b.assert_eval_equals("foo", "#if( $notnull != $null )foo#{else}bar#end");
}

#[test]
fn test_if_value() {
    let mut b = base();
    b.assert_eval_equals("bar", "#if( $null )foo#{else}bar#end");
    b.assert_eval_equals("bar", "#if( $nullToString )foo#{else}bar#end");
    b.assert_eval_equals("foo", "#if( !$null )foo#{else}bar#end");
    b.assert_eval_equals("foo", "#if( !$nullToString )foo#{else}bar#end");
}

#[test]
fn test_if_and() {
    let mut b = base();
    b.assert_eval_equals("bar", "#if( $null && $nullToString )foo#{else}bar#end");
    b.assert_eval_equals("bar", "#if( $nullToString && $null )foo#{else}bar#end");
    b.assert_eval_equals("bar", "#if( $null && $notnull )foo#{else}bar#end");
    b.assert_eval_equals("bar", "#if( $notnull && $nullToString )foo#{else}bar#end");
}

#[test]
fn test_if_or() {
    let mut b = base();
    b.assert_eval_equals("bar", "#if( $null || $nullToString )foo#{else}bar#end");
    b.assert_eval_equals("bar", "#if( $nullToString || $null )foo#{else}bar#end");
    b.assert_eval_equals("foo", "#if( $null || $notnull )foo#{else}bar#end");
    b.assert_eval_equals("foo", "#if( $notnull || $nullToString )foo#{else}bar#end");
}
