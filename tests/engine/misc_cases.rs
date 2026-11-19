//! `ContextAutoreferenceKeyTestCase`, `ContextSafetyTestCase`, `CommonsExtPropTestCase`, `OldPropertiesTestCase`,
//! `IntegerRangeMutabilityTest`, `RenderVelocityTemplateTest`, `VelocityAppTestCase`, `TestBaseTestCase`.

use std::sync::Arc;

use java_compat::lang::{JArray, JValue};
use java_compat::reflect::JType;
use velocity::props::{ExtProperties, PropValue};
use velocity::{Context, Engine};

use crate::base::*;
use crate::test_logger::{LOG_LEVEL_WARN, TestLogger};

#[test]
fn test_autoreference() {
    let mut b = Base::new();
    b.set_property("context.self_reference_key", "self");
    b.put("foo", s("bar"));
    b.assert_eval_equals("bar", "$foo");
    b.assert_eval_equals("bar", "$self.foo");
    b.assert_eval_equals("bar", "$self.self.foo");
    b.assert_eval_equals("true", "$self.containsKey('foo')");
    b.assert_eval_equals("false", "$self.containsKey('bar')");
    b.assert_eval_equals("bar", "$self.put('foo', 'baz')");
    b.assert_eval_equals("baz", "$foo");
}

fn templates_engine() -> Engine {
    let mut p = ExtProperties::new();
    p.set_property("resource.loader.file.path", resource("templates").into());
    let instance: Arc<dyn velocity::log::Logger> = Arc::new(TestLogger::default());
    p.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    Engine::with_properties(&p).unwrap()
}

#[test]
fn test_context_safety() {
    let engine = templates_engine();
    let template = engine.get_template("context_safety.vm", None).unwrap();
    let mut context = Context::new();
    // A `Vector` (a list) and a `String[]`, which `#foreach` treats differently.
    context.put("vector", JValue::string_list(["vector hello 1", "vector hello 2", "vector hello 3"]));
    let out1 = engine.merge(&template, &mut context).unwrap();
    context.put("vector", JValue::Array(Arc::new(JArray::new(JType::string(), vec![s("array hello 1"), s("array hello 2"), s("array hello 3")]))));
    let out2 = engine.merge(&template, &mut context).unwrap();
    for (out, name) in [(out1, "context_safety1"), (out2, "context_safety2")] {
        assert_eq!(normalize_newlines(&out), normalize_newlines(&file_contents(&resource(&format!("templates/compare/{name}.cmp")))), "Output incorrect for {name}");
    }
}

#[test]
fn test_extended_properties() {
    let c = ExtProperties::from_file(std::path::Path::new(&resource("configuration/test-config.properties"))).unwrap();
    let mut result = String::new();
    let message = |result: &mut String, m: &str| {
        result.push_str("--------------------------------------------------\n");
        result.push_str(&format!("{m}\n"));
        result.push_str("--------------------------------------------------\n\n");
    };
    let show = |result: &mut String, items: &[String]| {
        for i in items {
            result.push_str(&format!("{i}\n"));
        }
        result.push('\n');
    };
    message(&mut result, "Testing order of keys ...");
    show(&mut result, c.keys());
    message(&mut result, "Testing retrieval of CSV values ...");
    show(&mut result, &c.get_string_array("resource.loaders").unwrap());
    message(&mut result, "Testing subset(prefix).getKeys() ...");
    let subset = c.subset("resource.loader.file").unwrap();
    show(&mut result, subset.keys());
    message(&mut result, "Testing getVector(prefix) ...");
    show(&mut result, &subset.get_string_array("path").unwrap());
    message(&mut result, "Testing getString(key) ...");
    result.push_str(&format!("{}\n\n", c.get_string("config.string.value", None).unwrap().unwrap()));
    message(&mut result, "Testing getBoolean(key) ...");
    result.push_str(&format!("{}\n\n", c.get_boolean("config.boolean.value", false).unwrap()));
    message(&mut result, "Testing getByte(key) ...");
    result.push_str(&format!("{}\n\n", c.get_byte("config.byte.value").unwrap()));
    message(&mut result, "Testing getShort(key) ...");
    result.push_str(&format!("{}\n\n", c.get_short("config.short.value").unwrap()));
    message(&mut result, "Testing getInt(key) ...");
    result.push_str(&format!("{}\n\n", c.get_int("config.int.value", 0).unwrap()));
    message(&mut result, "Testing getLong(key) ...");
    result.push_str(&format!("{}\n\n", c.get_long("config.long.value", 0).unwrap()));
    message(&mut result, "Testing getFloat(key) ...");
    result.push_str(&format!("{}\n\n", JValue::Float(c.get_float("config.float.value").unwrap()).to_java_string()));
    message(&mut result, "Testing getDouble(key) ...");
    result.push_str(&format!("{}\n\n", JValue::Double(c.get_double("config.double.value").unwrap()).to_java_string()));
    message(&mut result, "Testing escaped-comma scalar...");
    result.push_str(&format!("{}\n\n", c.get_string("escape.comma1", None).unwrap().unwrap()));
    message(&mut result, "Testing escaped-comma vector...");
    show(&mut result, &c.get_string_array("escape.comma2").unwrap());
    result.push_str("\n\n");
    assert_eq!(normalize_newlines(&result), normalize_newlines(&file_contents(&resource("configuration/compare/output.cmp"))), "Output incorrect.");
}

/// `OldPropertiesTestCase.warnPattern`: `[warn] configuration key '<old>' has been deprecated in favor of '<new>'`.
fn deprecation_warning(line: &str) -> Option<(String, String)> {
    let rest = line.trim_start().strip_prefix("[warn]")?.trim_start().strip_prefix("configuration key '")?;
    let (old, rest) = rest.split_once("' has been deprecated in favor of '")?;
    let new = rest.strip_suffix('\'')?;
    let key = |k: &str| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphabetic() || c == '.' || c == '_');
    (key(old) && key(new)).then(|| (old.to_string(), new.to_string()))
}

#[test]
fn test_old_properties() {
    let old_properties = resource("oldproperties/velocity.properties");
    let logger = Arc::new(TestLogger::new(false, true));
    logger.set_enabled_level(LOG_LEVEL_WARN);
    velocity::props::set_deprecation_logger(logger.clone());
    logger.on();
    // `ve.setProperties(oldProperties)`.
    let ve = ExtProperties::from_file(std::path::Path::new(&old_properties)).unwrap();
    logger.off();
    let mut old_keys = std::collections::HashSet::new();
    let text: String = std::fs::read(&old_properties).unwrap().iter().map(|b| *b as char).collect();
    for line in text.lines() {
        // `^([a-z._]+)\s*=\s*[^#]+.*$`, case-insensitively.
        let Some((name, value)) = line.split_once('=') else { continue };
        let name = name.trim_end();
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphabetic() || c == '.' || c == '_') || value.trim_start().is_empty() || value.trim_start().starts_with('#') {
            continue;
        }
        let translated = velocity::props::translate_key(name);
        if translated != name {
            assert_eq!(format!("{:?}", ve.get(name)), format!("{:?}", ve.get(&translated)));
            old_keys.insert(name.to_string());
        }
    }
    for line in logger.get_log().lines() {
        if let Some((old, _)) = deprecation_warning(line) {
            assert!(old_keys.remove(&old), "unexpected warning for {old}");
        }
    }
    assert!(old_keys.is_empty(), "No warning detected for the following properties: {old_keys:?}");
    velocity::props::set_deprecation_logger(Arc::new(velocity::log::NopLogger("org.apache.velocity.deprecation".into())));
}

#[test]
fn test_new_properties() {
    let logger = Arc::new(TestLogger::new(false, true));
    logger.set_enabled_level(LOG_LEVEL_WARN);
    velocity::props::set_deprecation_logger(logger.clone());
    logger.on();
    Engine::with_properties(&ExtProperties::new()).unwrap();
    logger.off();
    for line in logger.get_log().lines() {
        if let Some((old, new)) = deprecation_warning(line) {
            panic!("Default properties contain deprecated property '{old}', deprecated in favor of '{new}'");
        }
    }
    velocity::props::set_deprecation_logger(Arc::new(velocity::log::NopLogger("org.apache.velocity.deprecation".into())));
}

#[test]
fn test_immutable_range() {
    let mut b = Base::new();
    let e = b.assert_eval_exception("#set($range = [1..2])#set($range[0] = 4)", Some("org.apache.velocity.exception.MethodInvocationException"));
    assert_eq!(e.into_data().cause.map(|c| c.class).as_deref(), Some("java.lang.UnsupportedOperationException"));
}

#[test]
fn test_mutable_range() {
    let mut b = Base::new();
    b.set_property("runtime.immutable_ranges", false);
    b.assert_eval_equals("4", "#set($range = [1..2])#set($range[0] = 4)$range[0]");
}

const TEMPLATE_STRING: &str = "#macro (showhelloworld $foo)\nHello, World\n#end\n\n<html>\n<head>\n<title>page title</title>\n</head>\n<body>\n<p>This is a test</p>\n<p>#showhelloworld ($foo)</p>\n</body>\n</html>";

/// `RenderVelocityTemplate`: `Velocity.init(p)` once (the singleton), with inline macros local to their template.
fn render_engine() -> &'static Engine {
    static ENGINE: std::sync::LazyLock<Engine> = std::sync::LazyLock::new(|| {
        let mut p = ExtProperties::new();
        p.set_property("velocimacro.permissions.allow.inline.local.scope", "true".into());
        Engine::with_properties(&p).unwrap()
    });
    &ENGINE
}

fn multiple_evals() {
    let mut context = Context::new();
    for _ in 0..1000 {
        let result = render_engine().evaluate(&mut context, "", TEMPLATE_STRING).unwrap();
        assert!(!result.contains("#showhelloworld"), "Failed to substitute macro:\n{result}");
        assert!(result.contains("<p>Hello, World"));
    }
}

#[test]
fn test_multiple_evals() {
    multiple_evals();
}

#[test]
fn test_multi_thread_multiple_evals() {
    let threads: Vec<_> = (0..4).map(|_| std::thread::spawn(multiple_evals)).collect();
    let errors = threads.into_iter().filter_map(|t| t.join().err()).count();
    assert_eq!(errors, 0, "Failed {errors} out of 4 template evaluations");
}

#[test]
fn test_velocity_app() {
    let mut b = Base::new();
    b.set_property("resource.loader.file.path", resource("templates"));
    b.engine();
    let mut context = Context::new();
    context.put("name", s("jason"));
    context.put("Floog", s("floogie woogie"));
    let cmp = "Hello jason! Nice floogie woogie!";
    assert_eq!(b.engine().evaluate(&mut context, "evaltest", "My name is $name -> $Floog").unwrap(), "My name is jason -> floogie woogie", "Output 1 incorrect.");
    assert_eq!(b.engine().merge_template("mergethis.vm", Some("UTF-8"), &mut context).unwrap(), cmp, "Output 2 incorrect.");
    assert_eq!(b.engine().invoke_velocimacro("floog", Some("test"), &["name", "Floog"], &mut context).unwrap(), cmp, "Output 3 incorrect.");
}

#[test]
fn test_get_file_name() {
    let pwd = std::env::current_dir().unwrap().to_string_lossy().into_owned();
    let root = "/";
    assert_eq!(get_file_name(Some("baz"), "foo", "bar"), format!("{pwd}/baz/foo.bar"));
    assert_eq!(get_file_name(Some(&format!("{root}baz")), "foo", "bar"), format!("{root}baz/foo.bar"));
    assert_eq!(get_file_name(Some("baz"), &format!("{root}foo"), "bar"), format!("{root}foo.bar"));
    assert_eq!(get_file_name(Some(&format!("{root}baz")), &format!("{root}foo"), "bar"), format!("{root}foo.bar"));
    assert_eq!(get_file_name(None, "", ""), "");
    assert_eq!(get_file_name(Some(""), "", ""), root);
    assert_eq!(get_file_name(None, "", "x"), ".x");
    assert_eq!(get_file_name(Some(""), "", "x"), format!("{root}.x"));
    assert_eq!(get_file_name(None, "foo", "bar"), "foo.bar");
    assert_eq!(get_file_name(None, &format!("{root}foo"), "bar"), format!("{root}foo.bar"));
    assert_eq!(get_file_name(Some(""), "foo", "bar"), format!("{root}foo.bar"));
    assert_eq!(get_file_name(Some(""), &format!("{root}foo"), "bar"), format!("{root}foo.bar"));
}

