//! Render speed of a counting `#foreach`. Ignored: run with
//! `cargo test -p velocity --release --test render_speed -- --ignored --nocapture`.

use std::time::Instant;

fn time(label: &str, template: &str) {
    let engine = velocity::Engine::mockserver();
    let started = Instant::now();
    let out = engine.render(template, &mut velocity::Context::new()).expect("renders");
    let elapsed = started.elapsed();
    println!("{label:<34} {:>9.1} ms  out={}", elapsed.as_secs_f64() * 1000.0, out.trim().chars().take(12).collect::<String>());
}

#[test]
#[ignore]
fn counting_foreach() {
    for n in [1_000_000u32, 3_000_000, 10_000_000] {
        time(&format!("set n=n+1 x{n}"), &format!("#set($n = 0)#foreach($i in [1..{n}])#set($n = $n + 1)#end$n"));
    }
    let n = 3_000_000;
    time("empty body", &format!("#foreach($i in [1..{n}])#end"));
    time("set n=1 (no arithmetic)", &format!("#set($n = 0)#foreach($i in [1..{n}])#set($n = 1)#end$n"));
    time("set n=n+1", &format!("#set($n = 0)#foreach($i in [1..{n}])#set($n = $n + 1)#end$n"));
    time("two sets", &format!("#set($n = 0)#set($m = 0)#foreach($i in [1..{n}])#set($n = $n + 1)#set($m = $m + 2)#end$n"));
    time("text body", &format!("#foreach($i in [1..{n}])x#end"));
    time("$i output", &format!("#foreach($i in [1..{n}])$i#end"));
}
