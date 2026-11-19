//! `BreakDirectiveTestCase`.

use crate::base::*;

fn base() -> Base {
    let mut b = Base::new();
    for scope in ["a", "define", "evaluate", "macro", "template"] {
        b.set_property(&format!("{scope}.provide.scope.control"), "true");
    }
    b
}

#[test]
fn test_bad_args() {
    let mut b = base();
    b.put("foo", s("foo"));
    for t in ["#break($null)", "#break($foo)", "#break(true)", "#break(1.2)", "#break([0..1])", "#break( $too $many )"] {
        b.assert_eval_exception(t, None);
    }
}

#[test]
fn test_stop_foreach() {
    let mut b = base();
    let template = "#foreach($i in [1..5])$i#if($i>2)#break($foreach)#end#end test";
    b.assert_eval_equals("123 test", template);
    b.assert_eval_equals("123 test123 test", &format!("#foreach($j in [1..2]){template}#end"));
    b.assert_eval_equals("1213 test2223", "#foreach($i in [1..2])#foreach($j in [2..3])$i$j#if($i+$j==5)#break($foreach.parent)#end#end test#end");
    b.assert_eval_equals("1, 2, 3, 4, 5", "#foreach($i in [1..10])$i#if($i > 4)#break#end, #end");
    b.assert_eval_equals("1", "#foreach($i in [1..5])$i#break #end");
    b.assert_eval_equals("~~~, ~~, ~, ", "#foreach($i in [1..3])#foreach($j in [2..4])#if($i*$j >= 8)#break#end~#end, #end");
}

#[test]
fn test_stop_template() {
    let mut b = base();
    b.add_template("a", "a#break($template)b");
    b.assert_tmpl_equals("a", "a");
    b.assert_eval_equals("ac", "#parse('a')c");
    b.add_template("b", "b#{break}a");
    b.assert_tmpl_equals("b", "b");
}

#[test]
fn test_stop_evaluate() {
    let mut b = base();
    b.assert_eval_equals("a", "a#break($evaluate)b");
    b.assert_eval_equals("a", "#evaluate('a#break($evaluate)b')");
    b.assert_eval_equals("a", "a#evaluate('#break($evaluate.topmost)')b");
    b.assert_eval_equals("a", "a#{break}b");
}

#[test]
fn test_stop_define_block() {
    let mut b = base();
    b.assert_eval_equals("a", "#define($a)a#break($define)b#end$a");
    b.assert_eval_equals("aa", "#define($a)a#break($define.parent)b#end#define($b)a${a}b#end$b");
    b.assert_eval_equals("a", "#define($a)a#{break}b#end$a");
}

#[test]
fn test_stop_macro() {
    let mut b = base();
    b.assert_eval_equals("a ", "#macro(a)a #break($macro) b#end#a");
    b.assert_eval_equals("b c ", "#macro(c)c #break($macro.parent) d#end#macro(b)b #c c#end#b");
    b.assert_eval_equals("d", "#macro(d)d#{break}e#end#d");
}

#[test]
fn test_stop_macro_body_block() {
    let mut b = base();
    b.assert_eval_equals(" a ", "#macro(a) $bodyContent #end#@a()a#break($a)b#end");
    b.assert_eval_equals(" b ", "#macro(b) $bodyContent #end#@b()b#{break}c#end");
}
