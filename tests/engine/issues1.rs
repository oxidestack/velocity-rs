//! `org.apache.velocity.test.issues`: `StackOverflow32805217TestCase` through `Velocity727TestCase`.

use java_compat::lang::JValue;
use java_compat::reflect::JType;

use crate::base::*;

const P: &str = "org.apache.velocity.test.issues.";
const TEMPLATE_INIT: Option<&str> = Some("org.apache.velocity.exception.TemplateInitException");
const PARSE_ERROR: Option<&str> = Some("org.apache.velocity.exception.ParseErrorException");
const VELOCITY_EXCEPTION: Option<&str> = Some("org.apache.velocity.exception.VelocityException");

#[test]
fn test_stack_overflow_32805217() {
    Base::new().assert_eval_equals("$map{value}", "$map{value}");
}

// ---- Velocity62TestCase ----

#[test]
fn test_nested() {
    let mut b = Base::new();
    b.put("foo", s("foo"));
    b.assert_eval_equals("foobarbar", "#macro( outer )#set( $foo = 'bar' )#inner()#end#macro( inner )$foo#end#inner()#outer()#inner()");
}

#[test]
fn test_recursive() {
    let mut b = Base::new();
    b.put("foo", s("foo"));
    b.put("i", JValue::Int(1));
    b.assert_eval_equals("12345", "#macro(recurse $i)$i#if( $i < 5 )#set( $i = $i + 1 )#recurse($i)#end#end#recurse(1)");
}

// ---- Velocity532TestCase ----

#[test]
fn test_532() {
    Base::new().assert_eval_equals("12345", "#macro( test )$foreach.count#end#foreach( $i in [1..5] )#test()#end");
}

#[test]
fn test_532b() {
    Base::new().assert_eval_equals("bar", "#macro( test $baz )#if( $foo == $null )#if( $foreach.count == 3 )bar#end#end#end#foreach( $i in [1..5] )#test($i)#end");
}

// ---- Velocity537TestCase / Velocity580TestCase ----

fn issue_template(issue: &str, name: &str) {
    let mut b = Base::new();
    b.add_property("resource.loader.file.path", resource(&format!("issues/{issue}/templates")));
    let out = b.merge(name).unwrap_or_else(|e| panic!("{name}: {}", e.message()));
    let compare = file_contents(&resource(&format!("issues/{issue}/compare/{name}.cmp")));
    assert_eq!(normalize_newlines(&out), normalize_newlines(&compare), "Output incorrect for Template: {name}");
}

#[test]
fn test_velocity537() {
    issue_template("velocity-537", "velocity537.vm");
}

#[test]
fn test_velocity537_again() {
    issue_template("velocity-537", "velocity537b.vm");
}

#[test]
fn test_velocity580() {
    issue_template("velocity-580", "velocity580.vm");
}

// ---- Velocity544TestCase ----

#[test]
fn test_boolean_property_executor() {
    let foobar = class(&format!("{P}Velocity544TestCase$Foobar"))
        .method("isTrue", &[], Some(JType::Boolean), |b, _| ret(b.field("value")))
        .method("isTrueObject", &[], Some(JType::class("java.lang.Boolean")), |b, _| ret(b.field("value")))
        .build();
    let mut b = Base::new();
    b.put("foobarTrue", foobar.with_fields(&[("value", JValue::Bool(true))]));
    b.put("foobarFalse", foobar.with_fields(&[("value", JValue::Bool(false))]));
    assert_eq!(b.evaluate("$foobarTrue.True $foobarFalse.True $foobarTrue.TrueObject $foobarFalse.TrueObject").unwrap(), "true false true false");
}

// ---- Velocity579TestCase ----

fn foobar_579() -> BeanFactory {
    let t = format!("{P}Velocity579TestCase$");
    class(&format!("{t}Foo")).interface().build();
    // `private static class Foobar extends FooImpl` (`private abstract class FooImpl implements Foo`).
    class(&format!("{t}Foobar"))
        .non_public()
        .extends(&[&format!("{t}FooImpl")])
        .implements(&[&format!("{t}Foo")])
        .method_of(&format!("{t}Foo"), "foo", &[string()], Some(string()), |_, a| ret(if a[0].is_null() { s("foo") } else { a[0].clone() }))
        .method("bar", &[], Some(string()), |_, _| ret(s("bar")))
        .build()
}

#[test]
fn test_public_method_in_private_impl_of_public_interface() {
    let mut b = Base::new();
    b.put("foo", foobar_579().new_instance());
    b.assert_eval_equals("bar", "$foo.foo('bar')");
    b.assert_eval_equals("$foo.bar()", "$foo.bar()");
}

#[test]
fn test_public_method_inherited_from_private_class() {
    // Upstream only asserts on Java 1.6.
    let t = format!("{P}Velocity579TestCase$");
    foobar_579();
    let my_bar = class(&format!("{t}MyBar")).extends(&[&format!("{t}Foobar"), &format!("{t}FooImpl")]).method("bar", &[], Some(string()), |_, _| ret(s("bar"))).build();
    let mut b = Base::new();
    b.put("bar", my_bar.new_instance());
}

// ---- Velocity587TestCase ----

#[test]
fn test_literal_two_backslashes() {
    Base::new().assert_eval_equals("\\\\", "#set( $bs2 = '\\\\' )$bs2");
}

#[test]
fn test_literal_one_backslash() {
    Base::new().assert_eval_equals("\\", "#set( $bs = '\\' )$bs");
}

#[test]
fn test_interpolated_two_backslashes() {
    Base::new().assert_eval_equals("\\\\", "#set( $bs2 = \"\\\\\" )$bs2");
}

#[test]
fn test_interpolated_one_backslash() {
    Base::new().assert_eval_equals("\\", "#set( $bs = \"\\\" )$bs");
}

// ---- Velocity589TestCase ----

#[test]
fn test_589() {
    let mut b = Base::new();
    b.put("myId", s("test"));
    b.assert_eval_equals("#test_bg", "#${myId}_bg");
}

// ---- Velocity614TestCase ----

#[test]
fn test_schmoo() {
    Base::new().assert_schmoo("#something(Stuff)");
}

#[test]
fn test_escape_schmoo_but_not_really_since_schmoo_has_no_escaping() {
    Base::new().assert_schmoo("\\#something(Stuff)");
}

#[test]
fn test_escape_macro_with_bad_arg() {
    Base::new().assert_eval_equals(" #evil(bar)", "#macro( evil $arg )$arg#end \\#evil(bar)");
}

#[test]
fn test_early_defined_macro_with_bad_arg() {
    Base::new().assert_eval_exception("#macro( evil $arg )$arg#end #evil(bar)", None);
}

#[test]
fn test_late_defined_macro_with_good_arg() {
    Base::new().assert_eval_equals("bar ", "#good('bar') #macro( good $arg )$arg#end");
}

#[test]
fn test_directives_with_bad_arg() {
    let mut b = Base::new();
    for t in [
        "#foreach(Stuff in That)foo#end",
        "#include(Stuff)",
        "#parse(Stuff)",
        "#define(Stuff)foo#end",
        "#macro( name Stuff)foo#end",
        "#foreach($i in [1..3])#break(Stuff)#end",
        "#literal(Stuff)foo#end",
    ] {
        b.assert_eval_exception(t, None);
    }
    b.assert_eval_exception("#evaluate(Stuff)", PARSE_ERROR);
}

#[test]
fn test_late_defined_macro_with_bad_arg() {
    Base::new().assert_eval_exception("#evil(bar) #macro( evil $arg )$arg#end", TEMPLATE_INIT);
}

// ---- Velocity615TestCase ----

fn base_615() -> Base {
    let mut b = Base::new();
    b.set_property("velocimacro.permissions.allow.inline", "true");
    b.set_property("velocimacro.permissions.allow.inline.to.replace.global", "false");
    b.set_property("velocimacro.permissions.allow.inline.local.scope", "true");
    b.set_property("velocimacro.arguments.strict", "true");
    b.set_property("space.gobbling", "bc");
    b
}

/// `Velocity615TestCase.Inc`: `plus()` and `toString()` both count up.
fn inc() -> JValue {
    let next = |b: &Bean| {
        let i = match b.field("i") {
            JValue::Int(i) => i,
            _ => 0,
        };
        b.set_field("i", JValue::Int(i + 1));
        i
    };
    class(&format!("{P}Velocity615TestCase$Inc"))
        .method("plus", &[], Some(JType::Int), move |b, _| ret(JValue::Int(next(b))))
        .method("toString", &[], Some(string()), move |b, _| ret(s(&next(b).to_string())))
        .build()
        .with_fields(&[("i", JValue::Int(0))])
}

#[test]
fn test_it_615() {
    base_615().assert_eval_equals("new foo new foo ", "#set( $foo = 'old' )#macro( test $foo )#set( $foo = \"new $foo \" )$foo#end#test( 'foo' )$foo");
}

fn feared(template: &str, expected: &str) {
    let mut b = base_615();
    b.put("i", inc());
    b.assert_eval_equals(expected, template);
}

#[test]
fn test_for_irrationally_feared_related_possible_problem() {
    feared("#macro( test $a )$a$a#end#test( \"$i\" )$i", "001");
}

#[test]
fn test_for_irrationally_feared_related_possible_problem2() {
    feared("#macro( test $a )#set( $a = 'a' )$a$a#end#test( \"$i\" )$i", "aa1");
}

#[test]
fn test_for_irrationally_feared_related_possible_problem3() {
    feared("#macro( test $a )$a$a#end#test( $i )$i", "012");
}

#[test]
fn test_for_irrationally_feared_related_possible_problem4() {
    feared("#macro( test $a )$a$a#end#test( $i.plus() )$i", "001");
}

#[test]
fn test_for_irrationally_feared_related_possible_problem5() {
    feared("#macro( test $a )#set( $a = $i )$a$a#end#test( 'a' )$i", "012");
}

#[test]
fn test_velocity681() {
    base_615().assert_eval_equals("$x", "#macro(myMacro $result)  #set($result = 'some value')#end#myMacro($x)$x");
}

// ---- Velocity616TestCase ----

fn base_616() -> Base {
    let mut b = Base::new();
    b.put("bar", s("bar"));
    b.put("foo", JValue::Bool(false));
    b
}

#[test]
fn test_if_no_brackets() {
    base_616().assert_eval_equals("#if (false) $bar #end", "\\#if ($foo) \\$bar \\#end");
}

#[test]
fn test_foreach_brackets() {
    base_616().assert_eval_equals("#{foreach}( $i in [1..3] )$i#{end}", "\\#{foreach}( $i in [1..3] )$i\\#{end}");
}

#[test]
fn test_if_brackets() {
    base_616().assert_eval_equals("#{if} (false) $bar #{end}", "\\#{if} ($foo) \\$bar \\#{end}");
}

#[test]
fn test_if_brackets_on_end_only() {
    base_616().assert_eval_equals("#if( false ) $bar #{end}", "\\#if( $foo ) \\$bar \\#{end}");
}

// ---- Velocity625TestCase ----

#[test]
fn test_625() {
    Base::new().assert_eval_equals("test", "#macro(test $a $b)test#end#test('x')");
}

// ---- Velocity627TestCase ----

#[test]
fn test_627() {
    let mut b = Base::new();
    b.set_property("directive.foreach.skip_invalid", false);
    b.assert_eval_exception_at("##\n##\n#foreach($i in \"junk\")blaa#end", "", 3, 16);
}

// ---- Velocity629TestCase ----

#[test]
fn test_629() {
    let mut b = Base::new();
    b.assert_eval_exception_at("##\n##\n#set($list=[1])#set($x=\"\n$list.get(1)\n\")", "", 4, 7);
    b.assert_eval_exception_at("##\n##\n#set($x=\"#if\")", "", 3, 9);
    b.assert_eval_exception_at("##\n##\n#macro(test $i)$i#end#set($list=[1])#test(\"$list.get(1)\")", "", 3, 50);
}

// ---- Velocity631TestCase ----

#[test]
fn test_631() {
    let mut b = Base::new();
    b.set_property("space.gobbling", "bc");
    b.assert_eval_equals("$a", "$a #set($b = 1)");
    b.assert_eval_equals("$a", "$a#set($b = 1)");
    b.assert_eval_equals("$a.b", "$a.b#set($b = 1)");
    b.assert_eval_equals("$a.b(", "$a.b(#set($b = 1)");
}

// ---- Velocity644TestCase ----

#[test]
fn test_644() {
    let mut b = Base::new();
    b.set_property("resource.loader.file.path", resource("templates"));
    b.set_property("velocimacro.library.path", "testCase644.vm");
    b.set_property("runtime.strict_mode.enable", true);
    b.put("NULL", JValue::Null);
    b.assert_eval_exception_at("#nullMethod()", "testCase644.vm", 9, 8);
    b.assert_eval_exception_at("#arrayError()", "testCase644.vm", 4, 8);
    b.assert_eval_exception_at("#badRef()", "testCase644.vm", 13, 3);
    b.assert_eval_exception_at("#forloop()", "testCase644.vm", 18, 18);
}

// ---- Velocity667TestCase ----

#[test]
fn test_667() {
    let mut b = Base::new();
    b.assert_eval_exception_at("#macro", "", 1, 7);
    b.assert_eval_exception_at("#macro #macro", "", 1, 7);
}

// ---- Velocity682TestCase ----

#[test]
fn test_682() {
    let mut b = Base::new();
    b.set_property("velocimacro.inline.local_scope", true);
    b.assert_eval_equals("foo1foo2", "#macro(eval $e)#evaluate($e)#end#eval('foo1')#eval('foo2')");
}

#[test]
fn test_682b() {
    Base::new().assert_eval_equals("foobar", "#macro( eval $e )#evaluate($e)#end#eval('foo')#eval('bar')");
}

#[test]
fn test_682c() {
    Base::new().assert_eval_equals("foo\nbar", "#macro( eval $e )#evaluate($e)#end\n#eval('foo')\n\n#eval('bar')");
}

// ---- Velocity689TestCase ----

#[test]
fn test_it_689() {
    let t = format!("{P}Velocity689TestCase$");
    class(&format!("{t}HasMethod")).interface().build();
    class(&format!("{t}HasOtherMethod")).interface().implements(&[&format!("{t}HasMethod")]).build();
    class(&format!("{t}NoMethod")).interface().implements(&[&format!("{t}HasOtherMethod"), &format!("{t}HasMethod")]).build();
    // `private static class Foo implements NoMethod`: its getters are the interfaces' methods.
    let foo = class(&format!("{t}Foo"))
        .non_public()
        .implements(&[&format!("{t}NoMethod"), &format!("{t}HasOtherMethod"), &format!("{t}HasMethod")])
        .method_of(&format!("{t}HasMethod"), "getBar", &[], Some(string()), |_, _| ret(s("bar")))
        .method_of(&format!("{t}HasOtherMethod"), "getBaz", &[], Some(string()), |_, _| ret(s("baz")))
        .build();
    let mut b = Base::new();
    b.put("foo", foo.new_instance());
    b.assert_eval_equals("baz, bar", "$foo.baz, $foo.bar");
}

// ---- Velocity701TestCase ----

#[test]
fn test_abstract_class() {
    let t = format!("{P}Velocity701TestCase$");
    // An anonymous subclass of the public abstract `Foo`.
    let foo = class(&format!("{P}Velocity701TestCase$1")).non_public().extends(&[&format!("{t}Foo")]).method_of(&format!("{t}Foo"), "getBar", &[], Some(string()), |_, _| ret(s("bar"))).build();
    let mut b = Base::new();
    b.put("foo", foo.new_instance());
    b.assert_eval_equals("bar", "$foo.bar");
}

#[test]
fn test_enum() {
    let t = format!("{P}Velocity701TestCase$");
    // `Bar.ONE`, a constant with its own class body.
    let one = class(&format!("{t}Bar$1")).non_public().extends(&[&format!("{t}Bar"), "java.lang.Enum"]).method_of(&format!("{t}Bar"), "getFoo", &[], Some(string()), |_, _| ret(s("foo"))).build();
    let mut b = Base::new();
    b.put("bar", one.new_instance());
    b.assert_eval_equals("foo", "$bar.foo");
}

// ---- Velocity702TestCase ----

#[test]
fn test_it_702() {
    let mut b = Base::new();
    b.set_property("resource.loaders", "high,low");
    b.add_property("high.resource.loader.class", velocity::resource::STRING_LOADER);
    b.add_property("high.resource.loader.cache", "false");
    b.add_property("high.resource.loader.repository.name", "high");
    b.add_property("high.resource.loader.repository.static", "false");
    b.add_property("high.resource.loader.modificationCheckInterval", "1");
    b.add_property("low.resource.loader.class", velocity::resource::STRING_LOADER);
    b.add_property("low.resource.loader.cache", "true");
    b.add_property("low.resource.loader.repository.name", "low");
    b.add_property("low.resource.loader.repository.static", "false");
    b.add_property("low.resource.loader.modificationCheckInterval", "1");
    let high = b.engine().string_repository("high").unwrap();
    let low = b.engine().string_repository("low").unwrap();
    high.put("foo", "foo");
    low.put("foo", "bar");
    b.assert_tmpl_equals("foo", "foo");
    high.remove("foo");
    b.assert_tmpl_equals("bar", "foo");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    high.put("foo", "woogie");
    b.assert_tmpl_equals("woogie", "foo");
}

// ---- Velocity709TestCase ----

#[test]
fn test_escaped_backslash_in_set_directive() {
    Base::new().assert_eval_equals("\\", "#set($var = \"\\\" )#set($var2 = \"${var}\")$var2");
}

#[test]
fn test_escaped_double_quote() {
    Base::new().assert_eval_equals("jeah \"baby\" jeah! \"\"", "#set($foo = \"jeah \"\"baby\"\" jeah! \"\"\"\"\")$foo");
}

#[test]
fn test_escaped_single_quote() {
    Base::new().assert_eval_equals("jeah 'baby' jeah!", "#set($foo = 'jeah ''baby'' jeah!')$foo");
}

// ---- Velocity727TestCase ----

#[test]
fn test_define_with_no_argument() {
    Base::new().assert_eval_exception("#define() foo bar #end", VELOCITY_EXCEPTION);
}
