//! `CommentsTestCase`: a fresh default engine per test.

use velocity::{Context, Engine};

fn eval(t: &str) -> String {
    Engine::new().evaluate(&mut Context::new(), "test", t).unwrap()
}

#[test]
fn test_multi_line() {
    assert_eq!(eval("abc #* test\r\ntest2*#\r\ndef"), "abc \r\ndef");
}

#[test]
fn test_single_line() {
    assert_eq!(eval("123 ## test test\r\nabc"), "123 abc");
    assert_eq!(eval("123 \r\n## test test\r\nabc"), "123 \r\nabc");
}

#[test]
fn test_combined() {
    assert_eq!(eval("test\r\n## #* *# ${user \r\nabc"), "test\r\nabc");
}
