//! `org.apache.velocity.tools.generic.MathToolTests`.

use velocity::tools::math_tool::MathTool;

use crate::base::Base;

fn setup() -> Base {
    let mut b = Base::new();
    b.put("math", MathTool::new().value());
    b
}

#[test]
fn test_base() {
    setup().test_base();
}

#[test]
fn test_max() {
    let mut b = setup();
    b.assert_eval_equals("4.0", "$math.max(4,3.5)");
    b.assert_eval_equals("4.0", "$math.max(4,3.5,3)");
}
