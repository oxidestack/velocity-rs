//! `VMLibraryTestCase`, `ParseWithMacroLibsTestCase`.

use std::sync::Arc;

use velocity::props::{ExtProperties, PropValue};
use velocity::{Context, Engine};

use crate::base::*;
use crate::test_logger::TestLogger;

fn engine(dir: &str, local: bool, replace_global: bool, cache: bool) -> Engine {
    let mut props = ExtProperties::new();
    props.set_property("velocimacro.inline.local_scope", local.into());
    props.set_property("velocimacro.permissions.allow.inline.to.replace.global", replace_global.into());
    props.set_property("file.resource.loader.cache", cache.into());
    let instance: Arc<dyn velocity::log::Logger> = Arc::new(TestLogger::default());
    props.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    props.set_property("resource.loaders", "file".into());
    props.set_property("resource.loader.file.path", resource(dir).into());
    Engine::with_properties(&props).expect("initialises")
}

/// `ve1`: template-local macros, no replacing globals, cached templates.
fn ve1() -> Engine {
    engine("macrolibs", true, false, true)
}

/// `ve2`: global macros that may be replaced, templates not cached.
fn ve2() -> Engine {
    engine("macrolibs", false, true, false)
}

#[track_caller]
fn assert_matches(result: &str, compare: &str) {
    let expected = file_contents(&resource(compare));
    assert_eq!(normalize_newlines(result), normalize_newlines(&expected), "Processed template did not match expected output");
}

#[test]
fn test_veloci_macro_lib_with_local_namespace() {
    let ve1 = ve1();
    let mut context = Context::new();
    let mut out = String::new();
    let template = ve1.get_template("vm_library_local.vm", None).unwrap();
    out += &ve1.merge_with_libraries(&template, &mut context, &["vm_library1.vm"]).unwrap();
    let template = ve1.get_template("vm_library_local.vm", None).unwrap();
    out += &ve1.merge_with_libraries(&template, &mut context, &["vm_library2.vm"]).unwrap();
    let t1 = ve1.get_template("vm_library_local.vm", None).unwrap();
    let t2 = ve1.get_template("vm_library_local.vm", None).unwrap();
    assert!(Arc::ptr_eq(&t1, &t2), "Both templates refer to the same object");
    let template = ve1.get_template("vm_library_local.vm", None).unwrap();
    out += &ve1.merge(&template, &mut context).unwrap();
    assert_matches(&out, "macrolibs/compare/vm_library_local.cmp");
}

#[test]
fn test_veloci_macro_lib_with_global_namespace() {
    let (ve1, ve2) = (ve1(), ve2());
    let mut context = Context::new();
    let mut out = String::new();
    let template = ve1.get_template("vm_library_global.vm", None).unwrap();
    out += &ve1.merge_with_libraries(&template, &mut context, &["vm_library1.vm"]).unwrap();
    let template = ve1.get_template("vm_library_global.vm", None).unwrap();
    out += &ve1.merge_with_libraries(&template, &mut context, &["vm_library2.vm"]).unwrap();
    let t1 = ve2.get_template("vm_library_global.vm", None).unwrap();
    let t2 = ve2.get_template("vm_library_global.vm", None).unwrap();
    assert!(!Arc::ptr_eq(&t1, &t2), "Defferent objects");
    assert_matches(&out, "macrolibs/compare/vm_library_global.cmp");
}

#[test]
fn test_veloci_macro_lib_with_duplicate_definitions() {
    let ve1 = ve1();
    let template = ve1.get_template("vm_library.vm", None).unwrap();
    let out = ve1.merge_with_libraries(&template, &mut Context::new(), &["vm_library1.vm", "vm_library2.vm"]).unwrap();
    assert_matches(&out, "macrolibs/compare/vm_library_duplicate.cmp");
}

#[test]
fn test_macros_with_no_definition() {
    let ve1 = ve1();
    let template = ve1.get_template("vm_library.vm", None).unwrap();
    // `merge(context, writer, null)`.
    let out = ve1.merge(&template, &mut Context::new()).unwrap();
    assert_matches(&out, "macrolibs/compare/vm_library.cmp");
}

fn parse_engine(cache: bool, local: bool) -> Engine {
    engine("parsemacros", true, local, cache)
}

fn test_parse_macro(ve: &Engine, include_file: &str, output: &str, test_caching_off: bool) {
    let mut context = Context::new();
    context.put("includefile", s(include_file));
    let template = ve.get_template("parseMacro1.vm", None).unwrap();
    let out = ve.merge(&template, &mut context).unwrap();
    assert_matches(&out, &format!("parsemacros/compare/{output}.cmp"));
    if test_caching_off {
        let t1 = ve.get_template("parseMacro1.vm", None).unwrap();
        let t2 = ve.get_template("parseMacro1.vm", None).unwrap();
        assert!(!Arc::ptr_eq(&t1, &t2), "Different objects");
    }
}

#[test]
fn test_parse_macro_local_cache_on() {
    let ve = parse_engine(true, true);
    test_parse_macro(&ve, "vm_library1.vm", "parseMacro1_1", false);
    test_parse_macro(&ve, "vm_library1.vm", "parseMacro1_1", false);
    test_parse_macro(&ve, "vm_library2.vm", "parseMacro1_1b", false);
    test_parse_macro(&ve, "vm_library2.vm", "parseMacro1_1b", false);
}

#[test]
fn test_parse_macro_local_cache_off() {
    let ve = parse_engine(false, true);
    test_parse_macro(&ve, "vm_library1.vm", "parseMacro1_2", true);
    test_parse_macro(&ve, "vm_library2.vm", "parseMacro1_2b", true);
}

#[test]
fn test_parse_macro_global_cache_on() {
    let ve = parse_engine(true, false);
    test_parse_macro(&ve, "vm_library1.vm", "parseMacro1_3", false);
    test_parse_macro(&ve, "vm_library1.vm", "parseMacro1_3", false);
    test_parse_macro(&ve, "vm_library2.vm", "parseMacro1_3b", false);
    test_parse_macro(&ve, "vm_library2.vm", "parseMacro1_3b", false);
}

#[test]
fn test_parse_macro_global_cache_off() {
    let ve = parse_engine(false, false);
    test_parse_macro(&ve, "vm_library1.vm", "parseMacro1_4", true);
    test_parse_macro(&ve, "vm_library2.vm", "parseMacro1_4b", true);
}

fn parse_ve1() -> Engine {
    engine("parsemacros", true, false, true)
}

#[test]
fn test_parse_macros_with_no_definition() {
    let ve1 = parse_ve1();
    let template = ve1.get_template("parseMacro2.vm", None).unwrap();
    let out = ve1.merge(&template, &mut Context::new()).unwrap();
    assert_matches(&out, "parsemacros/compare/parseMacro2.cmp");
}

#[test]
fn test_duplicate_definitions() {
    let ve1 = parse_ve1();
    let template = ve1.get_template("parseMacro3.vm", None).unwrap();
    let out = ve1.merge(&template, &mut Context::new()).unwrap();
    assert_matches(&out, "parsemacros/compare/parseMacro3.cmp");
}
