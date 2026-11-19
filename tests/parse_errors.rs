//! Parse-error messages byte for byte: `tests/cases/parse_errors.json` holds malformed templates and
//! `parse_errors.expected.json` the message Velocity 2.4.1 raises for each (`generators/record_parse_errors.py`
//! records them with `reference/reference.sh`). The messages list JavaCC's expected-token sets, which come out of the
//! generated parser's `jj_la1` bookkeeping and look-ahead rescans.
//!
//! `VELOCITY_PARSE_ERRORS=<cases.json>:<expected.json>` replays another recorded set (the differential fuzz corpus);
//! `VELOCITY_PARSE_ERRORS_SHOW=<n>` prints up to n differences (default 5); `VELOCITY_PARSE_ERRORS_DUMP=<file>` writes
//! all of them there as JSON.

use std::fs;
use std::path::Path;

use serde_json::Value;

fn load(path: &Path) -> Vec<Value> {
    serde_json::from_str(&fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

/// The message without the echoed template text: its first and last lines, then the expected list.
fn tail(message: &str) -> String {
    let lines: Vec<&str> = message.lines().collect();
    let at = lines.iter().position(|l| l.starts_with("Was expecting")).unwrap_or(lines.len());
    let head = lines.first().map(|l| l.chars().take(60).collect::<String>()).unwrap_or_default();
    let location = lines[..at].last().copied().unwrap_or_default();
    format!("{head} ... {location}\n{}", lines[at..].join("\n"))
}

#[test]
fn parse_error_messages_match_velocity() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let (cases, expected) = match std::env::var("VELOCITY_PARSE_ERRORS") {
        Ok(spec) => {
            let (a, b) = spec.split_once(':').expect("cases.json:expected.json");
            (load(Path::new(a)), load(Path::new(b)))
        }
        Err(_) => (load(&dir.join("parse_errors.json")), load(&dir.join("parse_errors.expected.json"))),
    };
    let show: usize = std::env::var("VELOCITY_PARSE_ERRORS_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(5);
    let mut failures = Vec::new();
    let mut dump = Vec::new();
    let mut compared = 0;
    for (case, want) in cases.iter().zip(&expected) {
        let template = case["template"].as_str().unwrap();
        let Some(want) = want.get("error").and_then(Value::as_str) else { continue };
        if !want.starts_with("org.apache.velocity.exception.ParseErrorException") {
            continue;
        }
        compared += 1;
        let before = velocity::jj_fallbacks();
        // A fresh engine per template: macros one defines are not known to the next, as when they were recorded.
        let engine = velocity::Engine::mockserver();
        let mut context = velocity::Context::new();
        let got = match engine.evaluate(&mut context, template, template) {
            Ok(out) => format!("rendered {out:?}"),
            Err(e) => format!("{}: {}", e.class_name(), e.message()),
        };
        if velocity::jj_fallbacks() != before {
            eprintln!("fallback: {template:?}");
        }
        if got != want {
            dump.push(serde_json::json!({"template": template, "want": want, "got": got}));
            let short: String = template.chars().take(100).collect();
            failures.push(format!("template {short:?}\n--- want\n{}\n--- got\n{}\n", tail(want), tail(&got)));
        }
    }
    if let Ok(path) = std::env::var("VELOCITY_PARSE_ERRORS_DUMP") {
        fs::write(path, serde_json::to_string(&dump).unwrap()).unwrap();
    }
    assert_eq!(velocity::jj_fallbacks(), 0, "parse failures whose expected tokens could not be rebuilt");
    assert!(failures.is_empty(), "{} of {compared} messages differ:\n{}", failures.len(), failures.iter().take(show).cloned().collect::<Vec<_>>().join("\n"));
}
