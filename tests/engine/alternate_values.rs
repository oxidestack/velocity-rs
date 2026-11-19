//! `AlternateValuesTestCase`.

use crate::base::*;

#[test]
fn test_default() {
    let mut b = Base::new();
    b.assert_eval_equals("<foo>", "<${foo|'foo'}>");
    b.assert_eval_equals("bar", "#set($bar='bar')${foo|$bar}");
    b.assert_eval_equals("bar", "#set($bar='bar')${bar|'foo'}");
    b.assert_eval_equals("bar", "#set($bar='bar')${foo|${bar}}");
    b.assert_eval_equals("baz", "${foo|${baz|'baz'}}");
    b.assert_eval_equals("hop", "${foo.bar.baz()[5]|'hop'}");
    b.assert_eval_equals("{foo}", "{${foo|'foo'}}");
    b.assert_eval_equals("<1>", "<${foo|1}>");
    b.assert_eval_equals("<1.1>", "<${foo|1.1}>");
}

#[test]
fn test_complex_eval() {
    let mut b = Base::new();
    b.assert_eval_equals("<no date tool>", "<${date.format('medium', $date.date)|'no date tool'}>");
    b.assert_eval_equals("true", "#set($val=false)${val.toString().replace(\"false\", \"true\")|'so what'}");
    b.assert_eval_equals("so what", "#set($foo='foo')${foo.contains('bar')|'so what'}");
    b.assert_eval_equals("so what", "#set($val=false)${val.toString().contains('bar')|'so what'}");
    b.assert_eval_equals("true", "#set($val=false)${val.toString().contains('false')|'so what'}");
    b.assert_eval_equals("", "$!{null|$null}");
    b.assert_eval_equals("null", "$!{null|'null'}");
    b.assert_eval_equals("so what", "#set($spaces='   ')${spaces.trim()|'so what'}");
}
