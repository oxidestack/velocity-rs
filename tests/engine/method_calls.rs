//! `ArrayMethodsTestCase`, `MethodOverloadingTestCase`, `PropertyMethodPrecedenceTestCase`,
//! `StaticUtilityMethodsTestCase`, `NumberMethodCallsTestCase`, `ExpressionAsMethodArgumentTestCase`,
//! `VarargMethodsTestCase`.

use std::sync::Arc;

use java_compat::lang::{JArray, JList, JValue};
use java_compat::reflect::JType;
use velocity::props::{ExtProperties, PropValue};
use velocity::{Context, Engine};

use crate::base::*;
use crate::provider::number_methods;
use crate::test_logger::TestLogger;

// ---- ArrayMethodsTestCase ----

fn check_result(b: &mut Base, method: &str, expected: &str, compare_to_list: bool) {
    let result = b.evaluate(&format!("$!array.{method}")).unwrap();
    assert_eq!(result, expected, "$!array.{method}");
    if compare_to_list {
        let list_result = b.evaluate(&format!("$!list.{method}")).unwrap();
        assert_eq!(result, list_result, "$!list.{method}");
    }
}

fn check_results(b: &mut Base, array: &Arc<JArray>, setme: Option<JValue>, compare_to_list: bool) {
    b.put("array", JValue::Array(array.clone()));
    if compare_to_list {
        b.put("list", JValue::List(JList::array_list(array.items())));
    }
    match &setme {
        Some(v) => b.put("setme", v.clone()),
        None => {
            b.context.remove("setme");
        }
    }
    let size = array.len();
    check_result(b, "size()", &size.to_string(), compare_to_list);
    let is_empty = size == 0;
    check_result(b, "isEmpty()", &is_empty.to_string(), compare_to_list);
    check_result(b, "empty", &is_empty.to_string(), compare_to_list);
    let render_array = b.evaluate("$array").unwrap();
    let render_list = b.evaluate("$list").unwrap();
    assert_eq!(render_array == render_list, compare_to_list, "{render_array} vs {render_list}");
    for i in 0..size {
        b.put("index", JValue::Int(i as i32));
        let value = array.get(i).unwrap();
        let get = "get($index)";
        let set = format!("set({i}, $setme)");
        let shown = if value.is_null() { String::new() } else { value.to_java_string() };
        check_result(b, get, &shown, compare_to_list);
        check_result(b, &set, &shown, compare_to_list);
        let now = array.get(i).unwrap();
        match &setme {
            None => assert!(now.is_null()),
            Some(v) => assert!(java_compat::lang::java_equals(&now, v), "{now:?} != {v:?}"),
        }
        match &setme {
            None => check_result(b, get, "", compare_to_list),
            Some(v) => {
                check_result(b, get, &v.to_java_string(), compare_to_list);
                check_result(b, "contains($setme)", "true", compare_to_list);
            }
        }
    }
}

#[test]
fn test_array_methods() {
    let mut b = Base::new();
    let strings = Arc::new(JArray::new(JType::string(), vec![s("foo"), s("bar"), s("baz")]));
    check_results(&mut b, &strings, Some(s("woogie")), true);
    let ints = Arc::new(JArray::new(JType::Int, vec![JValue::Int(1), JValue::Int(3), JValue::Int(7)]));
    check_results(&mut b, &ints, Some(JValue::Int(11)), false);
    let mixed = Arc::new(JArray::new(JType::object(), vec![JValue::Double(2.2), JValue::Null]));
    check_results(&mut b, &mixed, Some(s("whatever")), true);
    check_results(&mut b, &mixed, None, true);
    let empty = Arc::new(JArray::new(JType::object(), vec![]));
    check_results(&mut b, &empty, None, true);
    let lt = b.evaluate("$list.get(0)").unwrap_err();
    let at = b.evaluate("$array.get(0)").unwrap_err();
    assert_eq!(lt.class_name(), at.class_name());
}

// ---- MethodOverloadingTestCase ----

fn overloading(basefilename: &str) {
    let logger = Arc::new(TestLogger::default());
    let mut p = ExtProperties::new();
    p.add_property("resource.loader.file.path", resource("methodoverloading").into());
    let instance: Arc<dyn velocity::log::Logger> = logger.clone();
    p.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    let ve = Engine::with_properties(&p).unwrap();
    let template = ve.get_template(&format!("{basefilename}.vm"), None).unwrap();
    let test = class("org.apache.velocity.test.MethodOverloadingTestCase")
        .method("overloadedMethod", &[JType::class("java.lang.Integer")], Some(string()), |_, _| ret(s("Integer")))
        .method("overloadedMethod", &[string()], Some(string()), |_, _| ret(s("String")))
        .method("overloadedMethod2", &[JType::class("java.lang.Integer")], Some(string()), |_, _| ret(s("Integer")))
        .method("overloadedMethod2", &[string()], Some(string()), |_, _| ret(s("String")))
        .build()
        .new_instance();
    let mut context = Context::new();
    context.put("test", test);
    context.put("nullValue", JValue::Null);
    logger.on();
    let out = ve.merge(&template, &mut context).unwrap();
    logger.off();
    let compare = file_contents(&resource(&format!("methodoverloading/compare/{basefilename}.cmp")));
    assert_eq!(normalize_newlines(&out), normalize_newlines(&compare), "Output incorrect.");
    assert!(!logger.get_log().contains("IllegalArgumentException"));
}

#[test]
fn test_method_overloading() {
    overloading("single");
}

#[test]
fn test_parsed_method_overloading() {
    overloading("main");
}

// ---- PropertyMethodPrecedenceTestCase ----

fn precedence() -> Base {
    const P: &str = "org.apache.velocity.test.PropertyMethodPrecedenceTestCase$";
    let is_tool = |c: BeanClass| c.method("isFoo", &[], Some(JType::Boolean), |_, _| ret(JValue::Bool(true)));
    let getis = |c: BeanClass| is_tool(c).method("get", &[string()], Some(string()), |_, a| ret(s(&format!("get({})", a[0].to_java_string()))));
    let get_foo = |c: BeanClass| getis(c).method("getFoo", &[], Some(string()), |_, _| ret(s("getFoo")));
    let getfoo = |c: BeanClass| c.method("getfoo", &[], Some(string()), |_, _| ret(s("getfoo")));
    let mut b = Base::new();
    // Subclass methods come first in `getMethods()`.
    b.put("geta", get_foo(getfoo(class(&format!("{P}getGetgetisTool")))).build().new_instance());
    b.put("getA", get_foo(class(&format!("{P}GetgetisTool"))).build().new_instance());
    b.put("geta2", getis(getfoo(class(&format!("{P}get2getisTool")))).build().new_instance());
    b.put("get_a", getis(class(&format!("{P}getisTool"))).build().new_instance());
    b.put("isA", is_tool(class(&format!("{P}isTool"))).build().new_instance());
    b
}

#[test]
fn test_lowercase_property_methods() {
    let mut b = precedence();
    b.assert_eval_equals("getfoo", "$geta.foo");
    b.assert_eval_equals("getFoo", "$getA.foo");
    b.assert_eval_equals("get(foo)", "$get_a.foo");
    b.assert_eval_equals("true", "$isA.foo");
}

#[test]
fn test_uppercase_property_methods() {
    let mut b = precedence();
    b.assert_eval_equals("getFoo", "$geta.Foo");
    b.assert_eval_equals("getfoo", "$geta2.Foo");
    b.assert_eval_equals("getFoo", "$getA.Foo");
    b.assert_eval_equals("get(Foo)", "$get_a.Foo");
    b.assert_eval_equals("true", "$isA.Foo");
}

// ---- StaticUtilityMethodsTestCase ----

#[test]
fn test_math() {
    // A stand-in for `Math.class` with the static method the test calls.
    let math = class("java.lang.Math").static_method("ceil", &[JType::Double], Some(JType::Double), |a| ret(JValue::Double(num(&a[0]).ceil()))).build();
    let mut b = Base::new();
    b.put("Math", math.class_object());
    b.assert_eval_equals("java.lang.Math", "$Math.name");
    b.assert_eval_equals("3.0", "$Math.ceil(2.5)");
}

#[test]
fn test_foo() {
    let foo = class("org.apache.velocity.test.StaticUtilityMethodsTestCase$Foo")
        .static_method("foo", &[string()], Some(string()), |a| ret(if a[0].is_null() { s("foo") } else { a[0].clone() }))
        .build();
    let mut b = Base::new();
    b.put("Foo", foo.class_object());
    b.assert_eval_equals("test", "$Foo.foo('test')");
}

// ---- NumberMethodCallsTestCase ----

#[test]
fn test_number_method_calls() {
    let ve = Engine::with_properties(&ExtProperties::new()).unwrap();
    let mut vc = Context::new();
    vc.put("Test", number_methods());
    vc.put("AByte", byte(10));
    vc.put("AShort", short(10));
    vc.put("AInteger", JValue::Int(10));
    vc.put("ALong", JValue::Long(10));
    vc.put("ADouble", JValue::Double(10.0));
    vc.put("AFloat", JValue::Float(10.0));
    vc.put("ABigDecimal", big_decimal("10"));
    vc.put("ABigInteger", big_integer(10));
    for (template, compare) in [
        ("$Test.numMethod($AByte)", "byte (10)"),
        ("$Test.numMethod($AShort)", "short (10)"),
        ("$Test.numMethod($AInteger)", "int (10)"),
        ("$Test.numMethod($ADouble)", "double (10.0)"),
        ("$Test.numMethod($AFloat)", "double (10.0)"),
        ("$Test.numMethod($ALong)", "long (10)"),
        ("$Test.numMethod($ABigDecimal)", "BigDecimal (10)"),
        ("$Test.numMethod($ABigInteger)", "BigInteger (10)"),
        ("$Test.numMethod(10.0)", "double (10.0)"),
        ("$Test.numMethod(10)", "int (10)"),
        ("$Test.numMethod(10000000000)", "long (10000000000)"),
        ("$Test.numMethod(10000000000000000000000)", "BigInteger (10000000000000000000000)"),
        ("#set($val = 10.0 + 1.5)$Test.numMethod($val)", "double (11.5)"),
        ("#set($val = 100 + 1)$Test.numMethod($val)", "int (101)"),
        ("#set($val = 100 * 1000)$Test.numMethod($val)", "int (100000)"),
        ("#set($val = 100 + 1.5)$Test.numMethod($val)", "double (101.5)"),
        ("#set($val = $ALong + $AInteger)$Test.numMethod($val)", "long (20)"),
        ("#set($val = $ABigInteger + $AInteger)$Test.numMethod($val)", "BigInteger (20)"),
    ] {
        assert_eq!(ve.evaluate(&mut vc, "test", template).unwrap(), compare, "Incorrect results for template '{template}'.");
    }
}

// ---- ExpressionAsMethodArgumentTestCase ----

#[test]
fn test_expression_as_method() {
    let mut b = Base::new();
    let tool = class("org.apache.velocity.test.ExpressionAsMethodArgumentTestCase$MyAbsTool")
        .method("abs", &[JType::Int], Some(JType::Int), |_, a| ret(JValue::Int(match a[0] {
            JValue::Int(i) => i.wrapping_abs(),
            _ => 0,
        })))
        .build();
    b.put("tool", tool.new_instance());
    b.put("foo", JValue::Int(2));
    b.put("bar", JValue::Int(-3));
    b.assert_eval_equals("6", "$tool.abs( $foo * $bar )");
    b.assert_eval_equals("12", "$tool.abs( $foo * $tool.abs( $foo * $bar ) )");
}

// ---- VarargMethodsTestCase ----

fn array_items(v: &JValue) -> Vec<JValue> {
    match v {
        JValue::Array(a) => a.items(),
        _ => Vec::new(),
    }
}

fn nice_methods(c: BeanClass) -> BeanClass {
    c.method("var", &[JType::array(string())], Some(string()), |_, a| ret(s(&array_items(&a[0]).iter().map(|v| v.to_java_string()).collect::<String>())))
        .method("add", &[JType::array(JType::Double)], Some(JType::Double), |_, a| ret(JValue::Double(array_items(&a[0]).iter().map(num).fold(0.0, |t, v| t + v))))
        .method("test", &[JType::array(JType::object())], Some(string()), |_, _| ret(s("objects")))
        .method("test", &[JType::array(string())], Some(string()), |_, _| ret(s("strings")))
}

fn varargs() -> Base {
    const P: &str = "org.apache.velocity.test.VarargMethodsTestCase$";
    let nice = nice_methods(class(&format!("{P}NiceTool"))).build();
    let number = || JType::class("java.lang.Number");
    let nasty = class(&format!("{P}NastyTool"))
        .extends(&[&format!("{P}NiceTool")])
        .method("var", &[string()], Some(string()), |_, a| ret(s(&format!("only{}", a[0].to_java_string()))))
        .method("add", &[JType::array(JType::Int)], Some(JType::Int), |_, a| ret(JValue::Int(array_items(&a[0]).iter().map(|v| if let JValue::Int(i) = v { *i } else { 0 }).sum())))
        .method("add", &[JType::Int], Some(JType::Int), |_, a| ret(a[0].clone()))
        .method("test", &[], Some(string()), |_, _| ret(s("noargs")))
        .method("test", &[object()], Some(object()), |_, _| ret(s("object")))
        .method("test", &[string()], Some(object()), |_, _| ret(s("string")))
        .method("test", &[JType::array(JType::object())], Some(string()), |_, _| ret(s("object[]")))
        .method("test", &[object(), string()], Some(string()), |_, _| ret(s("object,string")))
        .method("test642", &[JType::array(JType::object())], Some(string()), |_, a| {
            if a[0].is_null() {
                return ret(JValue::Null);
            }
            ret(s(&format!("[{}]", array_items(&a[0]).iter().map(|v| v.to_java_string()).collect::<Vec<_>>().join(", "))))
        })
        .method("test649", &[JType::array(JType::Int)], Some(string()), |_, _| ret(s("int[]")))
        .method("test651", &[string(), string(), JType::array(JType::object())], Some(string()), |_, _| ret(s("String,String,Object[]")))
        .method("test651", &[string(), JType::class("java.util.List")], Some(string()), |_, _| ret(s("String,List")))
        .method("max", &[number(), number()], Some(number()), |_, a| ret(max_number(a)))
        .method("max", &[JType::array(number())], Some(number()), |_, a| ret(max_number(&array_items(&a[0]))));
    // `NiceTool`'s methods are inherited (and `test(Object[])` overridden).
    let nasty = nice_methods(nasty).build();
    let mut b = Base::new();
    b.put("nice", nice.new_instance());
    b.put("nasty", nasty.new_instance());
    b.put("objects", JArray::objects(vec![new_object(), s("org.apache.velocity.VelocityContext")]));
    b.put("strings", JValue::Array(Arc::new(JArray::new(string(), vec![s("one"), s("two")]))));
    b.put("doubles", JValue::Array(Arc::new(JArray::new(JType::Double, vec![JValue::Double(1.5), JValue::Double(2.5)]))));
    b.put("float", JValue::Float(1.0));
    b.put("ints", JValue::Array(Arc::new(JArray::new(JType::Int, vec![JValue::Int(1), JValue::Int(2)]))));
    b
}

/// `NastyTool.max(Number[])`: the first largest by `floatValue()`.
fn max_number(numbers: &[JValue]) -> JValue {
    if numbers.is_empty() {
        return JValue::Null;
    }
    let mut index = None;
    let mut val = f64::MIN_POSITIVE;
    for (i, n) in numbers.iter().enumerate() {
        let f = num(n) as f32 as f64;
        if f > val {
            index = Some(i);
            val = f;
        }
    }
    numbers[index.unwrap_or(0)].clone()
}

#[test]
fn test_strings() {
    let mut b = varargs();
    b.assert_eval_equals("onetwo", "$nice.var($strings)");
    b.assert_eval_equals("onetwo", "$nice.var('one','two')");
    b.assert_eval_equals("one", "$nice.var('one')");
    b.assert_eval_equals("", "$nice.var()");
}

#[test]
fn test_doubles() {
    let mut b = varargs();
    b.assert_eval_equals("4.0", "$nice.add($doubles)");
    b.assert_eval_equals("3.0", "$nice.add(1,2)");
    b.assert_eval_equals("1.0", "$nice.add(1)");
    b.assert_eval_equals("0.0", "$nice.add()");
}

#[test]
fn test_float_to_double_var_arg() {
    varargs().assert_eval_equals("1.0", "$nice.add($float)");
}

#[test]
fn test_string_vs_strings() {
    let mut b = varargs();
    b.assert_eval_equals("onlyone", "$nasty.var('one')");
    b.assert_eval_equals("onlynull", "$nasty.var($null)");
    b.assert_eval_equals("", "$nasty.var()");
}

#[test]
fn test_int_vs_doubles() {
    let mut b = varargs();
    b.assert_eval_equals("1", "$nasty.add(1)");
    b.assert_eval_equals("1.0", "$nasty.add(1.0)");
    b.assert_eval_equals("3.0", "$nasty.add(1.0,2)");
}

#[test]
fn test_ints() {
    let mut b = varargs();
    b.assert_eval_equals("3", "$nasty.add($ints)");
    b.assert_eval_equals("3", "$nasty.add(1,2)");
    b.assert_eval_equals("1", "$nasty.add(1)");
    b.assert_eval_equals("0", "$nasty.add()");
}

#[test]
fn test_strings_vs_objects_aka_subclass_vararg() {
    let mut b = varargs();
    b.assert_eval_equals("objects", "$nice.test($objects)");
    b.assert_eval_equals("objects", "$nice.test($nice,$nasty,$ints)");
    b.assert_eval_equals("strings", "$nice.test('foo')");
}

#[test]
fn test_object_var_arg_vs_object_etc() {
    varargs().assert_eval_equals("object,string", "$nasty.test($nice,'foo')");
}

#[test]
fn test_object_var_arg_vs_object_velocity605() {
    let mut b = varargs();
    b.assert_eval_equals("string", "$nasty.test('joe')");
    b.assert_eval_equals("object", "$nasty.test($nice)");
}

#[test]
fn test_no_args() {
    varargs().assert_eval_equals("noargs", "$nasty.test()");
}

#[test]
fn test_passing_array_to_var_arg_velocity642() {
    let mut b = varargs();
    b.assert_eval_equals("[one, two]", "$nasty.test642($strings)");
    b.assert_eval_equals("[1, 2]", "#set( $list = [1..2] )$nasty.test642($list.toArray())");
}

#[test]
fn test_null_to_primitive_var_arg() {
    varargs().assert_eval_equals("int[]", "$nasty.test649($null)");
}

#[test]
fn test_args_before_vararg_with_no_args() {
    varargs().assert_eval_equals("String,String,Object[]", "$nasty.test651('a','b')");
}

#[test]
fn test_velocity651() {
    varargs().assert_eval_equals("String,List", "$nasty.test651('test',['TEST'])");
}

#[test]
fn test_max() {
    varargs().assert_eval_equals("4", "$nasty.max(4, 3.5)");
}

/// `Number.doubleValue()`.
fn num(v: &JValue) -> f64 {
    match v {
        JValue::Byte(b) => *b as f64,
        JValue::Short(x) => *x as f64,
        JValue::Int(i) => *i as f64,
        JValue::Long(l) => *l as f64,
        JValue::Float(f) => *f as f64,
        JValue::Double(d) => *d,
        other => other.to_java_string().parse().unwrap_or(0.0),
    }
}
