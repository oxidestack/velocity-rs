//! The render deadline (`velocity::deadline`, MockServer's `velocityTemplateExecutionTimeout`): a runaway `#foreach`
//! over `[1..2147483647]` is iterated without being materialised and stops at the first check after the deadline.

use std::time::{Duration, Instant};

use java_compat::lang::JError;

fn render(template: &str) -> Result<String, velocity::VelocityError> {
    let engine = velocity::Engine::mockserver();
    engine.render(template, &mut velocity::Context::new())
}

#[test]
fn a_runaway_loop_stops_at_the_deadline() {
    let _deadline = velocity::deadline::start(Some(Duration::from_millis(100)), JError::new("example.Timeout", "too slow"));
    let started = Instant::now();
    let failed = render("#foreach($i in [1..2147483647])#foreach($j in [1..2147483647])#end#end").expect_err("stopped");
    assert_eq!(failed.into_data().cause.map(|c| c.class), Some("example.Timeout".to_string()));
    assert!(started.elapsed() < Duration::from_secs(10), "{:?}", started.elapsed());
}

#[test]
fn a_recursive_macro_stops_at_the_deadline() {
    let _deadline = velocity::deadline::start(Some(Duration::ZERO), JError::new("example.Timeout", "too slow"));
    let failed = render("#macro(m $n)#foreach($i in [1..3])x#end#m($n)#end#m(1)").expect_err("stopped");
    assert_eq!(failed.into_data().cause.map(|c| c.class), Some("example.Timeout".to_string()));
}

#[test]
fn a_huge_range_iterates_lazily() {
    assert_eq!(render("#foreach($i in [2147483640..2147483647])$i,#end#foreach($i in [3..1])$i#end").unwrap(), "2147483640,2147483641,2147483642,2147483643,2147483644,2147483645,2147483646,2147483647,321");
}
