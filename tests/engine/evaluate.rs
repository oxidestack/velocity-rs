//! `EvaluateTestCase`.

use velocity::props::ExtProperties;
use velocity::{Context, Engine};

use crate::base::*;

fn test_file(b: &mut Base, name: &str, props: &[(&str, &str)]) {
    b.add_property("resource.loader.file.path", resource("evaluate"));
    for (k, v) in props {
        b.add_property(k, *v);
    }
    b.assert_matches_compare(&format!("{name}.vm"), &resource(&format!("evaluate/compare/{name}.cmp")));
}

#[test]
fn test_evaluate() {
    test_file(&mut Base::new(), "eval1", &[]);
}

#[test]
fn test_evaluate_macro_preserve() {
    test_file(&mut Base::new(), "eval2", &[]);
    test_file(&mut Base::new(), "eval2", &[("velocimacro.inline.replace_global", "false")]);
}

#[test]
fn test_evaluate_vm_context() {
    test_file(&mut Base::new(), "evalvmcontext", &[]);
}

#[test]
fn test_stop_and_break() {
    let mut b = Base::new();
    b.set_property("evaluate.provide.scope.control", "true");
    b.assert_eval_equals("t ", "t #stop t2 #evaluate('t3')");
    b.assert_eval_equals("t ", "t #break t2 #evaluate('t3')");
    b.assert_eval_equals("t t2 t3 ", "t t2 #evaluate('t3 #stop t4') t5");
    b.assert_eval_equals("t t2 t3  t5", "t t2 #evaluate('t3 #break t4') t5");
    b.assert_eval_equals("t t2 t3 ", "t t2 #evaluate('t3 #break($evaluate.topmost) t4') t5");
}

#[test]
fn test_event_handler() {
    let mut p = ExtProperties::new();
    p.set_property("event_handler.reference_insertion.class", "org.apache.velocity.app.event.implement.EscapeHtmlReference".into());
    let engine = Engine::with_properties(&p).unwrap();
    let mut context = Context::new();
    context.put("lt", s("<"));
    context.put("gt", s(">"));
    let out = engine.evaluate(&mut context, "test", "${lt}test${gt} #evaluate('${lt}test2${gt}')").unwrap();
    assert_eq!(out, "&lt;test&gt; &lt;test2&gt;");
}

#[test]
fn test_errors() {
    let engine = Engine::new();
    let mut context = Context::new();
    let check = |context: &mut Context, t: &str, line: i32, column: i32| {
        let e = engine.evaluate(context, "test", t).expect_err("Expected exception");
        assert!(e.is_a("org.apache.velocity.exception.ParseErrorException"), "{} {}", e.class_name(), e.message());
        assert_eq!(e.template.as_deref(), Some("test"));
        assert_eq!((e.line, e.column), (line, column), "{}", e.message());
    };
    check(&mut context, "#evaluate()", 1, 1);
    check(&mut context, "#evaluate('aaa' 'bbb')", 1, 17);
    check(&mut context, "#evaluate(10)", 1, 11);
    context.put("eval", s("this is a multiline\n\n\n\n\n test #foreach() with an error"));
    check(&mut context, "first line\n second line: #evaluate($eval)", 2, 15);
}
