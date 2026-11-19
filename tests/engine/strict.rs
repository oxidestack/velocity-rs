//! `StrictAlternateValuesTestCase`, `StrictCompareTestCase`, `StrictEscapeTestCase`, `StrictForeachTestCase`,
//! `StrictMathTestCase`, `StrictReferenceTestCase`.

use java_compat::lang::{JList, JValue, ListKind};
use java_compat::reflect::JType;

use crate::base::*;

const VELOCITY_EXCEPTION: Option<&str> = Some("org.apache.velocity.exception.VelocityException");
const MATH_EXCEPTION: Option<&str> = Some("org.apache.velocity.exception.MathException");
const METHOD_EXCEPTION: Option<&str> = Some("org.apache.velocity.exception.MethodInvocationException");

fn alternate() -> Base {
    let mut b = Base::new();
    b.set_property("runtime.strict_mode.enable", true);
    b.put("foo", JValue::Null);
    b
}

#[test]
fn test_default() {
    let mut b = alternate();
    b.assert_eval_equals("<foo>", "<${foo|'foo'}>");
    b.assert_eval_equals("bar", "#set($bar='bar')${foo|$bar}");
    b.assert_eval_equals("bar", "#set($bar='bar')${foo|${bar}}");
    b.assert_eval_exception("${foo.bar.baz()[5]|'hop'}", VELOCITY_EXCEPTION);
    b.assert_eval_equals("{foo}", "{${foo|'foo'}}");
    b.assert_eval_exception("$foo", VELOCITY_EXCEPTION);
}

#[test]
fn test_complex_eval() {
    let mut b = alternate();
    b.assert_eval_exception("<${date.format('medium', $date.date)|'no date tool'}>", VELOCITY_EXCEPTION);
    b.assert_eval_equals("true", "#set($val=false)${val.toString().replace(\"false\", \"true\")|'so what'}");
    b.assert_eval_equals("so what", "#set($foo='foo')${foo.contains('bar')|'so what'}");
    b.assert_eval_equals("so what", "#set($val=false)${val.toString().contains('bar')|'so what'}");
    b.assert_eval_equals("true", "#set($val=false)${val.toString().contains('false')|'so what'}");
    b.assert_eval_exception("$!{null|$null}", VELOCITY_EXCEPTION);
    b.assert_eval_equals("null", "$!{null|'null'}");
    b.assert_eval_equals("so what", "#set($spaces='   ')${spaces.trim()|'so what'}");
}

#[test]
fn test_compare() {
    let mut b = Base::new();
    b.set_property("runtime.strict_mode.enable", true);
    b.put("NULL", JValue::Null);
    b.put("a", s("abc"));
    b.put("b", JValue::Int(3));
    b.put("TRUE", JValue::Bool(true));
    for t in [
        "#if($a > $NULL)#end",
        "#if($a < $NULL)#end",
        "#if($a >= $NULL)#end",
        "#if($a <= $NULL)#end",
        "#if($NULL > $a)#end",
        "#if($NULL < $a)#end",
        "#if($NULL >= $a)#end",
        "#if($NULL <= $a)#end",
        "#if($NULL >= $NULL)#end",
        "#if($a >= $b)#end",
        "#if($a <= $b)#end",
        "#if($a > $b)#end",
        "#if($a < $b)#end",
        "#if($a < 5)#end",
        "#if($a > 5)#end",
    ] {
        b.assert_eval_exception(t, VELOCITY_EXCEPTION);
    }
}

fn escape() -> Base {
    let mut b = Base::new();
    b.set_property("runtime.strict_mode.escape", true);
    b.put("pow", s("bang"));
    b.put("NULL", JValue::Null);
    let animal = class("org.apache.velocity.test.StrictEscapeTestCase$Animal")
        .method("getNull", &[], Some(object()), |_, _| ret(JValue::Null))
        .method("toString", &[], Some(string()), |_, _| ret(JValue::Null))
        .build();
    b.put("animal", animal.new_instance());
    b
}

#[test]
fn test_reference_escape() {
    let mut b = escape();
    b.set_property("runtime.strict_mode.enable", true);
    b.assert_eval_exception("\\\\$bogus", None);
    b.assert_eval_exception("\\\\\\\\$bogus", None);

    b.assert_eval_equals("$bogus", "\\$bogus");
    b.assert_eval_equals("$bogus.xyz", "\\$bogus.xyz");
    b.assert_eval_equals("${bogus}", "\\${bogus}");
    b.assert_eval_equals("${bogus.xyz}", "\\${bogus.xyz}");
    b.assert_eval_equals("\\$bogus", "\\\\\\$bogus");
    b.assert_eval_equals("\\xyz", "#set($foo = \"xyz\")\\\\$foo");
    b.assert_eval_equals("\\$foo", "#set($foo = \"xyz\")\\\\\\$foo");
    b.assert_eval_equals("$foo\\", "#set($foo = \"xyz\")\\$foo\\");
    b.assert_eval_equals("$pow", "#set($foo = \"\\$pow\")$foo");
    b.assert_eval_equals("\\bang", "#set($foo = \"\\\\$pow\")$foo");
    b.assert_eval_equals("\\$pow", "#set($foo = \"\\\\\\$pow\")$foo");

    b.assert_eval_equals("\\$%", "\\$%");

    b.assert_eval_equals("$!foo", "#set($foo = $NULL)\\$!foo");
    b.assert_eval_equals("$!animal.null", "\\$!animal.null");
    b.assert_eval_equals("$!animal", "\\$!animal");
}

#[test]
fn test_macro_escape() {
    let mut b = escape();
    b.set_property("runtime.strict_mode.enable", true);
    b.assert_eval_exception("\\\\#bogus()", None);

    b.assert_eval_equals("", "#macro(foo)bar#end");

    b.assert_eval_equals("#foo()", "\\#foo()");
    b.assert_eval_equals("\\bar", "\\\\#foo()");
    b.assert_eval_equals("\\#foo()", "\\\\\\#foo()");

    b.assert_eval_equals("bar", "#set($abc = \"#foo()\")$abc");
    b.assert_eval_equals("#foo()", "#set($abc = \"\\#foo()\")$abc");
    b.assert_eval_equals("\\bar", "#set($abc = \"\\\\#foo()\")$abc");

    b.assert_eval_equals("#@foo()", "\\#@foo()");
    b.assert_eval_equals("#@foo", "\\#@foo");
    b.assert_eval_equals("#@bar", "\\#@bar");
    b.assert_eval_equals("\\bar", "\\\\#@foo()#end");
    b.assert_eval_equals("#@foo()#end", "\\#@foo()\\#end");
    b.assert_eval_equals("#@foo#end", "\\#@foo\\#end");
    b.assert_eval_equals("#@bar #end", "\\#@bar \\#end");

    b.assert_eval_equals("#end #foreach #define() #elseif", "\\#end \\#foreach \\#define() \\#elseif");
    b.assert_eval_equals("#{end} #{foreach} #{define}() #{elseif}", "\\#{end} \\#{foreach} \\#{define}() \\#{elseif}");
    b.assert_eval_equals("#macro(foo) #end", "\\#macro(foo) \\#end");

    b.assert_eval_exception("\\\\#end", None);
    b.assert_eval_exception("\\\\#if()", None);
}

#[test]
fn test_strict_mode() {
    let mut b = escape();
    b.assert_eval_equals("#bogus()", "\\#bogus()");
    b.assert_eval_equals("\\#bogus", "\\\\#bogus");

    b.assert_eval_equals("\\$bogus", "\\\\$bogus");
    b.assert_eval_equals("\\\\$bogus", "\\\\\\\\$bogus");
    b.assert_eval_equals("\\$bogus", "#set($foo = \"\\\\$bogus\")$foo");
}

fn foreach() -> Base {
    let mut b = Base::new();
    b.set_property("directive.foreach.skip_invalid", false);
    let iterator = || Some(JType::class("java.util.Iterator"));
    let good = class("org.apache.velocity.test.StrictForeachTestCase$GoodIterable")
        .method("iterator", &[], iterator(), |_, _| ret(JValue::List(JList::new(ListKind::Iterator, vec![]))))
        .build();
    let bad = class("org.apache.velocity.test.StrictForeachTestCase$BadIterable").method("iterator", &[], Some(object()), |_, _| ret(new_object())).build();
    let ugly = class("org.apache.velocity.test.StrictForeachTestCase$UglyIterable").method("iterator", &[], iterator(), |_, _| ret(JValue::Null)).build();
    b.put("good", good.new_instance());
    b.put("bad", bad.new_instance());
    b.put("ugly", ugly.new_instance());
    b
}

#[test]
fn test_good() {
    let mut b = foreach();
    if let Err(e) = b.evaluate("#foreach( $i in $good )$i#end") {
        panic!("Doing #foreach on $good should not have exploded! {}", e.message());
    }
}

#[test]
fn test_bad() {
    let mut b = foreach();
    b.assert_eval_exception("#foreach( $i in $bad )$i#end", VELOCITY_EXCEPTION);
}

#[test]
fn test_ugly() {
    let mut b = foreach();
    b.assert_eval_exception("#foreach( $i in $ugly )$i#end", VELOCITY_EXCEPTION);
}

fn math() -> Base {
    let mut b = Base::new();
    b.set_property("runtime.strict_math", true);
    b.put("num", JValue::Int(5));
    b.put("zero", JValue::Int(0));
    b
}

fn assert_null_math_ex(b: &mut Base, operation: &str) {
    b.assert_eval_exception(&format!("#set( $foo = $null {operation} $num )"), MATH_EXCEPTION);
    b.assert_eval_exception(&format!("#set( $foo = $num {operation} $null )"), MATH_EXCEPTION);
}

fn assert_imaginary_math_ex(b: &mut Base, operation: &str) {
    b.assert_eval_exception(&format!("#set( $foo = $num {operation} $zero )"), MATH_EXCEPTION);
}

#[test]
fn test_add() {
    assert_null_math_ex(&mut math(), "+");
}

#[test]
fn test_sub() {
    assert_null_math_ex(&mut math(), "-");
}

#[test]
fn test_mul() {
    assert_null_math_ex(&mut math(), "*");
}

#[test]
fn test_mod() {
    let mut b = math();
    assert_null_math_ex(&mut b, "%");
    assert_imaginary_math_ex(&mut b, "%");
}

#[test]
fn test_div() {
    let mut b = math();
    assert_null_math_ex(&mut b, "/");
    assert_imaginary_math_ex(&mut b, "/");
}

fn reference() -> Base {
    let mut b = Base::new();
    b.set_property("runtime.strict_mode.enable", true);
    b.set_property("introspector.conversion_handler.class", "none");
    b.put("NULL", JValue::Null);
    b.put("bar", JValue::Null);
    b.put("TRUE", JValue::Bool(true));
    b
}

/// `StrictReferenceTestCase.Fargo`: `prop` is `"propiness"`, `next` is null unless given.
fn fargo(next: Option<JValue>) -> JValue {
    static FARGO: std::sync::LazyLock<BeanFactory> = std::sync::LazyLock::new(|| {
        class("org.apache.velocity.test.StrictReferenceTestCase$Fargo")
            .method("getProp", &[], Some(string()), |b, _| ret(b.field("prop")))
            .method("setProp", &[string()], None, |b, a| {
                b.set_field("prop", a[0].clone());
                Ok(None)
            })
            .method("getNullVal", &[], Some(string()), |_, _| ret(JValue::Null))
            .method("getNext", &[], Some(JType::class("org.apache.velocity.test.StrictReferenceTestCase$Fargo")), |b, _| ret(b.field("next")))
            .build()
    });
    FARGO.with_fields(&[("prop", s("propiness")), ("next", next.unwrap_or(JValue::Null))])
}

fn with_fargo() -> Base {
    let mut b = reference();
    b.put("fargo", fargo(Some(fargo(None))));
    b
}

#[test]
fn test_if_statement() {
    let mut b = with_fargo();
    b.assert_eval_equals("", "#if($bogus)xxx#end");
    b.assert_eval_equals("xxx", "#if($fargo)xxx#end");
    b.assert_eval_equals("", "#if( ! $fargo)xxx#end");
    b.assert_eval_equals("xxx", "#if($bogus || $fargo)xxx#end");
    b.assert_eval_equals("", "#if($bogus && $fargo)xxx#end");
    b.assert_eval_equals("", "#if($fargo != $NULL && $bogus)xxx#end");
    b.assert_eval_equals("xxx", "#if($fargo == $NULL || ! $bogus)xxx#end");
    b.assert_eval_equals("xxx", "#if(! $bogus1 && ! $bogus2)xxx#end");
    b.assert_eval_equals("xxx", "#if($fargo.prop == \"propiness\" && ! $bogus && $bar == $NULL)xxx#end");
    b.assert_eval_equals("", "#if($bogus && $bogus.foo)xxx#end");

    b.assert_eval_exception("#if($bogus.foo)#end", METHOD_EXCEPTION);
    b.assert_eval_exception("#if(!$bogus.foo)#end", METHOD_EXCEPTION);
}

#[test]
fn test_allow_null_values() {
    let mut b = reference();
    b.evaluate("$!bar").expect("$!bar renders");
    b.assert_eval_equals("true", "#if($bar == $NULL)true#end");
    b.assert_eval_equals("true", "#set($foobar = $NULL)#if($foobar == $NULL)true#end");
    b.assert_eval_equals("13", "#set($list = [1, $NULL, 3])#foreach($item in $list)#if($item != $NULL)$item#end#end");
}

#[test]
fn test_strict_variable_ref() {
    let mut b = reference();
    for t in [
        "$bogus",
        "#macro(test)$bogus#end #test()",
        "#set($bar = $bogus)",
        "#if($bogus == \"bar\") #end",
        "#if($bogus != \"bar\") #end",
        "#if(\"bar\" == $bogus) #end",
        "#if($bogus > 1) #end",
        "#foreach($item in $bogus)#end",
    ] {
        b.assert_eval_exception(t, METHOD_EXCEPTION);
    }
    for t in ["#set($foo = \"bar\") $foo", "#macro(test1 $foo1) $foo1 #end #test1(\"junk\")", "#macro(test2) #set($foo2 = \"bar\") $foo2 #end #test2()"] {
        if let Err(e) = b.evaluate(t) {
            panic!("{t} threw {}", e.message());
        }
    }
}

#[test]
fn test_strict_method_ref() {
    let mut b = with_fargo();
    b.assert_eval_equals("propiness", "$fargo.prop");
    b.assert_eval_equals("", "$!fargo.nullVal");
    b.assert_eval_equals("propiness", "$fargo.next.prop");

    b.assert_eval_exception("$fargo.foobar", METHOD_EXCEPTION);
    b.assert_eval_exception("$fargo.next.foobar", METHOD_EXCEPTION);
    b.assert_eval_exception("$fargo.foobar()", METHOD_EXCEPTION);
    b.assert_eval_exception("#set($fargo.next.prop = $TRUE)", METHOD_EXCEPTION);
    b.assert_eval_exception("$fargo.next.setProp($TRUE)", METHOD_EXCEPTION);
}

#[test]
fn test_strict_method_on_null() {
    let mut b = with_fargo();
    for t in [
        "$NULL.bogus",
        "$fargo.nullVal.bogus",
        "$fargo.next.nullVal.bogus",
        "#if (\"junk\" == $fargo.nullVal.bogus)#end",
        "#if ($fargo.nullVal.bogus > 2)#end",
        "#set($fargo.next.nullVal.bogus = \"junk\")",
        "#set($foo = $NULL.bogus)",
        "#foreach($item in $fargo.next.nullVal.bogus)#end",
    ] {
        b.assert_eval_exception(t, VELOCITY_EXCEPTION);
    }
    b.evaluate("$fargo.prop.toString()").expect("renders");
    b.assert_eval_exception("#set($fargo.prop = $NULL)$fargo.prop.next", VELOCITY_EXCEPTION);
    for t in ["$!fargo.next.next", "$!fargo.next.nullVal", "#foreach($item in $fargo.nullVal)#end"] {
        if let Err(e) = b.evaluate(t) {
            panic!("{t} threw {}", e.message());
        }
    }
}

#[test]
fn test_macros() {
    let mut b = reference();
    for t in ["#bogus()", "#bogus (  )", "#bogus( $a )", "abc#bogus ( $a )a "] {
        b.assert_eval_exception(t, VELOCITY_EXCEPTION);
    }
    b.assert_eval_equals(" true ", "#macro(test1) true #end#test1()");
    b.assert_eval_equals(" true ", "#macro(test2 $a) $a #end#test2 ( \"true\")");
    b.assert_eval_equals("#CCFFEE", "#CCFFEE");
    b.assert_eval_equals("#F - ()", "#F - ()");
    b.assert_eval_equals("#F{}", "#F{}");
}

#[test]
fn test_rendering_null() {
    let mut b = with_fargo();
    b.assert_eval_exception("#set($foo = $NULL)$foo", VELOCITY_EXCEPTION);
    b.assert_eval_equals("", "#set($foo = $NULL)$!foo");
    b.assert_eval_exception("$fargo.nullVal", VELOCITY_EXCEPTION);
    b.assert_eval_equals("", "$!fargo.nullVal");
    b.assert_eval_exception("$fargo.next.next", VELOCITY_EXCEPTION);
    b.assert_eval_equals("", "$!fargo.next.next");
    b.assert_eval_exception("$fargo.next.nullVal", VELOCITY_EXCEPTION);
    b.assert_eval_equals("", "$!fargo.next.nullVal");
}
