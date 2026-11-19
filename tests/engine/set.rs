//! `SetTestCase`.

use crate::base::*;

#[test]
fn test_set_null() {
    let mut b = Base::new();
    b.add_property("resource.loader.file.path", resource("set"));
    for t in ["set1", "set2"] {
        b.assert_matches_compare(&format!("{t}.vm"), &resource(&format!("set/compare/{t}.cmp")));
    }
}

#[test]
fn test_invalid_set() {
    let mut b = Base::new();
    for t in [
        "#set($c = $a - $b - $c)",
        "#set($c = $a + $b + $c)",
        "#set($c = $a * $b * $c)",
        "#set($c = $a / $b / $c)",
        "#set($c = $a % $b % $c)",
        "#set($c = $a && $b && $c)",
        "#set($c = $a || $b || $c)",
        "#set($c = $a + $b + !$c)",
        "#set($c = $a + $b + (-$c))",
        "#set($c = $a && ($b < $c))",
        "#set($c = !$a)",
        "#set($c = ($a < $b) - ($c < $d))",
        "#set($b = !$a)",
    ] {
        b.assert_eval_equals("", t);
    }
}
