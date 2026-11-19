//! `StringConcatenationTestCase`.

use crate::base::*;

fn base() -> Base {
    let mut b = Base::new();
    b.put("foo", s("foo"));
    b.put("baz", s("baz"));
    b
}

#[test]
fn test_string_ref_left() {
    let mut b = base();
    b.assert_eval_equals("foobar", "#set( $o = $foo + 'bar' )$o");
    b.assert_eval_equals("foo$bar", "#set( $o = $foo + $bar )$o");
    b.assert_eval_equals("foo1", "#set( $o = $foo + 1 )$o");
    b.assert_eval_equals("foobaz", "#set( $o = $foo + $baz )$o");
}

#[test]
fn test_string_ref_right() {
    let mut b = base();
    b.assert_eval_equals("barfoo", "#set( $o = 'bar' + $foo )$o");
    b.assert_eval_equals("$barfoo", "#set( $o = $bar + $foo )$o");
    b.assert_eval_equals("1foo", "#set( $o = 1 + $foo )$o");
}

#[test]
fn test_no_ref() {
    base().assert_eval_equals("bar1", "#set( $o = 'bar' + 1 )$o");
}

#[test]
fn test_all() {
    base().assert_eval_equals("foobar$bar1baz", "#set( $o = $foo + 'bar' + $bar + 1 + $baz )$o");
}
