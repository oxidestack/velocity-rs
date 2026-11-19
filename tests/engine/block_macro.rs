//! `BlockMacroTestCase`.

use crate::base::*;

#[test]
fn test_multiple_body_content_includes() {
    Base::new().assert_eval_equals("  Yeah, woohoo! jee jee", "#macro(foo $txt) Yeah, $txt! $bodyContent $bodyContent#end #@foo(\"woohoo\")jee#end");
}

#[test]
fn test_nested_velocity_logic() {
    Base::new().assert_eval_equals("  Yeah, woohoo! 1:2:3:", "#macro(foo $txt) Yeah, $txt! $bodyContent#end #@foo(\"woohoo\")#foreach($i in [1..3])$i:#{end}#end");
}

#[test]
fn test_empty_body() {
    Base::new().assert_eval_equals("  Yeah, woohoo! ", "#macro(foo $txt) Yeah, $txt! $bodyContent#end #@foo(\"woohoo\")#end");
}

#[test]
fn test_no_arguments_empty_body_call() {
    Base::new().assert_eval_equals("  Yeah! ", "#macro(foo) Yeah! $bodyContent#end #@foo()#end");
}

#[test]
fn test_custom_body_reference() {
    let mut b = Base::new();
    b.set_property("velocimacro.body_reference", "myBody");
    b.assert_eval_equals("  Yeah! ", "#macro(foo) Yeah! $myBody#end #@foo()#end");
}

#[test]
fn test_velocity671() {
    let mut b = Base::new();
    b.set_property("velocimacro.inline.local_scope", true);
    b.assert_eval_equals(" Yeah!", "#macro(echo)$bodyContent#end #@echo()Yeah!#end");
}

#[test]
fn test_strict() {
    let mut b = Base::new();
    b.set_property("runtime.strict_mode.enable", true);
    b.assert_eval_exception("#@foo#end", None);
    b.assert_eval_exception("#@foo()#end", None);
}

#[test]
fn test_velocity690() {
    let mut b = Base::new();
    b.assert_eval_equals(" output ", "#macro(foo) output #end#@foo #end");
    b.assert_eval_equals("#[ output )", "#macro(foo2)#[$bodyContent)#end#@foo2 output #end");
    b.assert_eval_equals("#[output)", "#macro(foo2)#[$bodyContent)#end#{@foo2}output#end");
    b.assert_eval_exception("#macro(foo) output #end#@foo", None);
}

#[test]
fn test_velocity675() {
    Base::new().assert_eval_equals("#@foo#end", "#@foo#end");
}

#[test]
fn test_velocity685() {
    let mut b = Base::new();
    b.set_property("velocimacro.arguments.strict", true);
    b.assert_eval_equals(" ", "#macro(foo)#end #@foo() junk #end");
}

#[test]
fn test_velocity686() {
    Base::new().assert_eval_equals(" a b ", "#macro(foo)#set( $x = $bodyContent )#end#@foo()b#end a $x ");
}

#[test]
fn test_nested_block_macro() {
    Base::new().assert_eval_equals("foo:foo,bar:bar", "#macro(foo)foo:$bodyContent#end#macro(bar)bar:$bodyContent#end#@foo()foo,#@bar()bar#end#end");
}

#[test]
fn test_recursive_block_macro() {
    let mut b = Base::new();
    b.set_property("velocimacro.max_depth", 3);
    b.assert_eval_equals("start:call:call:call:$bodyContent", "#macro(foo)start:$bodyContent#end#@foo()call:$bodyContent#end");
}

#[test]
fn test_blue_joes_problem() {
    let mut b = Base::new();
    b.set_property("macro.provide.scope.control", true);
    b.add_template("a", "#macro(wrap $layout)$!macro.put($layout,$bodyContent)#parse($layout)#end#@wrap('b')a#end");
    b.add_template("b", "#@wrap('c')b$!macro.get('b')b#end");
    b.add_template("c", "c$!macro.get('c')c");
    b.assert_tmpl_equals("cbabc", "a");
}
