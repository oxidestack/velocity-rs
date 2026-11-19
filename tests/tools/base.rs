//! velocity-tools' `BaseTestCase` (a file + string resource engine and its assertions), the test class path,
//! and helpers for the Java values the tests build.

#![allow(dead_code)]

use java_compat::lang::{JArray, JList, JMap, JValue, MapKind};
use java_compat::reflect::JType;
use velocity::props::ExtProperties;
use velocity::{Context, Engine, VelocityError};

/// The test resources (`src/test/resources`) on the class path, as Maven's surefire has them.
pub fn classpath() {
    velocity::tools::classpath::add_directory(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/tools/resources"));
}

pub fn s(v: &str) -> JValue {
    JValue::str(v)
}

pub fn i(v: i32) -> JValue {
    JValue::Int(v)
}

/// A `HashMap<String, Object>`.
pub fn map(entries: &[(&str, JValue)]) -> JMap {
    let m = JMap::new(MapKind::Hash);
    for (k, v) in entries {
        m.put_str(k, v.clone());
    }
    m
}

/// A `LinkedHashMap`.
pub fn linked(entries: &[(JValue, JValue)]) -> JMap {
    JMap::from_entries(MapKind::Linked, entries.iter().cloned())
}

/// A `String[]`.
pub fn strings(items: &[&str]) -> JValue {
    JArray::strings(items.iter().map(|s| s.to_string()))
}

/// An `int[]`.
pub fn ints(items: &[i32]) -> JValue {
    JValue::Array(std::sync::Arc::new(JArray::new(JType::Int, items.iter().map(|i| JValue::Int(*i)).collect())))
}

/// An `ArrayList`.
pub fn list(items: &[JValue]) -> JValue {
    JValue::List(JList::array_list(items.to_vec()))
}

/// `BaseTestCase`: an engine reading templates from files and from the `string.repo` repository.
pub struct Base {
    engine: Engine,
    pub context: Context,
}

impl Base {
    pub fn new() -> Base {
        let mut props = ExtProperties::new();
        props.set_property("resource.loaders", "file,string".into());
        props.add_property("string.resource.loader.class", velocity::resource::STRING_LOADER.into());
        props.add_property("string.resource.loader.repository.name", "string.repo".into());
        props.add_property("string.resource.loader.repository.static", "false".into());
        Base { engine: Engine::with_properties(&props).expect("engine initialises"), context: Context::new() }
    }

    pub fn put(&mut self, key: &str, value: JValue) {
        self.context.put(key, value);
    }

    /// `evaluate(template)`: the template (or its first 15 characters) as its own name.
    pub fn evaluate(&mut self, vtl: &str) -> Result<String, VelocityError> {
        let units: Vec<u16> = vtl.encode_utf16().collect();
        let tag = if units.len() <= 15 { vtl.to_string() } else { String::from_utf16_lossy(&units[..15]) };
        self.engine.evaluate(&mut self.context, &tag, vtl)
    }

    #[track_caller]
    pub fn assert_eval_equals(&mut self, expected: &str, vtl: &str) {
        match self.evaluate(vtl) {
            Ok(out) => assert_eq!(out, expected, "template {vtl:?}"),
            Err(e) => panic!("template {vtl:?} threw {} ({})", e.class_name(), e.message()),
        }
    }

    #[track_caller]
    pub fn assert_schmoo(&mut self, template: &str) {
        self.assert_eval_equals(template, template);
    }

    /// `testBase()`: with `DEBUG` off (the default) it asserts nothing; the schmoo checks it would make hold.
    pub fn test_base(&mut self) {
        self.assert_schmoo("");
        self.assert_schmoo("abc\n123");
    }
}
