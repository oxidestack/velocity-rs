//! `IntrospectorTestCase`, `Introspector2TestCase`, `Introspector3TestCase`, `IntrospectionCacheDataTestCase`,
//! `MethodCacheKeyTestCase`, `ClassMapTestCase`, `EnumConstantConversionTestCase`.

use std::sync::Arc;

use java_compat::lang::{JList, JValue};
use java_compat::reflect::{ClassDef, JMethod, JType};
use velocity::uberspector::{ClassMap, Introspector, MethodCacheKey};
use velocity::{Context, Engine};

use crate::base::*;

fn invoke(m: &JMethod, target: &JValue, args: &[JValue]) -> String {
    velocity::uberspect::invoke(m, target, args).ok().flatten().expect("a result").to_java_string()
}

fn method_provider() -> BeanFactory {
    let m = |label: &'static str| move |_: &Bean, _: &[JValue]| ret(s(label));
    // `untouchable()` (package-private) and `reallyuntouchable()` (private) are not public methods.
    class("org.apache.velocity.test.IntrospectorTestCase$MethodProvider")
        .method("booleanMethod", &[JType::Boolean], Some(string()), m("boolean"))
        .method("byteMethod", &[JType::Byte], Some(string()), m("byte"))
        .method("characterMethod", &[JType::Char], Some(string()), m("character"))
        .method("doubleMethod", &[JType::Double], Some(string()), m("double"))
        .method("floatMethod", &[JType::Float], Some(string()), m("float"))
        .method("integerMethod", &[JType::Int], Some(string()), m("integer"))
        .method("longMethod", &[JType::Long], Some(string()), m("long"))
        .method("shortMethod", &[JType::Short], Some(string()), m("short"))
        .build()
}

fn primitive(kind: &str, param: JValue) {
    let mp = method_provider();
    let introspector = Introspector::new(None);
    let m = introspector.get_method(&mp.class, &format!("{kind}Method"), std::slice::from_ref(&param)).expect("Method could not be found");
    assert_eq!(invoke(&m, &mp.new_instance(), &[param]), kind, "Method could not be found");
}

#[test]
fn test_introspector_boolean() {
    primitive("boolean", JValue::Bool(true));
}

#[test]
fn test_introspector_byte() {
    primitive("byte", byte(1));
}

#[test]
fn test_introspector_char() {
    primitive("character", JValue::Char('a'));
}

#[test]
fn test_introspector_double() {
    primitive("double", JValue::Double(1.0));
}

#[test]
fn test_introspector_float() {
    primitive("float", JValue::Float(1.0));
}

#[test]
fn test_introspector_integer() {
    primitive("integer", JValue::Int(1));
}

#[test]
fn test_introspector_primitive_long() {
    primitive("long", JValue::Long(1));
}

#[test]
fn test_introspector_primitive_short() {
    primitive("short", short(1));
}

#[test]
fn test_introspector_untouchable() {
    assert!(Introspector::new(None).get_method(&method_provider().class, "untouchable", &[]).is_none(), "able to access a private-access method.");
}

#[test]
fn test_introspector_really_untouchable() {
    assert!(Introspector::new(None).get_method(&method_provider().class, "reallyuntouchable", &[]).is_none(), "able to access a private-access method.");
}

const P2: &str = "org.apache.velocity.test.Introspector2TestCase$";

fn finder(name: &str, overloads: &[(&[&str], &'static str)]) -> Arc<ClassDef> {
    let mut c = class(&format!("{P2}{name}"));
    for (params, result) in overloads {
        let types: Vec<JType> = params.iter().map(|p| if *p == "Object" { object() } else { JType::class(&format!("{P2}{p}")) }).collect();
        let result = *result;
        c = c.static_method("find", &types, Some(string()), move |_| ret(s(result)));
    }
    c.build().class
}

#[test]
fn test_introspector() {
    class(&format!("{P2}Woogie")).interface().build();
    class(&format!("{P2}Bar")).implements(&[&format!("{P2}Woogie")]).build();
    let foo = class(&format!("{P2}Foo")).extends(&[&format!("{P2}Bar")]).implements(&[&format!("{P2}Woogie")]).build();
    let tester = finder(
        "Tester",
        &[(&["Woogie", "Object"], "Woogie-Object"), (&["Object", "Bar"], "Object-Bar"), (&["Bar", "Bar"], "Bar-Bar"), (&["Object"], "Object"), (&["Woogie"], "Woogie")],
    );
    let tester2 = finder(
        "Tester2",
        &[(&["Woogie", "Object"], "Woogie-Object"), (&["Object", "Bar"], "Object-Bar"), (&["Bar", "Object"], "Bar-Object"), (&["Object"], "Object"), (&["Woogie"], "Woogie")],
    );
    let params = [foo.new_instance(), foo.new_instance()];
    let introspector = Introspector::new(None);
    let m = introspector.get_method(&tester, "find", &params).expect("Returned method was null");
    assert_eq!(invoke(&m, &JValue::Null, &params), "Bar-Bar");
    assert!(introspector.get_method(&tester2, "find", &params).is_none(), "Introspector shouldn't have found a method as it's ambiguous.");
}

#[test]
fn test_simple() {
    let m = |label: &'static str| move |_: &Bean, _: &[JValue]| ret(s(label));
    let list = || JType::class("java.util.List");
    let mp = class("org.apache.velocity.test.Introspector3TestCase$MethodProvider")
        .method("ii", &[JType::Int, JType::Int], Some(string()), m("ii"))
        .method("lii", &[list(), JType::Int, JType::Int], Some(string()), m("lii"))
        .method("lll", &[list(), JType::Long, list()], Some(string()), m("lll"))
        .method("lll", &[list(), JType::Long, JType::Int], Some(string()), m("lli"))
        .method("lll", &[list(), JType::Long], Some(string()), m("Listl"))
        .method("ll", &[JType::Long, JType::Long], Some(string()), m("ll"))
        .build();
    let target = mp.new_instance();
    let empty = || JValue::List(JList::array_list(vec![]));
    let introspector = Introspector::new(None);
    for (name, args, expected) in [
        ("lii", vec![empty(), JValue::Int(1), JValue::Int(2)], "lii"),
        ("ii", vec![JValue::Int(1), JValue::Int(2)], "ii"),
        ("ll", vec![JValue::Long(1), JValue::Int(2)], "ll"),
        ("ll", vec![JValue::Long(1), JValue::Long(2)], "ll"),
        ("lll", vec![empty(), JValue::Long(1), empty()], "lll"),
        ("lll", vec![JValue::Null, JValue::Int(0)], "Listl"),
    ] {
        let method = introspector.get_method(&mp.class, name, &args).unwrap_or_else(|| panic!("{name} not found"));
        assert_eq!(invoke(&method, &target, &args), expected);
    }
}

#[test]
fn test_cache() {
    let this = class("org.apache.velocity.test.IntrospectionCacheDataTestCase")
        .method("exec", &[string()], Some(string()), |_, a| ret(s(&format!("[{}]", a[0].to_java_string()))))
        .build();
    let mut context = Context::new();
    context.put("this", this.new_instance());
    let ve = Engine::new();
    assert_eq!(ve.evaluate(&mut context, "test", "$this.exec('a')$this.exec('b')").unwrap(), "[a][b]");
    assert!(context.icache_hits() > 0);
}

fn self_equals_assertions(mck: &MethodCacheKey) {
    // The upstream test's `assertEquals(mck, mck)`: equality's reflexivity is what it checks.
    assert!(PartialEq::eq(mck, mck));
    assert_ne!(Some(mck), None);
}

#[test]
fn test_method_key_cache_equals() {
    let object = || Some("java.lang.Object".to_string());
    let mck1 = MethodCacheKey::new("test", Some(vec![object()]), false);
    self_equals_assertions(&mck1);
    assert_eq!(mck1, MethodCacheKey::new("test", Some(vec![object()]), false));
    assert_ne!(mck1, MethodCacheKey::new("test", Some(vec![Some("java.lang.String".into())]), false));
    assert_ne!(mck1, MethodCacheKey::new("boo", Some(vec![object()]), false));
    self_equals_assertions(&MethodCacheKey::new("boo", Some(vec![]), false));
    self_equals_assertions(&MethodCacheKey::new("boo", None, false));
    self_equals_assertions(&MethodCacheKey::new("boo", Some(vec![]), false));
    self_equals_assertions(&MethodCacheKey::new("boo", Some(vec![None]), false));
    self_equals_assertions(&MethodCacheKey::new("boo", Some(vec![object()]), false));
}

#[test]
fn test_primitives() {
    let void = |_: &Bean, _: &[JValue]| Ok(None);
    let def = class("org.apache.velocity.test.util.introspection.ClassMapTestCase$TestClassMap")
        .method("setBoolean", &[JType::Boolean], None, void)
        .method("setByte", &[JType::Byte], None, void)
        .method("setCharacter", &[JType::Char], None, void)
        .method("setDouble", &[JType::Double], None, void)
        .method("setFloat", &[JType::Float], None, void)
        .method("setInteger", &[JType::Int], None, void)
        .method("setLong", &[JType::Long], None, void)
        .method("setShort", &[JType::Short], None, void)
        .build();
    let c = ClassMap::new(&def.class);
    for (name, arg) in [
        ("setBoolean", JValue::Bool(true)),
        ("setByte", byte(4)),
        ("setCharacter", JValue::Char('c')),
        ("setDouble", JValue::Double(8.0)),
        ("setFloat", JValue::Float(15.0)),
        ("setInteger", JValue::Int(16)),
        ("setLong", JValue::Long(23)),
        ("setShort", short(42)),
    ] {
        assert!(c.find(name, &[arg]).unwrap().is_some(), "{name}");
    }
}

#[test]
fn test_string_to_enum_conversion() {
    const P: &str = "org.apache.velocity.test.util.introspection.EnumConstantConversionTestCase$Obj";
    let color = format!("{P}$Color");
    class(&color).enum_of(&["RED", "GREEN"]).build();
    let obj = class(P)
        .method("getAction", &[JType::class(&color)], Some(string()), |_, a| {
            ret(s(match a[0].to_java_string().as_str() {
                "RED" => "Stop",
                "GREEN" => "Go",
                _ => "???",
            }))
        })
        .build();
    let mut b = Base::new();
    b.put("obj", obj.new_instance());
    b.assert_eval_equals("Stop", "$obj.getAction('RED')");
    b.assert_eval_equals("Go", "$obj.getAction('GREEN')");
    b.assert_eval_exception("$obj.getAction('BLUE')", Some("org.apache.velocity.exception.MethodInvocationException"));
}
