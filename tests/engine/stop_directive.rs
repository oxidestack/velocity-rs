//! `StopDirectiveTestCase`.

use crate::base::*;

fn base() -> Base {
    let mut b = Base::new();
    b.set_property("resource.loader.file.path", resource("stop"));
    b.set_property("velocimacro.library.path", "vmlib1.vm");
    b
}

#[test]
fn test_stop() {
    let mut b = base();
    b.assert_eval_equals("Text1", "Text1#{stop}Text2");
    b.assert_tmpl_equals("Text 1", "stop1.vm");
    b.assert_tmpl_equals("Text123stuff1", "stop2.vm");
    b.assert_tmpl_equals("text1blaa1", "stop3.vm");
}

#[test]
fn test_nested_stop_all() {
    let mut b = base();
    b.add_template(
        "ns",
        ",template#macro(vm),macro${bodyContent}macro#end#define($define),define#foreach($i in [1..2]),foreach#{stop}foreach#{end}define#{end}#@vm(),bodyContent${define}bodyContent#{end}template",
    );
    b.assert_eval_equals("evaluate,template,macro,bodyContent,define,foreach", "#evaluate('evaluate#parse(\"ns\")evaluate')");
}

#[test]
fn test_stop_message() {
    let mut b = base();
    b.log.set_enabled_level(crate::test_logger::LOG_LEVEL_DEBUG);
    b.put("log", java_compat::lang::JValue::Object(b.log.clone()));
    b.assert_eval_equals("a", "a$!log.startCapture()#stop('woogie!')b");
    b.log.stop_capture();
    b.log.on();
    let log = b.log.get_log();
    assert!(log.contains("StopCommand: woogie!"), "Log: {log}");
}
