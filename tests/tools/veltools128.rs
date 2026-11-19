//! `org.apache.velocity.tools.generic.Veltools128Tests` (VELOCITY-762: `LoopTool.sync`).

use velocity::tools::loop_tool::LoopTool;

use crate::base::Base;

fn setup() -> Base {
    let mut b = Base::new();
    b.put("loop", LoopTool::new().value());
    b
}

#[test]
fn test_base() {
    setup().test_base();
}

#[test]
fn test_loop_tool_sync() {
    let template = "#foreach( $item in $loop.watch([1..3]).sync([3..5], 'other') )$item:$loop.other #end";
    setup().assert_eval_equals("1:3 2:4 3:5 ", template);
}
