//! `org.apache.velocity.test.issues`: `Velocity728TestCase` through `Velocity961TestCase`.

use std::sync::Arc;

use java_compat::lang::JValue;
use java_compat::reflect::{EnumConstant, JType};
use velocity::events::{EventCartridge, Handler, IncludeEventHandler};
use velocity::props::{ExtProperties, PropValue};
use velocity::{Context, Engine, VelocityError};

use crate::base::*;
use crate::test_logger::TestLogger;

const P: &str = "org.apache.velocity.test.issues.";
const VELOCITY_EXCEPTION: Option<&str> = Some("org.apache.velocity.exception.VelocityException");

#[test]
fn test_parse_with_no_argument() {
    Base::new().assert_eval_exception("#parse()", VELOCITY_EXCEPTION);
}

// ---- Velocity729TestCase ----

#[test]
fn test_dot_right_after_dollar_reference() {
    let mut b = Base::new();
    b.put("jee", s("foo"));
    b.assert_eval_equals("$.x schmoo foo", "$.x schmoo $jee");
}

#[test]
fn test_velocity754_jquery_post() {
    Base::new().assert_schmoo("$.post(\"someUrl\", \"\")");
}

// ---- Velocity736TestCase ----

#[test]
fn test_public_method_inherited_from_abstract_protected_class() {
    let t = format!("{P}Velocity736TestCase$");
    // `FileSizeLimitExceededException extends SizeException` (protected abstract), whose getter is public.
    let e = class(&format!("{t}FileSizeLimitExceededException"))
        .extends(&[&format!("{t}SizeException"), &format!("{t}FileUploadException"), "java.lang.Exception", "java.lang.Throwable"])
        .method_of(&format!("{t}SizeException"), "getPermittedSize", &[], Some(JType::Long), |b, _| ret(b.field("permitted")))
        .build();
    let mut b = Base::new();
    b.put("e", e.with_fields(&[("permitted", JValue::Long(100))]));
    b.assert_eval_equals("100", "$e.permittedSize");
}

// ---- Velocity742TestCase ----

#[test]
fn test_disable_and_restore_directive() {
    let s = "#include('doesnotexist.vm') directive is disabled";
    let mut b = Base::new();
    b.engine_mut().remove_directive("include");
    b.assert_eval_equals(s, s);
    b.engine_mut().load_directive("org.apache.velocity.runtime.directive.Include").unwrap();
    b.assert_eval_exception(s, Some("org.apache.velocity.exception.ResourceNotFoundException"));
}

// ---- Velocity747TestCase ----

fn engine1() -> Engine {
    // `vel.props` has spaces after its values.
    let mut props = ExtProperties::from_file(std::path::Path::new(&resource("issues/velocity-747/vel.props"))).unwrap();
    props.set_property("file.resource.loader.path", format!("{}/", resource("issues/velocity-747")).as_str().into());
    let instance: Arc<dyn velocity::log::Logger> = Arc::new(TestLogger::new(false, false));
    props.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    Engine::with_properties(&props).unwrap()
}

fn engine2() -> Engine {
    let mut p = ExtProperties::new();
    p.set_property("resource.loaders", "file,string".into());
    p.add_property("file.resource.loader.path", format!("{}/", resource("issues/velocity-747")).as_str().into());
    p.add_property("file.resource.loader.cache", "true".into());
    p.add_property("file.resource.loader.modificationCheckInterval", "-1".into());
    p.add_property("velocimacro.permissions.allow.inline.local.scope", "true".into());
    p.add_property("velocimacro.max.depth", "-1".into());
    p.add_property("string.resource.loader.class", velocity::resource::STRING_LOADER.into());
    p.add_property("string.resource.loader.repository.name", "stringRepo".into());
    p.add_property("string.resource.loader.repository.static", "false".into());
    let instance: Arc<dyn velocity::log::Logger> = Arc::new(TestLogger::new(false, false));
    p.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    Engine::with_properties(&p).unwrap()
}

fn macro_isolation(engine: Engine) {
    assert_eq!(engine.merge_template("one.vm", Some("UTF-8"), &mut Context::new()).unwrap(), "This is from Test1 macro of one.vm");
    assert_eq!(engine.merge_template("two.vm", Some("UTF-8"), &mut Context::new()).unwrap(), "This is from Test1 macro of two.vm");
}

#[test]
fn test_macro_isolation1() {
    macro_isolation(engine1());
}

#[test]
fn test_macro_isolation2() {
    macro_isolation(engine2());
}

// ---- Velocity753TestCase ----

#[test]
fn test_float_arg() {
    let tool = class(&format!("{P}Velocity753TestCase$Tool"))
        .method("test", &[JType::Double], Some(string()), |_, _| ret(s("double")))
        .method("test", &[object()], Some(string()), |_, _| ret(s("object")))
        .build();
    let mut b = Base::new();
    b.put("tool", tool.new_instance());
    b.put("float", JValue::Float(5.23));
    // In reflection, `Float` and `float` are equivalent, so `double` is chosen.
    b.assert_eval_equals("double", "$tool.test($float)");
}

// ---- Velocity755TestCase ----

#[test]
fn test_map_order() {
    Base::new().assert_eval_equals("1true3false5", "#set( $map = {'a': 1, 'b': true, 'c': 3, 'd': false, 'e': 5} )#foreach( $i in $map )$i#end");
}

// ---- Velocity758TestCase ----

#[test]
fn test_null_argument_for_parse() {
    Base::new().assert_eval_equals("", "#parse($foo)");
}

/// `Velocity758TestCase.Handler`: a null path becomes `null.vm`.
struct NullParse;

impl IncludeEventHandler for NullParse {
    fn include_event(&self, _: &Engine, _: &mut Context, path: Option<&str>, _: Option<&str>, _: &str) -> Result<Option<String>, VelocityError> {
        Ok(Some(path.unwrap_or("null.vm").to_string()))
    }
}

#[test]
fn test_override_null_argument_for_parse() {
    let mut b = Base::new();
    b.add_template("null.vm", "Parse arg was null");
    let ec = EventCartridge::new();
    ec.add(Handler { include: Some(Arc::new(NullParse)), ..Default::default() });
    b.context.attach_event_cartridge(ec);
    b.assert_eval_equals("Parse arg was null", "#parse($foo)");
}

// ---- Velocity762TestCase ----

#[test]
fn test_foreach_is_last() {
    Base::new().assert_eval_equals("false false true ", "#foreach( $i in [1..3] )$foreach.last #end");
}

// ---- Velocity785TestCase ----

#[test]
fn test_quote_escapes() {
    let mut b = Base::new();
    b.assert_eval_equals("\"", "#set($double_double = \"\"\"\")$double_double");
    b.assert_eval_equals("'", "#set($single_single = '''')$single_single");
    b.assert_eval_equals("''", "#set($double_single = \"''\")$double_single");
    b.assert_eval_equals("\"\"", "#set($single_double = '\"\"')$single_double");
}

// ---- Velocity830TestCase ----

#[test]
fn test_underscore_method() {
    let obj = class(&format!("{P}Velocity830TestCase$UnderscoreMethodObject"))
        .method("check", &[], Some(string()), |_, _| ret(s("ok")))
        .method("_1", &[], Some(string()), |_, _| ret(s("gotit")))
        .build();
    let mut b = Base::new();
    b.put("obj", obj.new_instance());
    b.assert_eval_equals("ok", "$obj.check()");
    b.assert_eval_equals("gotit", "$obj._1()");
}

// ---- Velocity855TestCase ----

/// An enum class object with its static `valueOf(String)`.
fn enum_class(name: &str, constants: &[&str]) -> JValue {
    let def = class(name).enum_of(constants).build().class;
    let for_value_of = def.clone();
    class(name)
        .enum_of(constants)
        .static_method("valueOf", &[string()], Some(JType::class(name)), move |a| EnumConstant::value_of(&for_value_of, &a[0].to_java_string()).map(Some))
        .build()
        .class_object()
}

#[test]
fn test_velocity855() {
    // Stand-ins for `javax.lang.model.element.ElementKind` and `javax.lang.model.type.TypeKind`.
    let mut b = Base::new();
    b.put("elementKind", enum_class("javax.lang.model.element.ElementKind", &["PACKAGE", "ENUM", "CLASS"]));
    b.put("typeKind", enum_class("javax.lang.model.type.TypeKind", &["BOOLEAN", "DECLARED", "ERROR"]));
    b.assert_eval_equals("ENUM DECLARED", "$elementKind.valueOf('ENUM') $typeKind.valueOf('DECLARED')");
}

// ---- Velocity889TestCase ----

#[test]
fn test_space_before_rparen() {
    let mut b = Base::new();
    b.assert_schmoo("#foo(\n)");
    b.assert_schmoo("#foo(\n )");
}

#[test]
fn test_space_before_rparen_with_arg() {
    let mut b = Base::new();
    b.assert_schmoo("#foo(\n$bar\n)");
    b.assert_schmoo("#foo(\n $bar\n )");
}

#[test]
fn test_space_before_rparen_with_default_arg() {
    let mut b = Base::new();
    b.assert_eval_equals("", "#macro(\nfoo\n,\n$bar\n=\n'bar')\n#end");
    b.assert_eval_equals("", "#macro(\n foo\n ,\n $bar\n =\n 'bar'\n )\n #end");
}

// ---- Velocity896TestCase ----

#[test]
fn test_tailing_hash() {
    let mut b = Base::new();
    b.assert_eval_equals("#", "#");
    b.assert_eval_equals("$", "$");
}

// ---- Velocity904TestCase ----

fn base_904(preserve: bool) -> Base {
    let mut b = Base::new();
    // Also covers the deprecated `velocimacro.arguments.preserve_literals`.
    b.set_property("velocimacro.arguments.preserve_literals", if preserve { "true" } else { "false" });
    b
}

#[test]
fn test_null_arg_no_preserve() {
    base_904(false).assert_eval_equals("$parameter", "#macro(testmacro $parameter)$parameter#end#testmacro($return)");
}

#[test]
fn test_null_arg_preserve() {
    base_904(true).assert_eval_equals("$return", "#macro(testmacro $parameter)$parameter#end#testmacro($return)");
}

#[test]
fn test_arg_set_to_null_no_preserve() {
    base_904(false).assert_eval_equals("$input", "#macro(mymacro $input)#set($input = $null)$input#end#set($variable = 'value')#mymacro($variable)");
}

#[test]
fn test_arg_set_to_null_preserve() {
    base_904(true).assert_eval_equals("$variable", "#macro(mymacro $input)#set($input = $null)$input#end#set($variable = 'value')#mymacro($variable)");
}

#[test]
fn test_sub_macro_no_preserve() {
    base_904(false).assert_eval_equals("$return$return$return", "#macro(macro1 $return)$return#macro2($param2)$return#end#macro(macro2 $return)$return#end#macro1($param)");
}

#[test]
fn test_sub_macro_preserve() {
    base_904(true).assert_eval_equals("$param$param2$param", "#macro(macro1 $return)$return#macro2($param2)$return#end#macro(macro2 $return)$return#end#macro1($param)");
}

#[test]
fn test_no_arg_no_preserve() {
    base_904(false).assert_eval_equals("", "#macro(testMacro $param)#end#testMacro()");
}

#[test]
fn test_no_arg_preserve() {
    base_904(true).assert_eval_equals("", "#macro(testMacro $param)#end#testMacro()");
}

#[test]
fn test_constant_set_to_null_no_preserve() {
    let mut b = base_904(false);
    b.assert_eval_equals("$input", "#macro(mymacro $input)#set($input = $null)$input#end#mymacro('string-value')");
    b.assert_eval_equals("$input", "#macro(mymacro $input)#set($input = $null)$input#end#mymacro(\"interpolated-$bar-value\")");
    b.assert_eval_equals("$input", "#macro(mymacro $input)#set($input = $null)$input#end#mymacro(true)");
    b.assert_eval_equals("$input", "#macro(mymacro $input)#set($input = $null)$input#end#mymacro(4.5)");
}

#[test]
fn test_constant_set_to_null_preserve() {
    let mut b = base_904(true);
    b.assert_eval_equals("'string-value'", "#macro(mymacro $input)#set($input = $null)$input#end#mymacro('string-value')");
    b.assert_eval_equals("\"interpolated-$bar-value\"", "#macro(mymacro $input)#set($input = $null)$input#end#mymacro(\"interpolated-$bar-value\")");
    b.assert_eval_equals("$input", "#macro(mymacro $input)#set($input = $null)$input#end#mymacro(true)");
    b.assert_eval_equals("$input", "#macro(mymacro $input)#set($input = $null)$input#end#mymacro(4.5)");
}

fn constants(b: &mut Base) {
    b.assert_eval_equals("true", "#macro(mymacro $input)$input#end#mymacro(true)");
    b.assert_eval_equals("1.5", "#macro(mymacro $input)$input#end#mymacro(1.5)");
    b.assert_eval_equals("foo", "#macro(mymacro $input)$input#end#mymacro('foo')");
    b.assert_eval_equals("{}", "#macro(mymacro $input)$input#end#mymacro({})");
    b.assert_eval_equals("[]", "#macro(mymacro $input)$input#end#mymacro([])");
}

#[test]
fn test_constant_no_preserve() {
    constants(&mut base_904(false));
}

#[test]
fn test_constant_preserve() {
    constants(&mut base_904(true));
}

// ---- Velocity919TestCase ----

#[test]
fn test_unbreakable_space() {
    Base::new().assert_eval_equals("before\u{200B}after", "before\u{200B}after");
}

#[test]
fn test_user_file_separator() {
    Base::new().assert_eval_equals("before\u{001C}after", "before\u{001C}after");
}

// ---- Velocity924TestCase ----

fn base_924() -> Base {
    let foo_class = class(&format!("{P}Velocity924TestCase$Foo")).method("getName", &[], Some(string()), |_, _| ret(s("foo"))).build();
    let mut b = Base::new();
    b.put("var", foo_class.new_instance());
    b
}

const FOO_924: &str = "org.apache.velocity.test.issues.Velocity924TestCase$Foo";

#[test]
fn test_velocity924_getter() {
    base_924().assert_eval_equals(&format!("{FOO_924} foo"), "$var.class.name $var.name");
}

#[test]
fn test_velocity924_method() {
    let mut b = base_924();
    b.assert_eval_equals(&format!("{FOO_924} foo"), "$var.class.getName() $var.getName()");
    b.assert_eval_equals(&format!("{FOO_924} foo"), "$var.class.name $var.name");
}

#[test]
fn test_velocity924_getter2() {
    base_924().assert_eval_equals(&format!("foo {FOO_924}"), "$var.name $var.class.name");
}

#[test]
fn test_velocity924_method2() {
    let mut b = base_924();
    b.assert_eval_equals(&format!("foo {FOO_924}"), "$var.getName() $var.class.getName()");
    b.assert_eval_equals(&format!("{FOO_924} foo"), "$var.class.name $var.name");
}

// ---- Velocity926TestCase ----

#[test]
fn test_names_collision() {
    Base::new().assert_eval_equals("bar foo", "#set($foo='foo')#set($bar='bar')#macro(test, $foo, $bar)$foo $bar#end#test($bar, $foo)");
}

// ---- Velocity927TestCase ----

#[test]
fn test_it_927() {
    Base::new().assert_eval_equals("", "#set($map = {\n })");
}

// ---- Velocity940TestCase ----

#[test]
fn test_non_block_inside_block() {
    Base::new().assert_eval_equals("Something First Something Second ", "#macro(test $label)Something $!label $!bodyContent#{end}#@test('First')#test('Second')#end");
}

#[test]
fn test_block_inside_block() {
    Base::new().assert_eval_equals("Something First Something Second Somewhere", "#macro(test $label)Something $!label $!bodyContent#{end}#@test('First')#@test('Second')Somewhere#end#end");
}

#[test]
fn test_block_inside_block_body_restored() {
    Base::new().assert_eval_equals(
        "Something First Something Second Somewhere After Somewhere After Something Second Somewhere After Somewhere",
        "#macro(test $label)Something $!label $!bodyContent After $!bodyContent#{end}#@test('First')#@test('Second')Somewhere#end#end",
    );
}

#[test]
fn test_block_inside_body() {
    Base::new().assert_eval_equals(
        "Something First Somehow Inside With Outside Someone Objectively Hidden",
        "#macro(test2 $label)Somehow $!label With $!bodyContent Someone#end#macro(test $label)Something $!label #@test2('Inside')Outside#end Objectively $!bodyContent#end#@test('First')Hidden#end",
    );
}

// ---- Velocity944TestCase ----

fn base_944() -> Base {
    let some = class(&format!("{P}Velocity944TestCase$SomeClass")).method("someMethod", &[string()], Some(string()), |_, a| ret(a[0].clone())).build();
    let mut b = Base::new();
    b.set_property("velocimacro.inline.local_scope", "true");
    b.put("someObj", some.new_instance());
    b
}

#[test]
fn test_velocity944_string_literal() {
    base_944().assert_eval_equals("<span>foo</span> some text <span>bar</span>", "#macro( m $v )<span>$v</span>#end#m( 'foo' ) some text #set($str=\"#m( 'bar' )\")$str");
}

#[test]
fn test_velocity944_reference_argument() {
    base_944().assert_eval_equals("<span>foo</span> some text <span>bar</span>", "#macro( m $v )<span>$v</span>#end#m( 'foo' ) some text ${someObj.someMethod(\"#m( 'bar' )\")}");
}

#[test]
fn test_velocity944_loaded_template() {
    let mut b = base_944();
    b.add_template("velocity944", "#macro( m $v )<span>$v</span>#end#m( 'foo' ) some text ${someObj.someMethod(\"#m( 'bar' )\")}");
    b.assert_tmpl_equals("<span>foo</span> some text <span>bar</span>", "velocity944");
}

// ---- Velocity952TestCase ----

fn base_952() -> Base {
    let t = format!("{P}Velocity952TestCase$");
    class(&format!("{t}Foo")).interface().build();
    let bar_methods = |c: BeanClass| {
        c.method("foo", &[], Some(string()), |_, _| ret(s("bar")))
            .method("foo2", &[], Some(string()), |_, _| ret(s("public")))
            .static_method("staticFoo", &[], Some(string()), |_| ret(s("static bar")))
    };
    let bar = bar_methods(class(&format!("{t}Bar")).extends(&[&format!("{t}PrivateSuperClass")]).implements(&[&format!("{t}Foo")])).build();
    let baz_class = class(&format!("{t}Baz"))
        .extends(&[&format!("{t}Bar"), &format!("{t}PrivateSuperClass")])
        .implements(&[&format!("{t}Foo")])
        .method("foo", &[], Some(string()), |_, _| ret(s("baz")))
        .static_method("staticFoo", &[], Some(string()), |_| ret(s("static baz")))
        .method_of(&format!("{t}Bar"), "foo2", &[], Some(string()), |_, _| ret(s("public")))
        .build();
    let mut b = Base::new();
    // The tests expect the default time zone upstream's build ran in: Europe/Paris (+01:00 in 1970).
    b.put("tz", java_compat::util::TimeZone::get("Europe/Paris").value());
    b.put("bar", bar.new_instance());
    b.put("baz", baz_class.new_instance());
    b
}

#[test]
fn test_end2_end() {
    base_952().assert_eval_equals("3600000", "$tz.getOffset(1)");
}

#[test]
fn test_bar() {
    let mut b = base_952();
    b.assert_eval_equals("bar", "$bar.foo()");
    b.assert_eval_equals("static bar", "$bar.staticFoo()");
}

#[test]
fn test_not_accessible_abstract() {
    base_952().assert_eval_equals("public", "$bar.foo2()");
}

#[test]
fn test_baz() {
    let mut b = base_952();
    b.assert_eval_equals("baz", "$baz.foo()");
    b.assert_eval_equals("static baz", "$baz.staticFoo()");
}

// ---- Velocity961TestCase ----

#[test]
fn test1_961() {
    Base::new().assert_eval_equals("$child.typeName()", "$child.typeName()#if($child.isRepeated())[]#end");
}

#[test]
fn test2_961() {
    Base::new().assert_eval_equals("$child.typeName()[]", "$child.typeName()#if(1)[]#end");
}
