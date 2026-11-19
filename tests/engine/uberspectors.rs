//! `UberspectorTestCase`, `UberspectImplTestCase`, `ChainedUberspectorsTestCase`,
//! `DeprecatedCheckUberspectorsTestCase`, `SecureIntrospectionTestCase`, `InfoTestCase`.

use std::sync::Arc;

use java_compat::lang::{JError, JList, JMap, JValue, ListKind, MapKind};
use java_compat::reflect::JType;
use velocity::events::Info;
use velocity::props::{ExtProperties, PropValue};
use velocity::uberspector::{Uberspect, UberspectClass, UberspectImpl, VelMethod, VelPropertyGet, VelPropertySet, register_uberspect_class};
use velocity::{Context, Engine, VelocityError};

use crate::base::*;
use crate::test_logger::TestLogger;

type R<T> = Result<T, VelocityError>;

fn uberspect() -> Arc<dyn Uberspect> {
    Engine::with_properties(&ExtProperties::new()).unwrap().uberspect()
}

fn no_info() -> Info {
    Info::default()
}

fn getter_name(u: &Arc<dyn Uberspect>, obj: &JValue, identifier: Option<&str>) -> Option<String> {
    u.get_property_get(obj, identifier, &no_info()).unwrap().map(|g| g.method_name())
}

fn setter_name(u: &Arc<dyn Uberspect>, obj: &JValue, identifier: Option<&str>, arg: &JValue) -> Option<String> {
    u.get_property_set(obj, identifier, arg, &no_info()).unwrap().map(|g| g.method_name())
}

fn hash_map() -> JValue {
    JValue::Map(JMap::new(MapKind::Hash))
}

/// `org.apache.velocity.test.misc.GetPutObject`.
fn get_put_object() -> JValue {
    class("org.apache.velocity.test.misc.GetPutObject")
        .method("get", &[], Some(object()), |b, _| ret(b.field("value")))
        .method("put", &[object()], None, |b, a| {
            b.set_field("value", a[0].clone());
            Ok(None)
        })
        .build()
        .new_instance()
}

/// `org.apache.velocity.test.misc.UberspectorTestObject`.
fn uberspector_test_object() -> JValue {
    let field = |f: &'static str| move |b: &Bean, _: &[JValue]| ret(b.field(f));
    let set = |f: &'static str| {
        move |b: &Bean, a: &[JValue]| {
            b.set_field(f, a[0].clone());
            Ok(None)
        }
    };
    class("org.apache.velocity.test.misc.UberspectorTestObject")
        .method("getpremium", &[], Some(string()), field("premium"))
        .method("setpremium", &[string()], None, set("premium"))
        .method("ispremiumBool", &[], Some(JType::Boolean), field("premiumBool"))
        .method("setpremiumBool", &[JType::Boolean], None, set("premiumBool"))
        .method("getRegular", &[], Some(string()), field("regular"))
        .method("setRegular", &[string()], None, set("regular"))
        .method("isRegularBool", &[], Some(JType::Boolean), field("regularBool"))
        .method("setRegularBool", &[JType::Boolean], None, set("regularBool"))
        .method("getAmbigous", &[], Some(string()), field("ambigous"))
        .method("setAmbigous", &[string()], None, set("ambigous"))
        .method("setAmbigous", &[JType::class("java.lang.StringBuffer")], None, |b, a| {
            b.set_field("ambigous", s(&a[0].to_java_string()));
            Ok(None)
        })
        .method("setUnambiguous", &[string()], None, set("unambiguous"))
        .method("setUnambiguous", &[JType::class("java.util.Map")], None, |b, a| {
            b.set_field("unambiguous", s(&a[0].to_java_string()));
            Ok(None)
        })
        .method("recordField", &[], Some(string()), |_, _| ret(s("")))
        .build()
        .with_fields(&[("premiumBool", JValue::Bool(false)), ("regularBool", JValue::Bool(false))])
}

/// A stand-in `StringBuffer` argument (the test only needs its class).
fn string_buffer() -> JValue {
    class("java.lang.StringBuffer").implements(&["java.lang.CharSequence"]).build().new_instance()
}

#[test]
fn test_null_objects() {
    let u = uberspect();
    assert!(u.get_property_get(&JValue::Null, Some("foo"), &no_info()).unwrap().is_none());
    assert!(u.get_property_set(&JValue::Null, Some("foo"), &new_object(), &no_info()).unwrap().is_none());
}

#[test]
fn test_empty_property_getter() {
    assert_eq!(getter_name(&uberspect(), &hash_map(), Some("")).as_deref(), Some("get"), "Found wrong method");
}

#[test]
fn test_empty_property_setter() {
    assert_eq!(setter_name(&uberspect(), &hash_map(), Some(""), &new_object()).as_deref(), Some("put"), "Found wrong method");
}

#[test]
fn test_null_property_getter() {
    let u = uberspect();
    assert_eq!(getter_name(&u, &get_put_object(), None).as_deref(), Some("get"), "Found wrong method");
    assert_eq!(getter_name(&u, &hash_map(), None), None);
}

#[test]
fn test_null_property_setter() {
    let u = uberspect();
    assert_eq!(setter_name(&u, &get_put_object(), None, &s("")).as_deref(), Some("put"), "Found wrong method");
    assert_eq!(setter_name(&u, &hash_map(), None, &s("")), None);
}

#[test]
fn test_null_parameter_type() {
    let u = uberspect();
    let uto = uberspector_test_object();
    assert_eq!(setter_name(&u, &uto, Some("Regular"), &JValue::Null).as_deref(), Some("setRegular"));
    assert_eq!(setter_name(&u, &uto, Some("Ambigous"), &JValue::Null), None);
    assert_eq!(setter_name(&u, &uto, Some("Ambigous"), &new_object()), None);
}

#[test]
fn test_multiple_parameter_types() {
    let u = uberspect();
    let uto = uberspector_test_object();
    assert_eq!(setter_name(&u, &uto, Some("Ambigous"), &s("")).as_deref(), Some("setAmbigous"));
    assert_eq!(setter_name(&u, &uto, Some("Ambigous"), &string_buffer()).as_deref(), Some("setAmbigous"));
}

#[test]
fn test_regular_getters() {
    let u = uberspect();
    let uto = uberspector_test_object();
    for (identifier, method) in [("Regular", "getRegular"), ("regular", "getRegular"), ("premium", "getpremium"), ("Premium", "getpremium"), ("recordField", "recordField")] {
        assert_eq!(getter_name(&u, &uto, Some(identifier)).as_deref(), Some(method), "Found wrong method for {identifier}");
    }
}

#[test]
fn test_boolean_getters() {
    let u = uberspect();
    let uto = uberspector_test_object();
    for (identifier, method) in [("RegularBool", "isRegularBool"), ("regularBool", "isRegularBool"), ("premiumBool", "ispremiumBool"), ("PremiumBool", "ispremiumBool")] {
        assert_eq!(getter_name(&u, &uto, Some(identifier)).as_deref(), Some(method), "Found wrong method for {identifier}");
    }
}

#[test]
fn test_regular_setters() {
    let u = uberspect();
    let uto = uberspector_test_object();
    for (identifier, method) in [("Regular", "setRegular"), ("regular", "setRegular"), ("premium", "setpremium"), ("Premium", "setpremium")] {
        assert_eq!(setter_name(&u, &uto, Some(identifier), &s("")).as_deref(), Some(method), "Found wrong method for {identifier}");
    }
}

#[test]
fn test_disambiguation() {
    let u = uberspect();
    let uto = uberspector_test_object();
    let setter = u.get_property_set(&uto, Some("unambiguous"), &s("string"), &no_info()).unwrap().expect("a setter");
    assert!(matches!(setter.invoke(&uto, &s("string")), velocity::uberspector::Outcome::Returned(_)));
    let setter = u.get_property_set(&uto, Some("unambiguous"), &hash_map(), &no_info()).unwrap().expect("a setter");
    assert!(matches!(setter.invoke(&uto, &hash_map()), velocity::uberspector::Outcome::Returned(_)));
}

// ---- UberspectImplTestCase ----

fn uberspect_impl() -> Base {
    const P: &str = "org.apache.velocity.test.util.introspection.UberspectImplTestCase$";
    let iterator = || Some(JType::class("java.util.Iterator"));
    let it = |items: [&'static str; 3]| move |_: &Bean, _: &[JValue]| ret(JValue::List(JList::new(ListKind::Iterator, items.iter().map(|v| s(v)).collect())));
    let mut b = Base::new();
    b.set_property("introspector.uberspect.class", velocity::uberspector::UBERSPECT_IMPL);
    b.put("privateClass", class(&format!("{P}PrivateClass")).non_public().method("iterator", &[], iterator(), it(["X", "Y", "Z"])).build().new_instance());
    // A private `iterator()` is no public method at all.
    b.put("privateMethod", class(&format!("{P}PrivateMethod")).build().new_instance());
    b.put("publicMethod", class(&format!("{P}PublicMethod")).method("iterator", &[], iterator(), it(["1", "2", "3"])).build().new_instance());
    b.put("iterable", JValue::List(JList::new(ListKind::Collection, vec![s("1"), s("2"), s("3")])));
    let over = class(&format!("{P}OverloadedMethods"))
        .method("foo", &[], Some(string()), |_, _| ret(s("foo0")))
        .method("foo", &[string()], Some(string()), |_, _| ret(s("foo1")))
        .method("foo", &[string(), string()], Some(string()), |_, _| ret(s("foo2")))
        .method("bar", &[JType::class("java.lang.Number"), JType::Int], Some(string()), |_, _| ret(s("bar1")))
        .method("bar", &[JType::class("java.lang.Number"), string()], Some(string()), |_, _| ret(s("bar2")))
        .build();
    b.put("over", over.new_instance());
    b
}

#[test]
fn test_private_iterator() {
    let mut b = uberspect_impl();
    b.assert_eval_equals("", "#foreach($i in $privateClass)$i#end");
    b.assert_eval_equals("", "#foreach($i in $privateMethod)$i#end");
    b.assert_eval_equals("123", "#foreach($i in $publicMethod)$i#end");
}

#[test]
fn test_iterable_foreach() {
    uberspect_impl().assert_eval_equals("123", "#foreach($i in $iterable)$i#end");
}

#[test]
fn test_overloaded_methods() {
    let mut b = uberspect_impl();
    b.assert_eval_equals("foo0", "$over.foo()");
    b.assert_eval_equals("foo1", "$over.foo('a')");
    b.assert_eval_equals("foo1", "$over.foo($null)");
    b.assert_eval_equals("foo2", "$over.foo('a', 'b')");
    b.assert_eval_equals("foo2", "$over.foo('a', $null)");
    b.assert_eval_equals("bar1", "$over.bar(1,1)");
    b.assert_eval_equals("$over.bar(1,1.1)", "$over.bar(1,1.1)");
    b.assert_eval_equals("bar2", "$over.bar(1,'1.1')");
}

// ---- ChainedUberspectorsTestCase ----

/// `ChainedUberspector`: `getPropertySet` with `foo` replaced by `bar`, everything else from the wrapped one.
struct ChainedUberspector(Arc<dyn Uberspect>);

impl Uberspect for ChainedUberspector {
    fn init(&self, rsvc: &Engine) -> R<()> {
        self.0.init(rsvc)
    }
    fn get_iterator(&self, obj: &JValue, info: &Info) -> R<Option<velocity::uberspector::Iteration>> {
        self.0.get_iterator(obj, info)
    }
    fn get_method(&self, obj: &JValue, name: &str, args: &[JValue], info: &Info) -> R<Option<Arc<dyn VelMethod>>> {
        self.0.get_method(obj, name, args, info)
    }
    fn get_property_get(&self, obj: &JValue, identifier: Option<&str>, info: &Info) -> R<Option<Arc<dyn VelPropertyGet>>> {
        self.0.get_property_get(obj, identifier, info)
    }
    fn get_property_set(&self, obj: &JValue, identifier: Option<&str>, arg: &JValue, info: &Info) -> R<Option<Arc<dyn VelPropertySet>>> {
        let identifier = identifier.map(|i| i.replace("foo", "bar"));
        self.0.get_property_set(obj, identifier.as_deref(), arg, info)
    }
}

/// `LinkedUberspector extends UberspectImpl`: `getPropertyGet` with `foo` replaced by `bar`.
#[derive(Default)]
struct LinkedUberspector(UberspectImpl);

impl Uberspect for LinkedUberspector {
    fn init(&self, rsvc: &Engine) -> R<()> {
        self.0.init(rsvc)
    }
    fn get_iterator(&self, obj: &JValue, info: &Info) -> R<Option<velocity::uberspector::Iteration>> {
        self.0.get_iterator(obj, info)
    }
    fn get_method(&self, obj: &JValue, name: &str, args: &[JValue], info: &Info) -> R<Option<Arc<dyn VelMethod>>> {
        self.0.get_method(obj, name, args, info)
    }
    fn get_property_get(&self, obj: &JValue, identifier: Option<&str>, info: &Info) -> R<Option<Arc<dyn VelPropertyGet>>> {
        let identifier = identifier.map(|i| i.replace("foo", "bar"));
        self.0.get_property_get(obj, identifier.as_deref(), info)
    }
    fn get_property_set(&self, obj: &JValue, identifier: Option<&str>, arg: &JValue, info: &Info) -> R<Option<Arc<dyn VelPropertySet>>> {
        self.0.get_property_set(obj, identifier, arg, info)
    }
}

#[test]
fn test_chaining() {
    const P: &str = "org.apache.velocity.test.util.introspection.ChainedUberspectorsTestCase$";
    register_uberspect_class(&format!("{P}ChainedUberspector"), UberspectClass::Chainable(Arc::new(|inner| Arc::new(ChainedUberspector(inner)) as Arc<dyn Uberspect>)));
    register_uberspect_class(&format!("{P}LinkedUberspector"), UberspectClass::Plain(Arc::new(|| Arc::new(LinkedUberspector::default()) as Arc<dyn Uberspect>)));
    let mut p = ExtProperties::new();
    let instance: Arc<dyn velocity::log::Logger> = Arc::new(TestLogger::default());
    p.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    p.add_property("introspector.uberspect.class", velocity::uberspector::UBERSPECT_IMPL.into());
    p.add_property("introspector.uberspect.class", format!("{P}ChainedUberspector").as_str().into());
    p.add_property("introspector.uberspect.class", format!("{P}LinkedUberspector").as_str().into());
    let ve = Engine::with_properties(&p).unwrap();
    let foo = class(&format!("{P}Foo"))
        .method("zeMethod", &[], Some(string()), |_, _| ret(s("ok")))
        .method("getBar", &[], Some(string()), |b, _| ret(b.field("bar")))
        .method("setBar", &[string()], None, |b, a| {
            b.set_field("bar", a[0].clone());
            Ok(None)
        })
        .build();
    let mut context = Context::new();
    context.put("foo", foo.new_instance());
    assert_eq!(ve.evaluate(&mut context, "test", "$foo.zeMethod()").unwrap(), "ok");
    ve.evaluate(&mut context, "test", "#set($foo.foo = 'someValue')").unwrap();
    assert_eq!(ve.evaluate(&mut context, "test", "$foo.bar").unwrap(), "someValue");
    assert_eq!(ve.evaluate(&mut context, "test", "$foo.foo").unwrap(), "someValue");
}

// ---- DeprecatedCheckUberspectorsTestCase ----

#[test]
fn test_deprecated_check() {
    const P: &str = "org.apache.velocity.test.util.introspection.DeprecatedCheckUberspectorsTestCase$";
    let logger = Arc::new(TestLogger::new(false, true));
    let mut b = Base::new();
    b.props = ExtProperties::new();
    let instance: Arc<dyn velocity::log::Logger> = logger.clone();
    b.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    b.add_property("introspector.uberspect.class", velocity::uberspector::UBERSPECT_IMPL);
    b.add_property("introspector.uberspect.class", velocity::uberspector::DEPRECATED_CHECK_UBERSPECTOR);
    let standard = class(&format!("{P}StandardObject"))
        .method("foo", &[], Some(string()), |_, _| ret(s("foo")))
        .method("getBar", &[], Some(string()), |_, _| ret(s("bar")))
        .deprecated_method("foo")
        .deprecated_method("getBar")
        .build();
    let deprecated = class(&format!("{P}DeprecatedObject"))
        .method("foo", &[], Some(string()), |_, _| ret(s("foo")))
        .method("getBar", &[], Some(string()), |_, _| ret(s("bar")))
        .deprecated()
        .build();
    b.put("obj1", standard.new_instance());
    b.put("obj2", deprecated.new_instance());
    b.engine();
    logger.start_capture();
    let mut context = std::mem::take(&mut b.context);
    b.engine().evaluate(&mut context, "test", "$obj1.foo() $obj1.bar $obj2.foo() $obj2.bar").unwrap();
    let log = logger.get_log();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines[0], "  [info] Deprecated usage of method [org.apache.velocity.test.util.introspection.DeprecatedCheckUberspectorsTestCase.StandardObject.foo] in test@1,7");
    assert_eq!(lines[1], "  [info] Deprecated usage of getter [org.apache.velocity.test.util.introspection.DeprecatedCheckUberspectorsTestCase.StandardObject.getBar] in test@1,19");
    assert_eq!(lines[2], "  [info] Deprecated usage of method [org.apache.velocity.test.util.introspection.DeprecatedCheckUberspectorsTestCase.DeprecatedObject.foo] in test@1,29");
    assert_eq!(lines[3], "  [info] Deprecated usage of getter [org.apache.velocity.test.util.introspection.DeprecatedCheckUberspectorsTestCase.DeprecatedObject.getBar] in test@1,41");
}

// ---- SecureIntrospectionTestCase ----

fn secure_engine() -> Engine {
    let mut p = ExtProperties::new();
    p.set_property("introspector.uberspect.class", velocity::config::SECURE_UBERSPECTOR.into());
    Engine::with_properties(&p).unwrap()
}

fn secure_test_object() -> JValue {
    class("org.apache.velocity.test.SecureIntrospectionTestCase")
        .method("getProperty", &[], Some(string()), |b, _| ret(b.field("testProperty")))
        .method("aTestMethod", &[], Some(JType::Int), |_, _| ret(JValue::Int(1)))
        .method("setProperty", &[string()], None, |b, a| {
            b.set_field("testProperty", a[0].clone());
            Ok(None)
        })
        .method("getCollection", &[], Some(JType::class("java.util.Collection")), |_, _| ret(JValue::List(JList::new(ListKind::Set, vec![s("aaa"), s("bbb"), s("ccc")]))))
        .method("getClass", &[], Some(JType::class("java.lang.Class")), |b, _| ret(velocity::JClass::of(b.class.clone())))
        .build()
        .new_instance()
}

/// An evaluation is bad when the result is empty or the input itself (a bad reference).
fn does_string_evaluate(ve: &Engine, c: &mut Context, input: &str) -> bool {
    let result = ve.evaluate(c, "foo", input).unwrap();
    !result.is_empty() && result != input
}

#[test]
fn test_bad_method_calls() {
    let ve = secure_engine();
    let mut c = Context::new();
    c.put("test", secure_test_object());
    for t in ["$test.Class.Methods", "$test.Class.ClassLoader", "$test.Class.ClassLoader.loadClass('java.util.HashMap').newInstance().size()"] {
        assert!(!does_string_evaluate(&ve, &mut c, t), "Should not have evaluated: {t}");
    }
}

#[test]
fn test_good_method_calls() {
    let ve = secure_engine();
    let mut c = Context::new();
    c.put("test", secure_test_object());
    for t in ["#foreach($item in $test.collection)$item#end", "$test.Class.Name", "#set($test.Property = 'abc')$test.Property", "$test.aTestMethod()"] {
        assert!(does_string_evaluate(&ve, &mut c, t), "Should have evaluated: {t}");
    }
}

// ---- InfoTestCase ----

/// `org.apache.velocity.test.misc.UberspectTestImpl`: throws `UberspectTestException` with the `Info` when
/// `UberspectImpl` finds nothing.
#[derive(Default)]
struct UberspectTestImpl(UberspectImpl);

fn uberspect_test_exception(message: String, info: &Info) -> VelocityError {
    let mut e = VelocityError::runtime(JError::new("org.apache.velocity.test.misc.UberspectTestException", message));
    e.template = Some(info.template.clone());
    e.line = info.line;
    e.column = info.column;
    e
}

impl Uberspect for UberspectTestImpl {
    fn init(&self, rsvc: &Engine) -> R<()> {
        self.0.init(rsvc)
    }
    fn get_iterator(&self, obj: &JValue, info: &Info) -> R<Option<velocity::uberspector::Iteration>> {
        self.0.get_iterator(obj, info)
    }
    fn get_method(&self, obj: &JValue, name: &str, args: &[JValue], info: &Info) -> R<Option<Arc<dyn VelMethod>>> {
        match self.0.get_method(obj, name, args, info)? {
            Some(m) => Ok(Some(m)),
            None if obj.is_null() => Err(uberspect_test_exception(format!("Can't call method '{name}' on null object"), info)),
            None => Err(uberspect_test_exception(format!("Did not find method {}.{name}", obj.class_name()), info)),
        }
    }
    fn get_property_get(&self, obj: &JValue, identifier: Option<&str>, info: &Info) -> R<Option<Arc<dyn VelPropertyGet>>> {
        let id = identifier.unwrap_or("null");
        match self.0.get_property_get(obj, identifier, info)? {
            Some(g) => Ok(Some(g)),
            None if obj.is_null() => Err(uberspect_test_exception(format!("Can't call getter '{id}' on null object"), info)),
            None => Err(uberspect_test_exception(format!("Did not find {}.{id}", obj.class_name()), info)),
        }
    }
    fn get_property_set(&self, obj: &JValue, identifier: Option<&str>, arg: &JValue, info: &Info) -> R<Option<Arc<dyn VelPropertySet>>> {
        self.0.get_property_set(obj, identifier, arg, info)
    }
}

fn check_info(template_name: &str, expected_line: i32, expected_col: i32) {
    register_uberspect_class("org.apache.velocity.test.misc.UberspectTestImpl", UberspectClass::Plain(Arc::new(|| Arc::new(UberspectTestImpl::default()) as Arc<dyn Uberspect>)));
    let mut p = ExtProperties::new();
    p.set_property("runtime.introspector.uberspect", "org.apache.velocity.test.misc.UberspectTestImpl".into());
    p.set_property("file.resource.loader.path", resource("info").into());
    let ve = Engine::with_properties(&p).unwrap();
    let template = ve.get_template(template_name, Some("UTF-8")).unwrap();
    let mut context = Context::new();
    context.put("main", class("org.apache.velocity.test.InfoTestCase").build().new_instance());
    let e = match ve.merge(&template, &mut context) {
        Ok(_) => panic!("Uberspect should have thrown an exception"),
        Err(e) => e,
    };
    assert_eq!(e.class_name(), "org.apache.velocity.test.misc.UberspectTestException");
    assert_eq!(e.template.as_deref(), Some(template_name), "Template Name");
    assert_eq!(e.line, expected_line, "Template Line");
    assert_eq!(e.column, expected_col, "Template Column");
}

#[test]
fn test_info_property() {
    check_info("info1.vm", 1, 7);
}

#[test]
fn test_info_method() {
    check_info("info2.vm", 1, 7);
}
