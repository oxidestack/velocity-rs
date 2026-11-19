//! `ConversionHandlerTestCase`.

use std::sync::{Arc, LazyLock};

use java_compat::lang::{JArray, JError, JList, JMap, JValue, MapKind};
use java_compat::reflect::{EnumConstant, JType};
use velocity::events::{Handler, Info, MethodExceptionEventHandler, register_handler_class};
use velocity::props::{ExtProperties, PropValue};
use velocity::uberspect::{Conv, ConverterFn, TypeConversionHandler, TypeConversionHandlerImpl};
use velocity::{Context, Engine, JClass, VelocityError};

use crate::base::*;
use crate::test_logger::{LOG_LEVEL_ERROR, TestLogger};

const OBJ: &str = "org.apache.velocity.test.util.introspection.ConversionHandlerTestCase$Obj";

fn color() -> Arc<java_compat::reflect::ClassDef> {
    static COLOR: LazyLock<BeanFactory> = LazyLock::new(|| class(&format!("{OBJ}$Color")).enum_of(&["RED", "GREEN"]).build());
    COLOR.class.clone()
}

/// `ConversionHandlerTestCase.Obj`.
fn obj() -> JValue {
    static OBJ_CLASS: LazyLock<BeanFactory> = LazyLock::new(|| {
        let ok = |label: &'static str| move |_: &Bean, a: &[JValue]| ret(s(&format!("{label} ok: {}", a[0].to_java_string())));
        let c = |n: &str| JType::class(n);
        let list_check = |expected: [JValue; 3]| {
            move |_: &Bean, a: &[JValue]| {
                let good = match &a[0] {
                    JValue::List(l) => l.len() == 3 && l.items().iter().zip(expected.iter()).all(|(x, y)| java_compat::lang::java_equals(x, y)),
                    _ => false,
                };
                ret(s(if good { "correct" } else { "wrong" }))
            }
        };
        color();
        class(OBJ)
            .method("integralBoolean", &[JType::Boolean], Some(string()), ok("boolean"))
            .method("integralByte", &[JType::Byte], Some(string()), ok("byte"))
            .method("integralShort", &[JType::Short], Some(string()), ok("short"))
            .method("integralInt", &[JType::Int], Some(string()), ok("int"))
            .method("integralLong", &[JType::Long], Some(string()), ok("long"))
            .method("integralFloat", &[JType::Float], Some(string()), ok("float"))
            .method("integralDouble", &[JType::Double], Some(string()), ok("double"))
            .method("integralChar", &[JType::Char], Some(string()), ok("char"))
            .method("objectBoolean", &[c("java.lang.Boolean")], Some(string()), ok("Boolean"))
            .method("objectByte", &[c("java.lang.Byte")], Some(string()), ok("Byte"))
            .method("objectShort", &[c("java.lang.Short")], Some(string()), ok("Short"))
            .method("objectInt", &[c("java.lang.Integer")], Some(string()), ok("Integer"))
            .method("objectLong", &[c("java.lang.Long")], Some(string()), ok("Long"))
            .method("objectBigInteger", &[c("java.math.BigInteger")], Some(string()), ok("BigInteger"))
            .method("objectFloat", &[c("java.lang.Float")], Some(string()), ok("Float"))
            .method("objectDouble", &[c("java.lang.Double")], Some(string()), ok("Double"))
            .method("objectBigDecimal", &[c("java.math.BigDecimal")], Some(string()), ok("BigDecimal"))
            .method("objectCharacter", &[c("java.lang.Character")], Some(string()), ok("Character"))
            .method("objectNumber", &[c("java.lang.Number")], Some(string()), ok("Number"))
            .method("objectObject", &[object()], Some(string()), ok("Object"))
            .method("objectString", &[string()], Some(string()), ok("String"))
            .method("objectEnum", &[c(&format!("{OBJ}$Color"))], Some(string()), ok("Enum"))
            .method("locale", &[c("java.util.Locale")], Some(string()), ok("Locale"))
            .method("toString", &[], Some(string()), |_, _| ret(s("instance of Obj")))
            // Generic parameters are keyed by their type names (`Type.getTypeName()`).
            .method("iWantAStringList", &[c("java.util.List<java.lang.String>")], Some(string()), list_check([s("a"), s("b"), s("c")]))
            .method("iWantAnIntegerList", &[c("java.util.List<java.lang.Integer>")], Some(string()), list_check([JValue::Int(1), JValue::Int(2), JValue::Int(3)]))
            .build()
    });
    OBJ_CLASS.new_instance()
}

/// `ConversionHandlerTestCase.Introspect`.
fn introspect() -> JValue {
    fn jtype(v: &JValue) -> Option<JType> {
        match v {
            JValue::Object(o) => o.as_any().downcast_ref::<JClass>().and_then(|c| c.ty.clone()),
            _ => None,
        }
    }
    let class_type = || JType::class("java.lang.Class");
    class("org.apache.velocity.test.util.introspection.ConversionHandlerTestCase$Introspect")
        .method("isStrictlyConvertible", &[class_type(), class_type()], Some(JType::Boolean), |_, a| {
            ret(JValue::Bool(velocity::uberspect::is_strict_convertible(&jtype(&a[0]).unwrap(), jtype(&a[1]).as_ref(), false)))
        })
        .method("isImplicitlyConvertible", &[class_type(), class_type()], Some(JType::Boolean), |_, a| {
            ret(JValue::Bool(velocity::uberspect::is_convertible(&jtype(&a[0]).unwrap(), jtype(&a[1]).as_ref(), false)))
        })
        .method("isExplicitlyConvertible", &[class_type(), class_type()], Some(JType::Boolean), |_, a| {
            ret(JValue::Bool(TypeConversionHandlerImpl::default().is_explicitly_convertible(&jtype(&a[0]).unwrap(), jtype(&a[1]).as_ref(), false)))
        })
        .build()
        .new_instance()
}

/// commons-lang3 `StringUtils`' `join` overloads.
fn strings() -> JValue {
    let join = |items: Vec<JValue>, sep: String| s(&items.iter().map(|v| if v.is_null() { String::new() } else { v.to_java_string() }).collect::<Vec<_>>().join(&sep));
    let items = |v: &JValue| match v {
        JValue::List(l) => l.items(),
        JValue::Array(a) => a.items(),
        _ => Vec::new(),
    };
    class("org.apache.commons.lang3.StringUtils")
        .method("join", &[JType::class("java.lang.Iterable"), string()], Some(string()), move |_, a| ret(join(items(&a[0]), a[1].to_java_string())))
        .method("join", &[JType::class("java.lang.Iterable"), JType::Char], Some(string()), move |_, a| ret(join(items(&a[0]), a[1].to_java_string())))
        .method("join", &[JType::array(object()), string()], Some(string()), move |_, a| ret(join(items(&a[0]), a[1].to_java_string())))
        .method("join", &[JType::array(object()), JType::Char], Some(string()), move |_, a| ret(join(items(&a[0]), a[1].to_java_string())))
        .method("join", &[JType::array(object())], Some(string()), move |_, a| ret(join(items(&a[0]), String::new())))
        .build()
        .new_instance()
}

fn create_context() -> Context {
    let mut context = Context::new();
    // A `TreeMap`: keys in order.
    let mut entries: Vec<(&str, JValue)> = vec![
        ("A. bool-true", JValue::Bool(true)),
        ("B. bool-false", JValue::Bool(false)),
        ("C. byte-0", byte(0)),
        ("D. byte-1", byte(1)),
        ("E. short", short(125)),
        ("F. int", JValue::Int(24323)),
        ("G. long", JValue::Long(5235235)),
        ("H. float", JValue::Float(34523.345)),
        ("I. double", JValue::Double(54235.3253)),
        ("J. char", JValue::Char('@')),
        ("K. object", obj()),
        ("L. enum", EnumConstant::value_of(&color(), "GREEN").unwrap()),
        ("M. string", s("foo")),
        ("M. string-green", s("green")),
        ("N. string-empty", s("")),
        ("O. string-false", s("false")),
        ("P. string-true", s("true")),
        ("Q. string-zero", s("0")),
        ("R. string-integral", s("123")),
        ("S. string-big-integral", s("12345678")),
        ("T. string-floating", s("123.345")),
        ("U. null", JValue::Null),
        ("V. locale", s("fr_FR")),
        ("W. BigInteger zero", big_integer(0)),
        ("X. BigInteger one", big_integer(1)),
        ("Y. BigInteger ten", big_integer(10)),
        ("Y. BigInteger bigint", JValue::BigInteger("12345678901234567890".into())),
        ("Z. BigDecimal zero", big_decimal("0")),
        ("ZA. BigDecimal one", big_decimal("1")),
        ("ZB. BigDecimal ten", big_decimal("10")),
        ("ZC. BigDecimal bigdec", big_decimal("12345678901234567890.01234567890123456789")),
    ];
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let map = JMap::new(MapKind::Linked);
    for (k, v) in entries {
        map.put(s(k), v);
    }
    context.put("map", JValue::Map(map));
    context.put("target", obj());
    let c = |n: &str| JType::class(n);
    let types = [
        JType::Boolean,
        JType::Char,
        JType::Byte,
        JType::Short,
        JType::Int,
        JType::Long,
        JType::Float,
        JType::Double,
        c("java.lang.Boolean"),
        c("java.lang.Character"),
        c("java.lang.Byte"),
        c("java.lang.Short"),
        c("java.lang.Integer"),
        c("java.lang.Long"),
        c("java.math.BigInteger"),
        c("java.lang.Float"),
        c("java.lang.Double"),
        c("java.math.BigDecimal"),
        c("java.lang.Number"),
        c("java.lang.String"),
        c("java.lang.Object"),
    ];
    context.put("types", JValue::Array(Arc::new(JArray::new(c("java.lang.Class"), types.into_iter().map(JClass::of_type).collect()))));
    context.put("introspect", introspect());
    context.put("strings", strings());
    context
}

/// `ConversionHandlerTestCase.PrintException`.
struct PrintException;

impl MethodExceptionEventHandler for PrintException {
    fn method_exception(&self, _: &Engine, _: &Context, _: &str, method: &str, e: &JError, _: &Info) -> Result<JValue, Option<VelocityError>> {
        let simple = e.class.rsplit(['.', '$']).next().unwrap_or_default();
        // The JDK 8 message for decimal parse failures.
        if e.class == "java.lang.NumberFormatException" && e.message.as_deref().is_some_and(|m| m.starts_with("Character")) {
            return Ok(s(&format!("{method} -> {simple}: null")));
        }
        Ok(s(&format!("{method} -> {simple}: {}", e.message.as_deref().unwrap_or("null"))))
    }
}

fn create_engine(with_conversions_handler: bool, log: &Arc<TestLogger>) -> Engine {
    const PRINT_EXCEPTION: &str = "org.apache.velocity.test.util.introspection.ConversionHandlerTestCase$PrintException";
    register_handler_class(PRINT_EXCEPTION, Arc::new(|| Handler { method_exception: Some(Arc::new(PrintException)), ..Default::default() }));
    let mut p = ExtProperties::new();
    p.set_property("velocimacro.inline.local_scope", true.into());
    let instance: Arc<dyn velocity::log::Logger> = log.clone();
    p.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    p.set_property("resource.loaders", "file".into());
    p.set_property("resource.loader.file.path", resource("conversion").into());
    if with_conversions_handler {
        p.set_property("event_handler.method_exception.class", PRINT_EXCEPTION.into());
    } else {
        p.set_property("introspector.conversion_handler.class", "none".into());
    }
    Engine::with_properties(&p).unwrap()
}

fn test_conversions(ve: &Engine, template: &str, output: &str, log: &TestLogger) {
    let mut context = create_context();
    log.set_enabled_level(LOG_LEVEL_ERROR);
    let t = ve.get_template(template, None).unwrap();
    let out = ve.merge(&t, &mut context).unwrap();
    let compare = file_contents(&resource(&format!("conversion/compare/{output}.cmp")));
    if let Ok(dir) = std::env::var("VELOCITY_TEST_DUMP") {
        let _ = std::fs::write(format!("{dir}/{output}.res"), &out);
    }
    assert!(normalize_newlines(&out) == normalize_newlines(&compare), "Processed template {output} did not match expected output");
}

#[test]
fn test_conversions_without_handler() {
    let log = Arc::new(TestLogger::new(false, false));
    test_conversions(&create_engine(false, &log), "test_conv.vtl", "test_conv_without_handler", &log);
}

#[test]
fn test_conversions_with_handler() {
    let log = Arc::new(TestLogger::new(false, false));
    test_conversions(&create_engine(true, &log), "test_conv.vtl", "test_conv_with_handler", &log);
}

#[test]
fn test_conversion_matrix() {
    let log = Arc::new(TestLogger::new(false, false));
    test_conversions(&create_engine(true, &log), "matrix.vhtml", "matrix", &log);
}

fn runtime_instance(extra: impl FnOnce(&mut ExtProperties)) -> Engine {
    let mut p = ExtProperties::new();
    p.set_property("velocimacro.inline.local_scope", true.into());
    let instance: Arc<dyn velocity::log::Logger> = Arc::new(TestLogger::new(false, false));
    p.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    p.set_property("resource.loaders", "file".into());
    p.set_property("resource.loader.file.path", resource("conversion").into());
    extra(&mut p);
    Engine::with_properties(&p).unwrap()
}

#[test]
fn test_custom_converter() {
    let ve = runtime_instance(|_| {});
    let ch = ve.conversion_handler().expect("a conversion handler");
    assert!(ch.as_any().is::<TypeConversionHandlerImpl>());
    let converter = |v: JValue| -> ConverterFn { Arc::new(move |_| Ok(v.clone())) };
    ch.add_converter(&JType::class("java.lang.Float"), &JType::class(OBJ), converter(JValue::Float(4.5))).unwrap();
    ch.add_converter(&JType::class("java.util.List<java.lang.Integer>"), &JType::string(), converter(JValue::List(JList::array_list(vec![JValue::Int(1), JValue::Int(2), JValue::Int(3)]))))
        .unwrap();
    ch.add_converter(&JType::class("java.util.List<java.lang.String>"), &JType::string(), converter(JValue::List(JList::array_list(vec![s("a"), s("b"), s("c")])))).unwrap();
    let mut context = Context::new();
    context.put("obj", obj());
    assert_eq!(ve.evaluate(&mut context, "test", "$obj.integralFloat($obj) / $obj.objectFloat($obj)").unwrap(), "float ok: 4.5 / Float ok: 4.5");
    assert_eq!(ve.evaluate(&mut context, "test", "$obj.iWantAStringList('anything')").unwrap(), "correct");
    assert_eq!(ve.evaluate(&mut context, "test", "$obj.iWantAnIntegerList('anything')").unwrap(), "correct");
}

/// `ConversionHandlerTestCase.MyCustomConverter`: everything converts, to `"foo"`.
struct MyCustomConverter;

impl TypeConversionHandler for MyCustomConverter {
    fn is_explicitly_convertible(&self, _: &JType, _: Option<&JType>, _: bool) -> bool {
        true
    }

    fn needed_converter(&self, _: &JType, _: Option<&JType>) -> Option<Conv> {
        Some(Conv::Custom(Arc::new(|_| Ok(s("foo")))))
    }

    fn add_converter(&self, _: &JType, _: &JType, _: ConverterFn) -> Result<(), JError> {
        Err(JError::new("java.lang.RuntimeException", "not implemented"))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn test_custom_conversion_handler_instance() {
    let ve = runtime_instance(|p| {
        let handler: Arc<dyn TypeConversionHandler> = Arc::new(MyCustomConverter);
        p.set_property("introspector.conversion_handler.instance", PropValue::Object(Arc::new(handler)));
    });
    let ch = ve.conversion_handler().expect("a conversion handler");
    assert!(ch.as_any().is::<MyCustomConverter>());
    let mut context = Context::new();
    context.put("obj", obj());
    assert_eq!(ve.evaluate(&mut context, "test", "$obj.objectString(1.0)").unwrap(), "String ok: foo");
}

#[test]
fn test_other_conversions() {
    let log = Arc::new(TestLogger::new(false, false));
    let ve = create_engine(false, &log);
    let mut context = create_context();
    assert_eq!(ve.evaluate(&mut context, "test", "$strings.join(['foo', 'bar'], ',')").unwrap(), "foo,bar");
}
