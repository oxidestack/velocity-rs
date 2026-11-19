//! `MacroCommentsTestCase`, `MacroDefaultArgTestCase`, `MacroForwardDefineTestCase`, `VelocimacroTestCase`,
//! `VelocimacroBCModeTestCase`, `InlineScopeVMTestCase`, `MacroAutoReloadTestCase`.

use std::sync::Arc;

use velocity::props::PropValue;

use crate::base::*;
use crate::test_logger::{LOG_LEVEL_DEBUG, TestLogger};

const PARSE_ERROR: Option<&str> = Some("org.apache.velocity.exception.ParseErrorException");

#[test]
fn test_comments() {
    let mut b = Base::new();
    b.assert_eval_equals("ab", "#macro(foo ##\n $bar \n ## blaa\n $bar2##\n)$bar$bar2#end#foo(\"a\" \"b\")");
    b.assert_eval_equals("", "#macro(foo1##\n)#end#foo1()");
    b.assert_eval_equals("ab", "#macro(foo2##\n\t ####\r $bar \n ##\n## Testing  blaa\n $bar2##\n)$bar$bar2#end#foo2(\"a\" \"b\")");
    b.assert_eval_equals("", "#macro(foo4 ## test\n  ## test2  ## test3 \n)#end#foo4()");
    b.assert_eval_equals("", "#macro(foo4 ## test\n  $x = 5 ## test2  ## test3 \n)#end#foo4()");
}

#[test]
fn test_comment_errors() {
    let mut b = Base::new();
    b.assert_eval_exception("#foo1(## test)", PARSE_ERROR);
    b.assert_eval_exception("#foo1($test ## test)", PARSE_ERROR);
    b.assert_eval_exception("#break(## test)", PARSE_ERROR);
}

fn default_arg() -> Base {
    let mut b = Base::new();
    b.set_property("runtime.strict_mode.enable", true);
    b.set_property("velocimacro.arguments.strict", true);
    b.set_property("velocimacro.inline.local_scope", true);
    b.set_property("runtime.strict_mode.escape", true);
    b
}

#[test]
fn test_compare() {
    let mut b = default_arg();
    b.assert_eval_equals("121", "#macro(foo $a=1)$a#end#foo()#foo(2)#foo");
    b.assert_eval_equals("12", "#macro(foo $a = 1)$a#end#foo()#foo(2)");
    b.assert_eval_equals("12", "#macro(foo $a= 1 )$a#end#foo()#foo(2)");
    b.assert_eval_equals("1x2x", "#macro(foo $a= 1 $b = \"x\")$a$b#end#foo()#foo(2)");
    b.assert_eval_equals("1 2 5 2 5 [1, 2] ", "#macro(foo $a=1 $b=2)$a $b #end#foo()#foo(5)#foo(5 [1,2])");
    b.assert_eval_equals("1 2 5 2 5 [1, 2] ", "#macro(foo $a=1 , $b=2)$a $b #end#foo()#foo(5)#foo(5 [1,2])");
    b.assert_eval_equals("1 2 5 2 5 [1, 2] ", "#macro(foo, $a=1\n $b =2 )$a $b #end#foo()#foo(5)#foo(5 [1,2])");

    b.assert_eval_equals("3 2 5 2 5 [1, 2] ", "#macro(foo, $a=$x $b =2 )$a $b #end#set($x=3)#foo()#foo(5)#foo(5 [1,2])");
    b.assert_eval_equals("{a=3} 2 5 2 5 [1, 2] ", "#macro(foo, $a = {\"a\":$x} $b =2 )$a $b #end#set($x=3)#foo()#foo(5)#foo(5 [1,2])");
    b.assert_eval_equals("3 2 5 2 5 [1, 2] ", "#macro(foo, $a = \"$x\" $b =2 )$a $b #end#set($x=3)#foo()#foo(5)#foo(5 [1,2])");
    b.assert_eval_equals("3$y 2 5 2 5 [1, 2] ", "#macro(foo, $a = \"$x\\$y\" $b =2 )$a $b #end#set($x=3)#foo()#foo(5)#foo(5 [1,2])");
    b.assert_eval_equals("5 3 2 5 [1, 2] 2 ", "#macro(foo, $c $a = \"$x\" $b =2 )$c $a $b #end#set($x=3)#foo(5)#foo(5 [1,2])");

    b.assert_eval_equals("1xno2xyes", "#macro(foo $a= 1 $b = \"x\")$a$b$bodyContent#end#@foo()no#end#@foo(2)yes#end");

    b.assert_eval_equals("xy", "#macro(foo $a=\"$b$c\"##\n)$a#end#set($b=\"x\")#set($c=\"y\")#foo()");
}

#[test]
fn test_default_arg_errors() {
    let mut b = default_arg();
    b.assert_eval_exception("#macro(foo $a = 1 $b)#end", None);
    b.assert_eval_exception("#macro(foo $c $a = 3 $b)#end", None);
    b.assert_eval_exception("#macro(foo $a $b = 1)#end#foo()", None);
    b.assert_eval_exception("#macro(foo $a $b $c = 4)#end#foo(1)", None);
    b.assert_eval_exception("#macro(foo $a = 3)#end#foo(2 3)", None);
}

#[test]
fn test_log_result() {
    // `Velocity.reset()`, a file loader on `macroforwarddefine`, invalid references logged, debug output captured.
    let logger = Arc::new(TestLogger::new(false, true));
    logger.set_enabled_level(LOG_LEVEL_DEBUG);
    let mut props = velocity::props::ExtProperties::new();
    props.set_property("resource.loaders", "file".into());
    props.set_property("resource.loader.file.path", resource("macroforwarddefine").into());
    props.set_property("runtime.log.log_invalid_references", "true".into());
    let instance: Arc<dyn velocity::log::Logger> = logger.clone();
    props.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    let engine = velocity::Engine::with_properties(&props).expect("initialises");
    let template = engine.get_template("macros.vm", None).expect("macros.vm");
    logger.start_capture();
    engine.merge(&template, &mut velocity::Context::new()).expect("merges");
    logger.stop_capture();
    let result = logger.get_log();
    let compare = file_contents(&resource("macroforwarddefine/compare/velocity.log.cmp"));
    assert_eq!(normalize_newlines(&result), normalize_newlines(&compare), "Log output was incorrect");
}

const TEMPLATE1: &str = "#macro(foo $a)$a#end #macro(bar $b)#foo($b)#end #foreach($i in [1..3])#if($i == 3)#foo($i)#else#bar($i)#end#end";
const RESULT1: &str = "  123";
const TEMPLATE2: &str = "#macro(bar $a)#set($a = $a + 1)$a#bar($a)#end#bar(0)";
const TEMPLATE3: &str = "#macro(baz $a)#set($a = $a + 1)$a#inner($a)#end#macro(inner $b)#baz($b)#end#baz(0)";
const TEMPLATE4: &str = "#macro(bad $a)#set($a = $a + 1)$a#inside($a)#end#macro(inside $b)#loop($b)#end#macro(loop $c)#bad($c)#end#bad(0)";

fn velocimacro_engine() -> velocity::Engine {
    let mut props = velocity::props::ExtProperties::new();
    props.set_property("velocimacro.inline.local_scope", true.into());
    props.set_property("velocimacro.max_depth", 5.into());
    let instance: Arc<dyn velocity::log::Logger> = Arc::new(TestLogger::default());
    props.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    velocity::Engine::with_properties(&props).expect("initialises")
}

#[test]
fn test_velocimacro() {
    let engine = velocimacro_engine();
    let out = engine.evaluate(&mut velocity::Context::new(), "vm_chain1", TEMPLATE1).expect("renders");
    assert_eq!(out, RESULT1, "output incorrect.");
}

#[test]
fn test_velocimacro_call_max() {
    let engine = velocimacro_engine();
    for (name, template, message) in [
        ("vm_chain2", TEMPLATE2, "Max calling depth of 5 was exceeded in macro 'bar' with Call Stack:bar->bar->bar->bar->bar at vm_chain2[line 1, column 15]"),
        ("vm_chain3", TEMPLATE3, "Max calling depth of 5 was exceeded in macro 'inner' with Call Stack:baz->inner->baz->inner->baz at vm_chain3[line 1, column 64]"),
        ("vm_chain4", TEMPLATE4, "Max calling depth of 5 was exceeded in macro 'loop' with Call Stack:bad->inside->loop->bad->inside at vm_chain4[line 1, column 94]"),
    ] {
        match engine.evaluate(&mut velocity::Context::new(), name, template) {
            Ok(_) => panic!("Did not exceed max macro call depth as expected"),
            Err(e) => {
                assert!(e.is_a("org.apache.velocity.exception.MacroOverflowException"), "{}", e.class_name());
                assert_eq!(e.message(), message);
            }
        }
    }
}

/// `VelocimacroBCModeTestCase`: `velocimacro.enable_bc_mode` except in the `NoPreserve` test.
fn bc_mode(enabled: bool) -> Base {
    let mut b = Base::new();
    b.set_property("velocimacro.enable_bc_mode", enabled);
    b.set_property("file.resource.loader.path", resource("bc_mode"));
    b
}

#[test]
fn test_preserve_literals() {
    bc_mode(true).assert_eval_equals("$bar", "#macro(m $foo)$foo#end#m($bar)");
}

#[test]
fn test_global_defaults() {
    bc_mode(true).assert_eval_equals("foo", "#macro(m $foo)$foo#end#set($foo='foo')#m()");
}

fn various_cases(enabled: bool, compare_ext: &str) {
    let mut b = bc_mode(enabled);
    b.context = velocity::Context::new();
    let result = b.merge("test_bc_mode.vtl").expect("merges");
    let compare = file_contents(&resource(&format!("bc_mode/compare/test_bc_mode.{compare_ext}")));
    assert_eq!(normalize_newlines(&compare), normalize_newlines(&result));
}

#[test]
fn test_various_cases_preserve() {
    various_cases(true, "bc_mode_enabled");
}

#[test]
fn test_various_cases_no_preserve() {
    various_cases(false, "bc_mode_disabled");
}

#[test]
fn test_inline_scope_vm() {
    let mut props = velocity::props::ExtProperties::new();
    props.set_property("velocimacro.inline.replace_global", "true".into());
    props.set_property("velocimacro.inline.local_scope", "true".into());
    props.set_property("resource.loader.file.path", resource("templates").into());
    let instance: Arc<dyn velocity::log::Logger> = Arc::new(TestLogger::default());
    props.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    props.set_property("space.gobbling", "bc".into());
    let engine = velocity::Engine::with_properties(&props).expect("initialises");
    // vm_test2 first: its macros must not leak into vm_test1.
    let template2 = engine.get_template("vm_test2.vm", None).expect("vm_test2");
    let template1 = engine.get_template("vm_test1.vm", None).expect("vm_test1");
    let mut context = velocity::Context::new();
    let out1 = engine.merge(&template1, &mut context).expect("vm_test1 merges");
    let out2 = engine.merge(&template2, &mut context).expect("vm_test2 merges");
    for (name, out) in [("vm_test1", out1), ("vm_test2", out2)] {
        let compare = file_contents(&resource(&format!("templates/compare/{name}.cmp")));
        assert_eq!(normalize_newlines(&out), normalize_newlines(&compare), "Output incorrect for {name}");
    }
}

/// `MacroAutoReloadTestCase`: a copy of `reload/` (the Java test rewrites `macros2.vtl` in place).
fn auto_reload(dir: &std::path::Path) -> Base {
    std::fs::create_dir_all(dir).expect("temp dir");
    for f in ["macros.vtl", "foo.vtl"] {
        std::fs::copy(resource(&format!("reload/{f}")), dir.join(f)).expect("copies");
    }
    std::fs::copy(resource("reload/macros.vtl"), dir.join("macros2.vtl")).expect("copies");
    let mut b = Base::new();
    b.props = velocity::props::ExtProperties::new();
    let instance: Arc<dyn velocity::log::Logger> = b.log.clone();
    b.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    b.set_property("resource.loaders", "file,string");
    b.add_property("file.resource.loader.path", dir.to_string_lossy().as_ref());
    b.add_property("velocimacro.library", "macros2.vtl");
    b.add_property("velocimacro.library.autoreload", "true");
    b.add_property("file.resource.loader.cache", "false");
    b.add_property("string.resource.loader.class", velocity::resource::STRING_LOADER);
    b.add_property("string.resource.loader.repository.name", "stringRepo");
    b.add_property("string.resource.loader.repository.static", "false");
    b.string_repo_name = "stringRepo".into();
    b
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("velocity-rs-{name}-{}", std::process::id()))
}

/// `File.setLastModified(lastModified() + 1000)`.
fn touch_later(path: &std::path::Path) {
    let f = std::fs::File::options().write(true).open(path).expect("opens");
    let modified = f.metadata().and_then(|m| m.modified()).expect("mtime");
    f.set_modified(modified + std::time::Duration::from_secs(1)).expect("sets mtime");
}

#[test]
fn test_changed_macro() {
    let dir = temp_dir("changed-macro");
    let mut b = auto_reload(&dir);
    let template = "#foo('hip')";
    b.assert_eval_equals("hop_hip", template);
    std::fs::write(dir.join("macros2.vtl"), "#macro(foo $txt)hip_$txt#{end}").expect("writes");
    touch_later(&dir.join("macros2.vtl"));
    b.assert_eval_equals("hip_hip", template);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_new_macro() {
    let dir = temp_dir("new-macro");
    let mut b = auto_reload(&dir);
    b.assert_eval_equals("hop_hip", "#foo('hip')");
    let mut text = std::fs::read_to_string(dir.join("macros2.vtl")).expect("reads");
    text.push_str("\n#macro(bar $txt)hep_$txt#{end}");
    std::fs::write(dir.join("macros2.vtl"), text).expect("writes");
    touch_later(&dir.join("macros2.vtl"));
    b.assert_eval_equals("hop_hip hep_hip", "#foo('hip') #bar('hip')");
    let _ = std::fs::remove_dir_all(&dir);
}
