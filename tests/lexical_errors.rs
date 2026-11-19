//! Lexical-error messages byte for byte: `tests/cases/lexical_errors.json` holds templates the lexer cannot read and
//! `lexical_errors.expected.json` what Velocity 2.4.1 raises for each, through `evaluate` (`at *unset*[...]`, or the
//! raw `TokenMgrError` text inside `#evaluate`) and through `getTemplate` (the template named). Recorded by
//! `generators/record_lexical_errors.py`.

use std::fs;
use std::path::Path;

use serde_json::Value;

fn load(path: &Path) -> Vec<Value> {
    serde_json::from_str(&fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

fn outcome(result: Result<String, velocity::VelocityError>) -> Option<String> {
    match result {
        Ok(_) => None,
        Err(e) => Some(format!("{}: {}", e.class_name(), e.message())),
    }
}

/// `getTemplate(template)` from a string repository holding the template under its own text, then `merge`.
fn via_get_template(template: &str) -> Result<String, velocity::VelocityError> {
    let mut p = velocity::config::mockserver_properties();
    p.set_property(velocity::config::CUSTOM_DIRECTIVES, velocity::IFNULL_DIRECTIVE.into());
    for (k, v) in [("resource.loader.string.class", velocity::resource::STRING_LOADER), ("resource.loader.string.repository.static", "true"), ("resource.loader.string.repository.name", "lexical-errors")] {
        p.set_property(k, v.into());
    }
    let engine = velocity::Engine::with_properties(&p)?;
    velocity::resource::static_repository("lexical-errors").unwrap().put_with_encoding(template, template, "UTF-8");
    let loaded = engine.get_template(template, Some("UTF-8"))?;
    engine.merge(&loaded, &mut velocity::Context::new())
}

#[test]
fn lexical_error_messages_match_velocity() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let cases = load(&dir.join("lexical_errors.json"));
    let expected = load(&dir.join("lexical_errors.expected.json"));
    assert_eq!(cases.len(), expected.len());
    let mut failures = Vec::new();
    for (case, want) in cases.iter().zip(&expected) {
        let template = case["template"].as_str().unwrap();
        // A fresh engine per template, as when they were recorded.
        let engine = velocity::Engine::mockserver();
        let got = outcome(engine.evaluate(&mut velocity::Context::new(), template, template));
        if got.as_deref() != want["evaluate"].as_str() {
            failures.push(format!("evaluate {template:?}\n--- want\n{}\n--- got\n{got:?}\n", want["evaluate"]));
        }
        let got = outcome(via_get_template(template));
        if got.as_deref() != want["template"].as_str() {
            failures.push(format!("getTemplate {template:?}\n--- want\n{}\n--- got\n{got:?}\n", want["template"]));
        }
    }
    assert!(failures.is_empty(), "{} of {} messages differ:\n{}", failures.len(), cases.len() * 2, failures.iter().take(8).cloned().collect::<Vec<_>>().join("\n"));
}
