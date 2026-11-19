//! `EventHandlingTestCase`, `FilteredEventHandlingTestCase`, `IncludeEventHandlingTestCase`,
//! `InvalidEventHandlerTestCase`.

use std::sync::Arc;

use java_compat::lang::{JError, JList, JMap, JValue, MapKind};
use java_compat::reflect::JType;
use parking_lot::Mutex;
use velocity::events::{
    EventCartridge, Handler, IncludeEventHandler, Info, InvalidReferenceEventHandler, MethodExceptionEventHandler, ReferenceInsertionEventHandler, register_handler_class,
};
use velocity::props::{ExtProperties, PropValue};
use velocity::{Context, Engine, VelocityError};

use crate::base::*;
use crate::test_logger::TestLogger;

type R<T> = Result<T, VelocityError>;

const NO_REFERENCE_VALUE: &str = "<no reference value>";
const REFERENCE_VALUE: &str = "<reference value>";

fn runtime_exception(cause: &str) -> VelocityError {
    VelocityError::runtime(JError::new("java.lang.RuntimeException", cause))
}

/// `EventHandlingTestCase.TestEventCartridge`.
struct TestEventCartridge;

impl ReferenceInsertionEventHandler for TestEventCartridge {
    fn reference_insert(&self, _: &Engine, _: &Context, reference: &str, value: JValue) -> R<JValue> {
        Ok(if !value.is_null() {
            s(REFERENCE_VALUE)
        } else if reference == "$floobie" {
            s(NO_REFERENCE_VALUE)
        } else {
            JValue::Null
        })
    }
}

impl MethodExceptionEventHandler for TestEventCartridge {
    fn method_exception(&self, _: &Engine, context: &Context, _: &str, method: &str, e: &JError, _: &Info) -> Result<JValue, Option<VelocityError>> {
        if context.contains_key("allow_exception") && method == "throwException" {
            return Ok(s("handler"));
        }
        Err(Some(runtime_exception(&e.to_string())))
    }
}

const TEST_EVENT_CARTRIDGE: &str = "org.apache.velocity.test.EventHandlingTestCase$TestEventCartridge";

fn test_event_cartridge() -> Handler {
    let h = Arc::new(TestEventCartridge);
    Handler { reference_insertion: Some(h.clone()), method_exception: Some(h), ..Default::default() }
}

/// The test case itself, with its `throwException()`.
fn this() -> JValue {
    class("org.apache.velocity.test.EventHandlingTestCase")
        .method("throwException", &[], None, |_, _| Err(JError::new("java.lang.Exception", "Hello from throwException()")))
        .build()
        .new_instance()
}

fn do_test_event_handlers(b: &mut Base) {
    let outer = b.context.clone();
    // Reference insertion.
    b.context = Context::wrap(&outer);
    b.put("name", s("Velocity"));
    b.assert_eval_equals(&REFERENCE_VALUE.repeat(3), "$name$name$name");
    // Null references too.
    b.context = Context::wrap(&outer);
    b.put("name", s("Velocity"));
    b.assert_eval_equals(NO_REFERENCE_VALUE, "$floobie");
    // A method exception the handler takes.
    b.context = Context::wrap(&outer);
    b.put("allow_exception", JValue::Bool(true));
    b.put("this", this());
    b.evaluate(" $this.throwException()").expect("the handler takes the exception");
    // Without the switch it propagates.
    b.context = Context::wrap(&outer);
    b.put("this", this());
    b.assert_eval_exception("$this.throwException()", Some("org.apache.velocity.exception.MethodInvocationException"));
    b.context = outer;
}

#[test]
fn test_manual_event_handlers() {
    let mut b = Base::new();
    let ec = EventCartridge::new();
    ec.add(test_event_cartridge());
    b.context.attach_event_cartridge(ec);
    do_test_event_handlers(&mut b);
}

#[test]
fn test_configuration_event_handlers() {
    register_handler_class(TEST_EVENT_CARTRIDGE, Arc::new(test_event_cartridge));
    let mut b = Base::new();
    b.set_property("event_handler.method_exception.class", TEST_EVENT_CARTRIDGE);
    b.set_property("event_handler.reference_insertion.class", TEST_EVENT_CARTRIDGE);
    do_test_event_handlers(&mut b);
}

/// `org.apache.velocity.test.eventhandler.Handler1`.
struct Handler1;

impl ReferenceInsertionEventHandler for Handler1 {
    fn reference_insert(&self, _: &Engine, _: &Context, _: &str, value: JValue) -> R<JValue> {
        Ok(if value.is_null() {
            JValue::Null
        } else {
            let v = value.to_java_string();
            s(&format!("{}{}", v.to_uppercase(), v.to_lowercase()))
        })
    }
}

impl MethodExceptionEventHandler for Handler1 {
    fn method_exception(&self, _: &Engine, _: &Context, _: &str, _: &str, e: &JError, _: &Info) -> Result<JValue, Option<VelocityError>> {
        Err(Some(runtime_exception(&e.to_string())))
    }
}

impl IncludeEventHandler for Handler1 {
    fn include_event(&self, _: &Engine, _: &mut Context, _: Option<&str>, _: Option<&str>, _: &str) -> R<Option<String>> {
        Ok(Some("notfound.vm".into()))
    }
}

/// `org.apache.velocity.test.eventhandler.Handler2`.
struct Handler2;

impl ReferenceInsertionEventHandler for Handler2 {
    fn reference_insert(&self, _: &Engine, _: &Context, _: &str, value: JValue) -> R<JValue> {
        Ok(if value.is_null() { JValue::Null } else { s(&value.to_java_string().to_uppercase()) })
    }
}

impl MethodExceptionEventHandler for Handler2 {
    fn method_exception(&self, _: &Engine, _: &Context, _: &str, _: &str, e: &JError, _: &Info) -> Result<JValue, Option<VelocityError>> {
        Ok(s(&format!("Exception: {e}")))
    }
}

impl IncludeEventHandler for Handler2 {
    fn include_event(&self, _: &Engine, _: &mut Context, path: Option<&str>, _: Option<&str>, _: &str) -> R<Option<String>> {
        Ok(Some(format!("subdir/{}", path.unwrap_or("null"))))
    }
}

fn register_handlers() {
    register_handler_class(
        "org.apache.velocity.test.eventhandler.Handler1",
        Arc::new(|| {
            let h = Arc::new(Handler1);
            Handler { reference_insertion: Some(h.clone()), method_exception: Some(h.clone()), include: Some(h), ..Default::default() }
        }),
    );
    register_handler_class(
        "org.apache.velocity.test.eventhandler.Handler2",
        Arc::new(|| {
            let h = Arc::new(Handler2);
            Handler { reference_insertion: Some(h.clone()), method_exception: Some(h.clone()), include: Some(h), ..Default::default() }
        }),
    );
}

fn filtered_engine(logger: &Arc<TestLogger>, sequence: &str) -> Engine {
    let mut p = ExtProperties::new();
    let instance: Arc<dyn velocity::log::Logger> = logger.clone();
    p.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    p.set_property("event_handler.method_exception.class", sequence.into());
    p.set_property("event_handler.reference_insertion.class", sequence.into());
    p.set_property("event_handler.include.class", sequence.into());
    p.add_property("resource.loader.file.path", resource("includeevent").into());
    Engine::with_properties(&p).expect("initialises")
}

#[test]
fn test_filtered_event_handling() {
    register_handlers();
    let handler1 = "org.apache.velocity.test.eventhandler.Handler1";
    let handler2 = "org.apache.velocity.test.eventhandler.Handler2";
    let logger = Arc::new(TestLogger::new(false, false));
    let ve = filtered_engine(&logger, &format!("{handler1},{handler2}"));
    let ve2 = filtered_engine(&logger, &format!("{handler2},{handler1}"));

    let mut context = Context::new();
    context.put("test", s("abc"));
    assert_eq!(ve.evaluate(&mut context, "test", "$test").unwrap(), "ABCABC", "Reference insertion test 1");
    let mut context = Context::new();
    context.put("test", s("abc"));
    assert_eq!(ve2.evaluate(&mut context, "test", "$test").unwrap(), "ABCabc", "Reference insertion test 2");

    let mut context = Context::new();
    context.put("test", JValue::List(JList::array_list(vec![])));
    match ve.evaluate(&mut context, "test", "$test.get(0)") {
        Err(e) if e.is_a("org.apache.velocity.exception.MethodInvocationException") => {}
        other => panic!("Method exception event test 1: {:?}", other.map_err(|e| e.class_name())),
    }
    let mut context = Context::new();
    context.put("test", JValue::List(JList::array_list(vec![])));
    ve2.evaluate(&mut context, "test", "$test.get(0)").unwrap();

    for engine in [&ve, &ve2] {
        logger.start_capture();
        engine.evaluate(&mut Context::new(), "test", "#set($test1 = $test2)").unwrap();
        assert_eq!(logger.get_log(), "", "log null set test");
    }
    logger.stop_capture();

    for (engine, name) in [(&ve, "test4"), (&ve2, "test5")] {
        let template = engine.get_template(&format!("{name}.vm"), None).unwrap();
        let out = engine.merge(&template, &mut Context::new()).unwrap();
        assert_eq!(normalize_newlines(&out), normalize_newlines(&file_contents(&resource(&format!("includeevent/compare/{name}.cmp")))), "Output incorrect.");
    }
}

/// `IncludeEventHandlingTestCase` itself, as the include handler.
#[derive(Default)]
struct IncludeBehavior(Mutex<u8>);

const PASS_THROUGH: u8 = 0;
const RELATIVE_PATH: u8 = 1;
const BLOCK: u8 = 2;

impl IncludeEventHandler for IncludeBehavior {
    fn include_event(&self, _: &Engine, _: &mut Context, include: Option<&str>, current: Option<&str>, _: &str) -> R<Option<String>> {
        let include = include.unwrap_or_default();
        Ok(match *self.0.lock() {
            PASS_THROUGH => Some(include.to_string()),
            RELATIVE_PATH => {
                if include.starts_with('/') || include.starts_with('\\') {
                    return Ok(Some(include.to_string()));
                }
                let current = current.unwrap_or_default();
                match current.rfind(['/', '\\']) {
                    None => Some(include.to_string()),
                    Some(i) => Some(format!("{}/{include}", &current[..i])),
                }
            }
            _ => None,
        })
    }
}

#[test]
fn test_include_event_handling() {
    let mut p = ExtProperties::new();
    p.add_property("resource.loader.file.path", resource("includeevent").into());
    let instance: Arc<dyn velocity::log::Logger> = Arc::new(TestLogger::default());
    p.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    let engine = Engine::with_properties(&p).unwrap();
    let template1 = engine.get_template("test1.vm", None).unwrap();
    let template2 = engine.get_template("subdir/test2.vm", None).unwrap();
    let template3 = engine.get_template("test3.vm", None).unwrap();
    let mut context = Context::new();
    let behavior = Arc::new(IncludeBehavior::default());
    let ec = EventCartridge::new();
    ec.add(Handler { include: Some(behavior.clone()), ..Default::default() });
    context.attach_event_cartridge(ec);
    let mut outputs = Vec::new();
    for (mode, template) in [(PASS_THROUGH, &template1), (RELATIVE_PATH, &template2), (BLOCK, &template3)] {
        *behavior.0.lock() = mode;
        outputs.push(engine.merge(template, &mut context).unwrap());
    }
    for (name, out) in ["test1", "test2", "test3"].iter().zip(outputs) {
        assert_eq!(normalize_newlines(&out), normalize_newlines(&file_contents(&resource(&format!("includeevent/compare/{name}.cmp")))), "Output incorrect for {name}");
    }
}

/// `InvalidEventHandlerTestCase.TestEventCartridge`.
struct InvalidReferences;

impl InvalidReferenceEventHandler for InvalidReferences {
    fn invalid_get_method(&self, _: &Engine, _: &Context, reference: &str, object: &JValue, property: Option<&str>, _: &Info) -> R<JValue> {
        match reference {
            "$a1.foobar" => {
                assert!(matches!(object, JValue::Int(5)));
                assert_eq!(property, Some("foobar"));
                Err(runtime_exception("expected exception"))
            }
            "$a2" | "$a3" | "$zz" => {
                assert!(object.is_null());
                assert_eq!(property, None);
                Err(runtime_exception("expected exception"))
            }
            "$a4.foobar" => {
                assert!(matches!(object, JValue::Int(5)));
                assert_eq!(property, Some("foobar"));
                Ok(s("zzz"))
            }
            "$q1" => Ok(JValue::Null),
            "$tree.x" | "$tree.field.x" => {
                assert_eq!(property, Some("x"));
                Ok(JValue::Null)
            }
            "$tree.child.y" | "$tree.child.Field.y" => {
                assert_eq!(property, Some("y"));
                Ok(JValue::Null)
            }
            other => panic!("invalidGetMethod: unexpected reference: {other}"),
        }
    }

    fn invalid_method(&self, _: &Engine, _: &Context, reference: &str, object: &JValue, method: &str, _: &Info) -> R<JValue> {
        match object {
            JValue::Int(_) => {
                assert_eq!(reference, "$a1.afternoon()");
                assert_eq!(method, "afternoon");
                Err(runtime_exception("expected exception"))
            }
            JValue::Str(_) if method == "baby" => Ok(s("www")),
            _ => panic!("Unexpected invalid method.  {method}"),
        }
    }

    fn invalid_set_method(&self, _: &Engine, _: &Context, left: &str, right: &str, _: &Info) -> R<bool> {
        match left {
            "xx" => {
                assert_eq!(right, "q1.afternoon()");
                Err(runtime_exception("expected exception"))
            }
            "yy" => {
                assert_eq!(right, "$q1");
                Err(runtime_exception("expected exception"))
            }
            other => panic!("Unexpected left hand side.  {other}"),
        }
    }
}

const INVALID_REFERENCES: &str = "org.apache.velocity.test.InvalidEventHandlerTestCase$TestEventCartridge";

fn invalid_references() -> Handler {
    Handler { invalid_reference: Some(Arc::new(InvalidReferences)), ..Default::default() }
}

fn eval(ve: &Engine, context: &mut Context, s: &str) -> Result<String, VelocityError> {
    ve.evaluate(context, "mystring", s)
}

#[track_caller]
fn assert_fails(ve: &Engine, context: &mut Context, s: &str) {
    assert!(eval(ve, context, s).is_err(), "Expected exception for {s}");
}

fn test_object() -> JValue {
    class("org.apache.velocity.test.InvalidEventHandlerTestCase$TestObject")
        .method("getNullValueAttribute", &[], Some(string()), |_, _| ret(JValue::Null))
        .method("getRealString", &[], Some(string()), |_, _| ret(s("helloFooRealStr")))
        .method("getString", &[], Some(string()), |_, _| ret(s("helloFoo")))
        .method("getNullString", &[], Some(string()), |_, _| ret(JValue::Null))
        .method("getNullDate", &[], Some(JType::class("java.util.Date")), |_, _| ret(JValue::Null))
        .method("toString", &[], Some(string()), |_, _| ret(s("TestObject [nullValueAttribute=null]")))
        .build()
        .new_instance()
}

fn handler0(ve: &Engine, vc: &Context) {
    let mut context = Context::wrap(vc);
    context.put("propertyAccess", s("lorem ipsum"));
    context.put("objRef", test_object());
    context.put("objRefArrayList", JValue::List(JList::array_list(vec![s("firstOne"), JValue::Null])));
    let map = JMap::new(MapKind::Hash);
    map.put(JValue::Int(41), s("41 is not 42"));
    context.put("objRefHashMap", JValue::Map(map));
    for t in [
        "#set($resultVar = $propertyAccess.bytes)",
        "#set($resultVar = $propertyAccess.getBytes())",
        "$objRef.getRealString()",
        "$objRef.getNullValueAttribute()",
        "$objRef.nullValueAttribute",
        "$objRefHashMap.remove(41)",
        "$objRefHashMap.remove(42)",
        "$objRefArrayList.get(0)",
        "$objRefArrayList.get(1)",
    ] {
        eval(ve, &mut context, t).unwrap_or_else(|e| panic!("{t}: {}", e.message()));
    }
}

fn numbers(vc: &Context) -> Context {
    let mut context = Context::wrap(vc);
    context.put("a1", JValue::Int(5));
    context.put("a4", JValue::Int(5));
    context.put("b1", s("abc"));
    context
}

fn handler1(ve: &Engine, vc: &Context) {
    let mut context = numbers(vc);
    eval(ve, &mut context, "$a1 $a1.intValue() $b1 $b1.length() #set($c1 = '5')").unwrap();
    assert_fails(ve, &mut context, "$a1.foobar");
    assert_eq!(eval(ve, &mut context, "$!a1.foobar").unwrap(), "");
    assert_eq!(eval(ve, &mut context, "#if($a1.foobar)yes#{else}no#end").unwrap(), "no");
    assert_fails(ve, &mut context, "$a2.foobar");
    assert_eq!(eval(ve, &mut context, "$!a2.foobar").unwrap(), "");
    assert_fails(ve, &mut context, "#if($a2.foobar)yes#{else}no#end");
    assert_eq!(eval(ve, &mut context, "#if($a2 and $a2.foobar)yes#{else}no#end").unwrap(), "no");
    assert_fails(ve, &mut context, "$a3");
    assert_eq!(eval(ve, &mut context, "$!a3").unwrap(), "");
    assert_eq!(eval(ve, &mut context, "#if($a3)yes#{else}no#end").unwrap(), "no");
    assert_eq!(eval(ve, &mut context, "$a4.foobar").unwrap(), "zzz");
}

fn handler2(ve: &Engine, vc: &Context) {
    let mut context = numbers(vc);
    assert_fails(ve, &mut context, "$a1.afternoon()");
    assert_eq!(eval(ve, &mut context, "$!a1.afternoon()").unwrap(), "");
    assert_fails(ve, &mut context, "$zz.daylight()");
    assert_eq!(eval(ve, &mut context, "$!zz.daylight()").unwrap(), "");
    assert_eq!(eval(ve, &mut context, "$b1.baby()").unwrap(), "www");
}

fn handler3(ve: &Engine, vc: &Context) {
    let mut context = numbers(vc);
    assert_fails(ve, &mut context, "#set($xx = $a1.afternoon())");
    assert_fails(ve, &mut context, "#set($yy = $q1)");
}

fn tree(field: &str, child: Option<JValue>) -> JValue {
    static TREE: std::sync::LazyLock<BeanFactory> = std::sync::LazyLock::new(|| {
        let tree = || Some(JType::class("org.apache.velocity.test.InvalidEventHandlerTestCase$Tree"));
        class("org.apache.velocity.test.InvalidEventHandlerTestCase$Tree")
            .method("getField", &[], Some(string()), |b, _| ret(b.field("field")))
            .method("setField", &[string()], None, |b, a| {
                b.set_field("field", a[0].clone());
                Ok(None)
            })
            .method("getChild", &[], tree(), |b, _| ret(b.field("child")))
            .method("setChild", &[JType::class("org.apache.velocity.test.InvalidEventHandlerTestCase$Tree")], None, |b, a| {
                b.set_field("child", a[0].clone());
                Ok(None)
            })
            .method("testMethod", &[], Some(string()), |_, _| ret(s("123")))
            .build()
    });
    TREE.with_fields(&[("field", s(field)), ("child", child.unwrap_or(JValue::Null))])
}

fn handler4(ve: &Engine, vc: &Context) {
    let mut context = Context::wrap(vc);
    context.put("tree", tree("10", Some(tree("12", None))));
    eval(ve, &mut context, "$tree.Field $tree.field $tree.child.Field").unwrap();
    eval(ve, &mut context, "$tree.x $tree.field.x $tree.child.y $tree.child.Field.y").unwrap();
}

fn run_invalid_handlers(ve: &Engine, vc: &Context) {
    handler0(ve, vc);
    handler1(ve, vc);
    handler2(ve, vc);
    handler3(ve, vc);
    handler4(ve, vc);
}

#[test]
fn test_invalid_manual_event_handlers() {
    let ve = Engine::with_properties(&ExtProperties::new()).unwrap();
    let mut inner = Context::new();
    let ec = EventCartridge::new();
    ec.add(invalid_references());
    inner.attach_event_cartridge(ec);
    run_invalid_handlers(&ve, &inner);
}

#[test]
fn test_invalid_configuration_event_handlers() {
    register_handler_class(INVALID_REFERENCES, Arc::new(invalid_references));
    let mut p = ExtProperties::new();
    p.set_property("event_handler.invalid_references.class", INVALID_REFERENCES.into());
    let ve = Engine::with_properties(&p).unwrap();
    run_invalid_handlers(&ve, &Context::new());
}
