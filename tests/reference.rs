//! Replays `tests/cases/*.json` and compares with `*.expected.json`, which `reference/reference.sh` records from
//! the real Velocity 2.4.1 (and velocity-tools 3.1) configured as MockServer configures them. A case that fails
//! upstream must fail here too; the error text is not compared.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use java_compat::lang::{JList, JMap, JValue, MapKind};
use serde_json::Value;

/// JSON as `Reference.java` converts it: integers to `Integer`/`Long`, fractions to `Double`.
fn java(v: &Value) -> JValue {
    match v {
        Value::Null => JValue::Null,
        Value::Bool(b) => JValue::Bool(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) if i32::try_from(i).is_ok() => JValue::Int(i as i32),
            Some(i) => JValue::Long(i),
            None => JValue::Double(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => JValue::str(s.as_str()),
        Value::Array(a) => JValue::List(JList::array_list(a.iter().map(java).collect())),
        Value::Object(o) => JValue::Map(JMap::from_entries(MapKind::Linked, o.iter().map(|(k, v)| (JValue::str(k.as_str()), java(v))))),
    }
}

#[test]
fn matches_velocity() {
    // MockServer's trimmed JDK has no `jdk.localedata`: non-English locales format with root data.
    velocity::tools::set_locale_data(java_compat::text::LocaleData::JavaBase);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let filter = std::env::var("VELOCITY_CASE").ok();
    let mut failures = Vec::new();
    let mut total = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".json") || name.ends_with(".expected.json") {
            continue;
        }
        let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let expected: Vec<Value> = serde_json::from_str(&fs::read_to_string(path.with_extension("expected.json")).unwrap()).unwrap();
        let engine = velocity::Engine::mockserver();
        for (case, want) in cases.iter().zip(&expected) {
            let template = case["template"].as_str().unwrap();
            if filter.as_ref().is_some_and(|f| !template.contains(f.as_str())) {
                continue;
            }
            total += 1;
            let vars: HashMap<String, JValue> = case["context"].as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), java(v))).collect()).unwrap_or_default();
            let mut context = velocity::Context::new().with_layer(Arc::new(vars)).with_layer(velocity::tools::context());
            let got = engine.render(template, &mut context);
            let ok = match (&got, want.get("output")) {
                (Ok(out), Some(Value::String(w))) => out == w,
                (Err(_), None) => true,
                _ => false,
            };
            if !ok {
                failures.push(format!(
                    "{name}: {template:?}\n   want {}\n   got  {:?}",
                    want.get("output").map(|v| format!("{v}")).unwrap_or_else(|| format!("error {}", want["error"])),
                    got
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{} of {total} cases differ:\n{}", failures.len(), failures.join("\n"));
}
