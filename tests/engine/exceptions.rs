//! `IncludeErrorTestCase`, `ParseExceptionTestCase`, `ParserTestCase`, `ExceptionTestCase`,
//! `MethodInvocationExceptionTestCase`, `WrappedExceptionTestCase`.

use std::sync::Arc;

use java_compat::lang::{JError, JValue};
use velocity::directive::{Directive, DirectiveCall, DirectiveType, register_directive_class};
use velocity::events::{Handler, IncludeEventHandler, Info, MethodExceptionEventHandler, ReferenceInsertionEventHandler, register_handler_class};
use velocity::props::{ExtProperties, PropValue};
use velocity::resource::{LoaderSettings, ResourceLoader, register_loader_class};
use velocity::{Context, Engine, VelocityError};

use crate::base::*;
use crate::provider::test_provider;
use crate::test_logger::TestLogger;

const PARSE_ERROR: &str = "org.apache.velocity.exception.ParseErrorException";
const RESOURCE_NOT_FOUND: &str = "org.apache.velocity.exception.ResourceNotFoundException";
const METHOD_INVOCATION: &str = "org.apache.velocity.exception.MethodInvocationException";

fn engine(properties: &[(&str, &str)]) -> Engine {
    let mut p = ExtProperties::new();
    for (k, v) in properties {
        p.set_property(k, (*v).into());
    }
    Engine::with_properties(&p).expect("initialises")
}

fn check_exception(template: &str, class: &str) {
    let ve = engine(&[("file.resource.loader.path", &resource("includeerror"))]);
    let t = ve.get_template(template, Some("UTF-8")).unwrap();
    match ve.merge(&t, &mut Context::new()) {
        Ok(_) => panic!("File should have thrown an exception"),
        Err(e) => assert!(e.is_a(class), "{template}: {} is not a {class}", e.class_name()),
    }
}

#[test]
fn test_missing_parse_error() {
    check_exception("missingparse.vm", RESOURCE_NOT_FOUND);
}

#[test]
fn test_missing_include_error() {
    check_exception("missinginclude.vm", RESOURCE_NOT_FOUND);
}

#[test]
fn test_parse_error() {
    check_exception("parsemain.vm", PARSE_ERROR);
}

#[test]
fn test_parse_error2() {
    check_exception("parsemain2.vm", PARSE_ERROR);
}

#[test]
fn test_parse_exception_from_template() {
    let ve = engine(&[("file.resource.loader.cache", "true"), ("file.resource.loader.path", &resource("parseexception"))]);
    let e = match ve.get_template("badtemplate.vm", None).and_then(|t| ve.merge(&t, &mut Context::new())) {
        Ok(_) => panic!("Should have thown a ParseErrorException"),
        Err(e) => e,
    };
    assert!(e.is_a(PARSE_ERROR), "{}", e.class_name());
    assert_eq!(e.template.as_deref(), Some("badtemplate.vm"));
    assert_eq!(e.line, 5);
    assert_eq!(e.column, 9);
}

#[test]
fn test_parse_exception_from_eval() {
    Base::new().assert_eval_exception_at("   #set($abc)   ", "", 1, 13);
}

#[test]
fn test_parse_exception_from_macro_def() {
    Base::new().assert_eval_exception_at("#macro($blarg) foo #end", "", 1, 7);
}

#[test]
fn test_parse_exception_from_macro_def_body() {
    Base::new().assert_eval_exception_at("#macro(aa $blarg) #set(!! = bb) #end #aa('aa')", "", 1, 24);
}

#[test]
fn test_parse_exception_from_macro_invoke() {
    Base::new().assert_eval_exception_at("#macro(   foo $a) $a #end #foo(woogie)", "", 1, 32);
}

#[test]
fn test_parse_exception_macro_invalid_argument_count() {
    let mut b = Base::new();
    b.set_property("velocimacro.arguments.strict", "true");
    b.assert_eval_exception_at("#macro(foo $a) $a #end #foo('test1' 'test2')", "", 1, 24);
}

#[test]
fn test_parse_exception_macro_invalid_argument_count_no_exception() {
    Base::new().assert_eval_equals("test1", "#macro(foo $a)$a#end#foo('test1' 'test2')");
}

#[test]
fn test_parse_exception_minus_sign_dissalowed() {
    Base::new().assert_eval_exception_at("${foo-bar}", "", 1, 6);
}

fn logged_engine() -> Engine {
    let mut p = ExtProperties::new();
    let instance: Arc<dyn velocity::log::Logger> = Arc::new(TestLogger::default());
    p.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    Engine::with_properties(&p).expect("initialises")
}

fn assert_parse_error(ve: &Engine, template: &str) {
    match ve.evaluate(&mut Context::new(), "foo", template) {
        Ok(_) => panic!("Could evaluate template with errors: {template}"),
        Err(e) => assert!(e.is_a(PARSE_ERROR), "{template}: {}", e.class_name()),
    }
}

#[test]
fn test_equals() {
    let ve = logged_engine();
    ve.evaluate(&mut Context::new(), "foo", "#if($a == $b) foo #end").unwrap();
    assert_parse_error(&ve, "#if($a = $b) foo #end");
}

#[test]
fn test_macro() {
    let ve = logged_engine();
    ve.evaluate(&mut Context::new(), "foo", "#macro(foo) foo #end").unwrap();
    assert_parse_error(&ve, "#macro($x) foo #end");
}

#[test]
fn test_args() {
    let ve = logged_engine();
    ve.evaluate(&mut Context::new(), "foo", "#macro(foo) foo #end").unwrap();
    ve.evaluate(&mut Context::new(), "foo", "#foreach(  $i     in  $woogie   ) end #end").unwrap();
    assert_parse_error(&ve, "#macro(   foo $a) $a #end #foo(woogie)");
}

#[test]
fn test_ast_reference_to_string_only_called_once() {
    let ve = logged_engine();
    let counter = class("org.apache.velocity.test.ParserTestCase$ToStringCounter")
        .method("toString", &[], Some(string()), |b, _| {
            let n = match b.field("timesCalled") {
                JValue::Int(n) => n,
                _ => 0,
            };
            b.set_field("timesCalled", JValue::Int(n + 1));
            ret(s("foo"))
        })
        .build()
        .with_fields(&[("timesCalled", JValue::Int(0))]);
    let mut context = Context::new();
    context.put("counter", counter.clone());
    ve.evaluate(&mut context, "foo", "$counter").unwrap();
    let JValue::Object(o) = &counter else { unreachable!() };
    let bean = o.as_any().downcast_ref::<Bean>().unwrap();
    assert!(matches!(bean.field("timesCalled"), JValue::Int(1)), "{:?}", bean.field("timesCalled"));
}

/// `org.apache.velocity.test.misc.ExceptionGeneratingEventHandler`.
struct ExceptionGeneratingEventHandler;

fn exception() -> VelocityError {
    VelocityError::runtime(JError::new("java.lang.RuntimeException", "exception"))
}

impl IncludeEventHandler for ExceptionGeneratingEventHandler {
    fn include_event(&self, _: &Engine, _: &mut Context, _: Option<&str>, _: Option<&str>, _: &str) -> Result<Option<String>, VelocityError> {
        Err(exception())
    }
}

impl MethodExceptionEventHandler for ExceptionGeneratingEventHandler {
    fn method_exception(&self, _: &Engine, _: &Context, _: &str, _: &str, _: &JError, _: &Info) -> Result<JValue, Option<VelocityError>> {
        Err(Some(exception()))
    }
}

impl ReferenceInsertionEventHandler for ExceptionGeneratingEventHandler {
    fn reference_insert(&self, _: &Engine, _: &Context, _: &str, _: JValue) -> Result<JValue, VelocityError> {
        Err(exception())
    }
}

const EXCEPTION_HANDLER: &str = "org.apache.velocity.test.misc.ExceptionGeneratingEventHandler";

fn register_fixtures() {
    register_handler_class(
        EXCEPTION_HANDLER,
        Arc::new(|| {
            let h = Arc::new(ExceptionGeneratingEventHandler);
            Handler { include: Some(h.clone()), method_exception: Some(h.clone()), reference_insertion: Some(h), ..Default::default() }
        }),
    );
    register_loader_class(
        "org.apache.velocity.test.misc.ExceptionGeneratingResourceLoader",
        Arc::new(|_| Ok(Arc::new(ExceptionGeneratingResourceLoader(LoaderSettings::default())) as Arc<dyn ResourceLoader>)),
    );
    register_directive_class("org.apache.velocity.test.misc.ExceptionGeneratingDirective", Arc::new(|| Arc::new(ExceptionGeneratingDirective) as Arc<dyn Directive>));
}

/// `org.apache.velocity.test.misc.ExceptionGeneratingResourceLoader`.
struct ExceptionGeneratingResourceLoader(LoaderSettings);

impl ResourceLoader for ExceptionGeneratingResourceLoader {
    fn class_name(&self) -> String {
        "org.apache.velocity.test.misc.ExceptionGeneratingResourceLoader".into()
    }

    fn read(&self, _: &str, _: Option<&str>) -> Result<String, VelocityError> {
        Err(exception())
    }

    fn is_source_modified(&self, _: &str, _: i64) -> bool {
        false
    }

    fn last_modified(&self, _: &str) -> i64 {
        0
    }

    fn common(&self) -> &LoaderSettings {
        &self.0
    }
}

/// `org.apache.velocity.test.misc.ExceptionGeneratingDirective`.
struct ExceptionGeneratingDirective;

impl Directive for ExceptionGeneratingDirective {
    fn name(&self) -> String {
        "Exception".into()
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Block
    }

    fn render(&self, _: &mut DirectiveCall<'_, '_>) -> Result<bool, VelocityError> {
        Err(exception())
    }
}

fn assert_exception(ve: &Engine, context: &mut Context, input: &str) {
    assert!(ve.evaluate(context, "test", input).is_err(), "Expected RuntimeException for {input}");
}

fn test_context() -> Context {
    let mut context = Context::new();
    context.put("test", s("test"));
    context
}

#[test]
fn test_reference_insertion_event_handler_exception() {
    register_fixtures();
    let ve = engine(&[("event_handler.reference_insertion.class", EXCEPTION_HANDLER)]);
    assert_exception(&ve, &mut test_context(), "this is a $test");
}

#[test]
fn test_method_exception_event_handler_exception() {
    register_fixtures();
    let ve = engine(&[("event_handler.method_exception.class", EXCEPTION_HANDLER)]);
    let mut context = Context::new();
    context.put("test", test_provider());
    for input in ["$test.getThrow()", "$test.throw"] {
        match ve.evaluate(&mut context, "test", input) {
            Err(e) if e.is_a(METHOD_INVOCATION) => {}
            other => panic!("Expected MethodInvocationException for {input}: {:?}", other.map_err(|e| e.class_name())),
        }
    }
}

#[test]
fn test_include_event_handler_exception() {
    register_fixtures();
    let ve = engine(&[("event_handler.include.class", EXCEPTION_HANDLER)]);
    assert_exception(&ve, &mut test_context(), "#include('dummy')");
}

#[test]
fn test_resource_loader_exception() {
    register_fixtures();
    let mut p = ExtProperties::new();
    p.set_property("resource.loaders", "except".into());
    p.set_property("except.resource.loader.class", "org.apache.velocity.test.misc.ExceptionGeneratingResourceLoader".into());
    let result = Engine::with_properties(&p).and_then(|ve| ve.get_template("test.txt", None).map(|_| ()));
    assert!(result.is_err(), "Should have thrown RuntimeException");
}

#[test]
fn test_directive_exception() {
    register_fixtures();
    let ve = engine(&[("userdirective", "org.apache.velocity.test.misc.ExceptionGeneratingDirective")]);
    assert_exception(&ve, &mut test_context(), "#Exception() test #end");
}

fn woogie() -> JValue {
    class("org.apache.velocity.test.MethodInvocationExceptionTestCase")
        .method("doException", &[], None, |_, _| Err(JError::without_message("java.lang.NullPointerException")))
        .method("getFoo", &[], None, |_, _| Err(JError::new("java.lang.Exception", "Hello from getFoo()")))
        .method("setFoo", &[string()], None, |_, _| Err(JError::new("java.lang.Exception", "Hello from setFoo()")))
        .build()
        .new_instance()
}

fn assert_method_invocation_exception(template: &str) {
    let ve = logged_engine();
    let mut vc = Context::new();
    vc.put("woogie", woogie());
    match ve.evaluate(&mut vc, "test", template) {
        Ok(_) => panic!("No exception thrown: {template}"),
        Err(e) => {
            assert!(e.is_a(METHOD_INVOCATION), "Wrong exception thrown for {template}: {}", e.class_name());
            let log = ve.log();
            use velocity::log::LogExt;
            log.debug(|| format!("  reference = {:?}", e.reference_name));
            log.debug(|| format!("  method    = {:?}", e.method_name));
            log.debug(|| format!("  throwable = {:?}", e.cause));
        }
    }
}

#[test]
fn test_normal_method_invocation_exception() {
    assert_method_invocation_exception("$woogie.doException() boing!");
}

#[test]
fn test_getter_method_invocation_exception() {
    assert_method_invocation_exception("$woogie.foo boing!");
}

#[test]
fn test_capitalized_getter_method_invocation_exception() {
    assert_method_invocation_exception("$woogie.Foo boing!");
}

#[test]
fn test_setter_method_invocation_exception() {
    assert_method_invocation_exception("#set($woogie.foo = 'lala') boing!");
}

#[test]
fn test_macro_invocation_exception() {
    assert_method_invocation_exception("#macro (macro1 $param) $param #end  #macro1($woogie.getFoo())");
}

#[test]
fn test_method_exception() {
    let ve = engine(&[]);
    let mut context = Context::new();
    context.put("test", test_provider());
    match ve.evaluate(&mut context, "test", "$test.getThrow()") {
        Err(e) if e.is_a(METHOD_INVOCATION) => {
            let cause = e.into_data().cause.expect("a cause");
            assert_eq!(cause.class, "java.lang.Exception");
            assert_eq!(cause.message.as_deref(), Some("From getThrow()"));
        }
        other => panic!("expected an exception: {:?}", other.map_err(|e| e.class_name())),
    }
}
