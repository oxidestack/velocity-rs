//! `SpaceGobblingTestCase`: every template of `gobbling/` against its compare file, in every mode.

use crate::base::*;

const MODES: [&str; 4] = ["NONE", "BC", "LINES", "STRUCTURED"];

#[test]
fn test_space_gobbling() {
    let dir = resources().join("gobbling");
    let mut templates: Vec<String> = std::fs::read_dir(&dir)
        .expect("cannot read input templates")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    templates.sort();
    let mut failures = Vec::new();
    for mode in MODES {
        for t in &templates {
            let mut b = Base::new();
            b.set_property("resource.loaders", "file");
            b.set_property("resource.loader.file.path", dir.to_string_lossy().as_ref());
            b.set_property("space.gobbling", mode);
            let result = match b.merge(t) {
                Ok(o) => o,
                Err(e) => format!("{} threw {}", t, e.message()),
            };
            let compare = file_contents(&resource(&format!("gobbling/compare/{t}.{mode}")));
            if normalize_newlines(&result) != normalize_newlines(&compare) {
                failures.push(format!(
                    "Processed template did not match expected output for template {t} and mode {mode}\n-----Result-----\n{result}----Expected----\n{compare}----------------"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
