//! `BuiltInEventHandlerTestCase`.

use std::sync::Arc;

use java_compat::lang::{JList, JValue};
use velocity::events::{EventCartridge, Handler, ReferenceInsertionEventHandler};
use velocity::handlers::{EscapeReference, ReportInvalidReferences};
use velocity::props::ExtProperties;
use velocity::{Context, Engine};

use crate::base::*;

const TEMPLATE: &str = "$a1 $c1 $a1.length() $a1.foobar() $!c1 $n1 $!n1 #if($c1) nop #end";

/// `new VelocityEngine()` with `properties`, then `init()`.
fn engine(properties: &[(&str, &str)]) -> Engine {
    let mut p = ExtProperties::new();
    for (k, v) in properties {
        p.set_property(k, (*v).into());
    }
    Engine::with_properties(&p).expect("initialises")
}

/// A context with a cartridge holding `reporter`, and `$a1`, `$b1`, `$n1`.
fn reporting_context(reporter: &Arc<ReportInvalidReferences>) -> Context {
    let mut context = Context::new();
    let ec = EventCartridge::new();
    ec.add(Handler { invalid_reference: Some(reporter.clone()), ..Default::default() });
    context.attach_event_cartridge(ec);
    context.put("a1", s("test"));
    context.put("b1", s("test"));
    context.put("n1", JValue::Null);
    context
}

fn invalid(reporter: &ReportInvalidReferences) -> Vec<String> {
    reporter.invalid.lock().iter().map(|i| i.reference.clone()).collect()
}

#[test]
fn test_report_invalid_references1() {
    let ve = engine(&[]);
    let reporter = Arc::new(ReportInvalidReferences::default());
    let mut context = reporting_context(&reporter);
    ve.evaluate(&mut context, "test", TEMPLATE).unwrap();
    assert_eq!(invalid(&reporter), ["$c1", "$a1.foobar()"]);
}

#[test]
fn test_report_invalid_references2() {
    let ve = engine(&[("event_handler.invalid_references.exception", "true")]);
    let reporter = Arc::new(ReportInvalidReferences::default());
    let mut context = reporting_context(&reporter);
    ve.evaluate(&mut context, "test", "$a1 no problem").unwrap();
    assert!(ve.evaluate(&mut context, "test", "$a1 $c1 $a1.length() $a1.foobar()").is_err(), "Expected exception.");
}

#[test]
fn test_report_quiet_invalid_references() {
    let ve = engine(&[("event_handler.invalid_references.quiet", "true")]);
    let reporter = Arc::new(ReportInvalidReferences::default());
    let mut context = reporting_context(&reporter);
    ve.evaluate(&mut context, "test", TEMPLATE).unwrap();
    assert_eq!(invalid(&reporter), ["$c1", "$a1.foobar()", "$c1"]);
}

#[test]
fn test_report_null_invalid_references() {
    let ve = engine(&[("event_handler.invalid_references.null", "true")]);
    let reporter = Arc::new(ReportInvalidReferences::default());
    let mut context = reporting_context(&reporter);
    ve.evaluate(&mut context, "test", TEMPLATE).unwrap();
    assert_eq!(invalid(&reporter), ["$c1", "$a1.foobar()", "$n1"]);
}

#[test]
fn test_report_null_quiet_invalid_references() {
    let ve = engine(&[("event_handler.invalid_references.quiet", "true"), ("event_handler.invalid_references.null", "true")]);
    let reporter = Arc::new(ReportInvalidReferences::default());
    let mut context = reporting_context(&reporter);
    ve.evaluate(&mut context, "test", TEMPLATE).unwrap();
    assert_eq!(invalid(&reporter), ["$c1", "$a1.foobar()", "$c1", "$n1", "$n1"]);
}

#[test]
fn test_report_tested_invalid_references() {
    let ve = engine(&[("event_handler.invalid_references.tested", "true")]);
    let reporter = Arc::new(ReportInvalidReferences::default());
    let mut context = reporting_context(&reporter);
    ve.evaluate(&mut context, "test", TEMPLATE).unwrap();
    assert_eq!(invalid(&reporter), ["$c1", "$a1.foobar()", "$c1"]);
}

/// `esc.referenceInsert(null, "", value)` on a handler that never got the runtime (no match expression).
fn insert(esc: &EscapeReference, value: &str) -> String {
    esc.reference_insert(&Engine::new(), &Context::new(), "", s(value)).unwrap().to_java_string()
}

#[test]
fn test_escape_html() {
    let esc = EscapeReference::html();
    assert_eq!(insert(&esc, "test string&another<b>bold</b>test"), "test string&amp;another&lt;b&gt;bold&lt;/b&gt;test");
    assert_eq!(insert(&esc, "<\">"), "&lt;&quot;&gt;");
    assert_eq!(insert(&esc, "test string"), "test string");
}

#[test]
fn test_escape_xml() {
    let esc = EscapeReference::xml();
    assert_eq!(insert(&esc, "test string&another<b>bold</b>test"), "test string&amp;another&lt;b&gt;bold&lt;/b&gt;test");
    assert_eq!(insert(&esc, "<\">"), "&lt;&quot;&gt;");
    assert_eq!(insert(&esc, "'"), "&apos;");
    assert_eq!(insert(&esc, "test string"), "test string");
}

#[test]
fn test_escape_sql() {
    let esc = EscapeReference::sql();
    assert_eq!(insert(&esc, "Jimmy's Pizza"), "Jimmy''s Pizza");
    assert_eq!(insert(&esc, "test string"), "test string");
}

#[test]
fn test_escape_java_script() {
    let esc = EscapeReference::javascript();
    assert_eq!(insert(&esc, "Jimmy's Pizza"), "Jimmy\\'s Pizza");
    assert_eq!(insert(&esc, "test string"), "test string");
}

#[test]
fn test_escape_reference_match_all() {
    let ve = engine(&[("event_handler.reference_insertion.class", "org.apache.velocity.app.event.implement.EscapeHtmlReference")]);
    let mut context = Context::new();
    context.put("bold", s("<b>"));
    assert_eq!(ve.evaluate(&mut context, "test", "$bold test & test").unwrap(), "&lt;b&gt; test & test");
    let mut context = Context::new();
    context.put("bold", s("<b>"));
    assert_eq!(ve.evaluate(&mut context, "test", "$bold.substring(0,1)").unwrap(), "&lt;");
}

fn new_escape_context() -> Context {
    let mut context = Context::new();
    for k in ["test1", "test1_js", "test1_js_test"] {
        context.put(k, s("Jimmy's <b>pizza</b>"));
    }
    context
}

#[test]
fn test_escape_reference_match() {
    let ve = engine(&[
        ("event_handler.reference_insertion.class", "org.apache.velocity.app.event.implement.EscapeHtmlReference,org.apache.velocity.app.event.implement.EscapeJavaScriptReference"),
        ("eventhandler.escape.javascript.match", "/.*_js.*/"),
    ]);
    assert_eq!(ve.evaluate(&mut new_escape_context(), "test", "$test1").unwrap(), "Jimmy's &lt;b&gt;pizza&lt;/b&gt;");
}

#[test]
fn test_print_exception_handler() {
    const HANDLER: (&str, &str) = ("event_handler.method_exception.class", "org.apache.velocity.app.event.implement.PrintExceptions");
    let ve1 = engine(&[HANDLER]);
    let ve2 = engine(&[HANDLER, ("eventhandler.methodexception.templateinfo", "true")]);
    let ve3 = engine(&[HANDLER, ("eventhandler.methodexception.stacktrace", "true")]);
    let mut context = Context::new();
    context.put("list", JValue::List(JList::array_list(vec![])));
    for (ve, template_info, stack_trace) in [(&ve1, false, false), (&ve2, true, false), (&ve3, false, true)] {
        let result = ve.evaluate(&mut context, "test", "$list.get(0)").unwrap();
        assert!(result.contains("IndexOutOfBoundsException"), "{result}");
        assert!(result.contains("Index: 0, Size: 0") || result.contains("Index 0 out of bounds for length 0"), "{result}");
        assert_eq!(result.contains("at test (line 1, column 7)"), template_info, "{result}");
        assert_eq!(result.contains("rangeCheck") || result.contains("Preconditions.outOfBounds"), stack_trace, "{result}");
    }
}

#[test]
fn test_include_not_found() {
    let mut p = ExtProperties::new();
    p.set_property("event_handler.include.class", "org.apache.velocity.app.event.implement.IncludeNotFound".into());
    p.add_property("resource.loader.file.path", resource("includeevent").into());
    let ve = Engine::with_properties(&p).unwrap();
    let template = ve.get_template("test6.vm", None).unwrap();
    let out = ve.merge(&template, &mut Context::new()).unwrap();
    assert_eq!(normalize_newlines(&out), normalize_newlines(&file_contents(&resource("includeevent/compare/test6.cmp"))), "Output incorrect.");
}

#[test]
fn test_include_not_found_missing_resource_name() {
    let mut b = Base::new();
    b.set_property("event_handler.include.class", "org.apache.velocity.app.event.implement.IncludeNotFound");
    b.add_template("notfound.vm", "$missingResource");
    b.assert_eval_equals("foo", "#parse('foo')");
}

#[test]
fn test_include_relative_path() {
    let mut p = ExtProperties::new();
    p.set_property("event_handler.include.class", "org.apache.velocity.app.event.implement.IncludeRelativePath".into());
    p.add_property("resource.loader.file.path", resource("includeevent").into());
    let ve = Engine::with_properties(&p).unwrap();
    let template = ve.get_template("subdir/test2.vm", None).unwrap();
    let out = ve.merge(&template, &mut Context::new()).unwrap();
    assert_eq!(normalize_newlines(&out), normalize_newlines(&file_contents(&resource("includeevent/compare/test2.cmp"))), "Output incorrect.");
}
