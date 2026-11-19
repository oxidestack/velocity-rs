//! `ScopeTestCase`.

use java_compat::lang::{JMap, JValue, MapKind};

use crate::base::*;

fn base() -> Base {
    let mut b = Base::new();
    for scope in ["a", "define", "evaluate", "foo", "macro", "template", "vm"] {
        b.set_property(&format!("{scope}.provide.scope.control"), "true");
    }
    b.set_property("space.gobbling", "bc");
    b
}

fn assert_null(b: &Base, key: &str) {
    assert!(b.context.get(key).is_none_or(|v| v.is_null()), "{key} should be null: {:?}", b.context.get(key));
}

#[test]
fn test_scope_get_leak_into_inner() {
    let mut b = base();
    b.add_template("foo", "#foreach($i in [1..1])#set($foreach.a=$i)#foreach($j in [2..2])$foreach.a#set($foreach.a=$j)#foreach($k in [3..3])$foreach.a#end#end$foreach.a#end");
    b.assert_tmpl_equals("121", "foo");
}

#[test]
fn test_scope_get_leak_doesnt_hide_nullset() {
    let mut b = base();
    b.add_template("a", "#macro(a)#set($macro.a='a')#b()$macro.a#end#macro(b)$macro.a#set($macro.a=$null)$!macro.a#end#a()");
    b.assert_tmpl_equals("aa", "a");
}

#[test]
fn test_root_template_merge_scope() {
    let mut b = base();
    b.add_template("foo", "foo#break($template)bar");
    b.assert_tmpl_equals("foo", "foo");
    assert_null(&b, "template");
}

#[test]
fn test_parse_scope() {
    let mut b = base();
    b.add_template("test", "$template.info.depth$!parse.parent.info.depth#set( $template.foo = 'bar' )$template.foo#break($template)woogie");
    b.assert_eval_equals("1bar", "#parse( 'test' )");
    assert_null(&b, "template");
}

#[test]
fn test_nested_parse_scope() {
    let mut b = base();
    let grab = JMap::new(MapKind::Hash);
    b.put("grab", JValue::Map(grab.clone()));
    b.add_template("inner", "Inner depth: $template.info.depth#set( $template.foo = '?' )$!grab.put('inner',$template)#break($template)$template.foo");
    b.add_template("outer", "#set( $template.foo = '!' )Outer depth: $template.info.depth #parse('inner')$!grab.put('outer', $template)$template.foo");
    b.assert_eval_equals("Outer depth: 1 Inner depth: 2!", "#parse('outer')");
    let (inner, outer) = (grab.get_str("inner").unwrap(), grab.get_str("outer").unwrap());
    assert!(!matches!((&inner, &outer), (JValue::Object(x), JValue::Object(y)) if std::ptr::addr_eq(std::sync::Arc::as_ptr(x), std::sync::Arc::as_ptr(y))));
    assert_null(&b, "template");
    b.add_template("3", "$template.topmost.foo#set( $template.topmost.foo = 'bar' )");
    b.add_template("2", "#parse( '3' )$!parse.foo");
    b.add_template("1", "#set( $template.foo = 'foo' )#parse('2')$template.foo");
    b.assert_eval_equals("foobar", "#parse('1')$!parse");
    assert_null(&b, "template");
}

#[test]
fn test_foreach_scope() {
    let mut b = base();
    b.assert_eval_equals("0:1:true,1:2:true,", "#foreach( $i in [0..2] )#if( $i > 1 )#break($foreach)#end$foreach.index:$foreach.count:$foreach.hasNext,#end");
    assert_null(&b, "foreach");
}

#[test]
fn test_nested_foreach_scope() {
    let mut b = base();
    b.assert_eval_equals("123", "#foreach( $i in [1..5] )#foreach( $j in [1..2] )#if ( $i > $foreach.count + $foreach.index + $foreach.info.depth )#break($foreach.topmost)#end#end$i#end");
    assert_null(&b, "foreach");
}

#[test]
fn test_macro_scope() {
    let mut b = base();
    b.assert_eval_equals("012", "#macro( foo $i )#if($i > 2 )#break($macro)#end$i#end#foo( 0 )#foo( 1 )#foo( 2 )");
    assert_null(&b, "macro");
}

#[test]
fn test_recursive_macro_scope() {
    let mut b = base();
    b.assert_eval_equals("123", "#macro( foo )$macro.info.depth#if($macro.info.depth > 2 )#break($macro.topmost)#end#foo()#end#foo()");
    assert_null(&b, "macro");
}

#[test]
fn test_nested_macro_scope() {
    let mut b = base();
    b.assert_eval_equals("2ab", "#macro( a )$macro.info.depth#set($macro.c = 'a')$macro.c#end#macro( b )#set($macro.c = 'b' )#a()$macro.c#end#b()");
    assert_null(&b, "macro");
}

#[test]
fn test_body_macro_scope() {
    let mut b = base();
    b.assert_eval_equals("1 foobar", "#macro( foo $bar )$bodyContent$macro.bar#end#@foo( 'bar' )#set( $macro.bar = 'foo'+$bar )#set( $foo.d = $foo.info.depth )$foo.d #end");
    assert_null(&b, "foo");
    assert_null(&b, "macro");
}

#[test]
fn test_recursive_body_macro_scope() {
    let mut b = base();
    b.set_property("velocimacro.max_depth", "5");
    b.assert_eval_equals("1,2,3,4,5,", "#macro( foo )$bodyContent$macro.i#end#@foo()#set( $macro.i = \"$!macro.i$foo.info.depth,\" )$!bodyContent#end");
    assert_null(&b, "foo");
    assert_null(&b, "macro");
}

#[test]
fn test_define_scope() {
    let mut b = base();
    b.assert_eval_equals("bar1", "#define( $foo )#set( $define.bar = 'bar'+$define.info.depth )$define.bar#end$foo");
    assert_null(&b, "define");
}

#[test]
fn test_nested_define_scope() {
    let mut b = base();
    b.assert_eval_equals("2", "#define($a)$b c#end#define($b)$define.info.depth#break($define.topmost)#end$a");
    assert_null(&b, "define");
}

#[test]
fn test_recursive_define_scope() {
    let mut b = base();
    b.set_property("directive.define.max_depth", "10");
    b.assert_eval_equals("1,2,3,4,5", "#define($a)$define.info.depth#if($define.info.depth == 5)#break($define)#end,$a#end$a");
    assert_null(&b, "define");
}

#[test]
fn test_root_evaluate_scope() {
    let mut b = base();
    b.assert_eval_equals("1", "$evaluate.info.depth");
    b.assert_eval_equals("foo", "foo#break($evaluate)bar");
    assert_null(&b, "evaluate");
}

#[test]
fn test_evaluate_scope() {
    let mut b = base();
    b.put("h", s("#"));
    b.put("d", s("$"));
    let template = "${h}set( ${d}evaluate.foo = 'bar' )${d}evaluate.foo ${d}evaluate.info.depth";
    b.add_template("eval", &format!("#evaluate(\"{template}\")"));
    b.assert_tmpl_equals("bar 1", "eval");
    assert_null(&b, "evaluate");
    assert_null(&b, "template");
}

#[test]
fn test_nested_evaluate_scope() {
    let mut b = base();
    b.put("h", s("#"));
    b.put("d", s("$"));
    b.add_template("e", "#evaluate(\"${h}evaluate( '${d}evaluate.info.depth${h}stop(${d}evaluate) blah' )\")");
    b.assert_tmpl_equals("2", "e");
    assert_null(&b, "evaluate");
    assert_null(&b, "template");
}

#[test]
fn test_turning_off_template_scope() {
    let mut b = base();
    b.set_property("template.provide.scope.control", "false");
    b.add_template("test", "$template.info.depth");
    b.assert_tmpl_equals("$template.info.depth", "test");
    b.assert_eval_equals("$template.info.depth", "#parse('test')");
}

#[test]
fn test_turning_off_evaluate_scope() {
    let mut b = base();
    b.set_property("evaluate.provide.scope.control", "false");
    b.assert_schmoo("$evaluate.info.depth");
    b.assert_eval_equals("$evaluate.info.depth", "#evaluate( '$evaluate.info.depth' )");
}

#[test]
fn test_turning_off_macro_scope() {
    let mut b = base();
    b.set_property("macro.provide.scope.control", "false");
    b.set_property("foo.provide.scope.control", "false");
    b.assert_eval_equals("$macro", "#macro(a)$macro#end#a()");
    b.assert_eval_equals("$macro $foo", "#macro(foo)$bodyContent#end#@foo()$macro $foo#end");
}

#[test]
fn test_turning_off_define_scope() {
    let mut b = base();
    b.set_property("define.provide.scope.control", "false");
    b.assert_eval_equals("$define", "#define($a)$define#end$a");
}

#[test]
fn test_turning_off_foreach_scope() {
    let mut b = base();
    b.set_property("foreach.provide.scope.control", "false");
    b.assert_eval_equals("$foreach$foreach", "#foreach($i in [0..1])$foreach#end");
}

#[test]
fn test_template_replaced() {
    let mut b = base();
    b.put("template", s("foo"));
    b.add_template("test", "$template.replaced");
    b.assert_tmpl_equals("foo", "test");
    b.assert_eval_equals("foo", "#parse('test')");
    b.assert_context_value("template", &s("foo"));
}

#[test]
fn test_evaluate_replaced() {
    let mut b = base();
    b.put("evaluate", s("foo"));
    b.assert_eval_equals("foo", "$evaluate.replaced");
    b.assert_eval_equals("foo", "#evaluate('$evaluate.replaced')");
    b.assert_context_value("evaluate", &s("foo"));
}

#[test]
fn test_macro_replaced() {
    let mut b = base();
    b.put("macro", s("foo"));
    b.assert_eval_equals("foo foo foo", "$macro #macro(a)$macro.replaced#end#a() $macro");
    b.assert_context_value("macro", &s("foo"));
}

#[test]
fn test_foreach_replaced() {
    let mut b = base();
    b.put("foreach", s("foo"));
    b.assert_eval_equals("foofoofoo", "$foreach#foreach($i in [1..1])$foreach.replaced#end$foreach");
    b.assert_context_value("foreach", &s("foo"));
    b.put("foreach", s("a"));
    b.assert_eval_equals("a", "#foreach($i in [1..1])#foreach($j in [1..1])$foreach.replaced#end#end");
    b.assert_context_value("foreach", &s("a"));
}

#[test]
fn test_define_replaced() {
    let mut b = base();
    b.put("define", s("a"));
    b.assert_eval_equals("a", "#define($a)$define.replaced#end$a");
    b.assert_context_value("define", &s("a"));
}

#[test]
fn test_body_content_replaced() {
    let mut b = base();
    b.put("vm", s("a"));
    b.assert_eval_equals("a", "#macro(vm)$bodyContent#end#@vm()$vm.replaced#end");
    b.assert_context_value("vm", &s("a"));
}

#[test]
fn test_info_depth() {
    base().assert_eval_equals("2", "#foreach($i in [1..1])#foreach($j in [0..0])$foreach.info.depth#end#end");
}

#[test]
fn test_info_name() {
    base().assert_eval_equals("foreach evaluate", "#foreach($i in [1..1])$foreach.info.name #evaluate('$evaluate.info.name')#end");
}

#[test]
fn test_info_type() {
    let mut b = base();
    b.add_template("info", "#foreach($i in [1..1])$foreach.info.type#end #evaluate('$evaluate.info.type') $template.info.type");
    b.assert_tmpl_equals("block line utf-8", "info");
}

#[test]
fn test_info_line_and_column() {
    let mut b = base();
    let template = " #evaluate('$evaluate.info.line, $evaluate.info.column')";
    b.assert_eval_equals(" 1, 2", template);
    b.assert_eval_equals("\n\n   3, 4", &format!("\n\n  {template}"));
}

#[test]
fn test_info_template() {
    let mut b = base();
    b.add_template("test", "#evaluate('$evaluate.info.template')");
    b.assert_tmpl_equals("test", "test");
    b.assert_eval_equals("test", "#parse('test')");
}
