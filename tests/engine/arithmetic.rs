//! `ArithmeticTestCase`: `MathUtils` result values and types.

use java_compat::lang::JValue;
use velocity::math;

use crate::base::*;

fn check(result: JValue, expected: f64, class: &str) {
    assert!((math::double_value(&result) - expected).abs() <= 0.01, "{result:?} != {expected}");
    assert_eq!(result.class_name(), class, "ResultType does not match: {result:?}");
}

#[test]
fn test_add() {
    check(math::add(&JValue::Int(10), &short(20)), 30.0, "java.lang.Integer");
    check(math::add(&byte(10), &short(20)), 30.0, "java.lang.Short");
    check(math::add(&JValue::Float(10.0), &short(20)), 30.0, "java.lang.Float");
    check(math::add(&byte(10), &JValue::Double(20.0)), 30.0, "java.lang.Double");
    check(math::add(&big_integer(10), &JValue::Int(20)), 30.0, "java.math.BigInteger");
    check(math::add(&JValue::Int(20), &big_decimal_of(10.0)), 30.0, "java.math.BigDecimal");
    check(math::add(&JValue::Int(i32::MAX), &short(20)), i32::MAX as f64 + 20.0, "java.lang.Long");
    check(math::add(&JValue::Int(20), &JValue::Long(i64::MAX)), i64::MAX as f64 + 20.0, "java.math.BigInteger");
    check(math::add(&JValue::Int(-20), &JValue::Long(i64::MIN)), i64::MIN as f64 - 20.0, "java.math.BigInteger");
}

#[test]
fn test_subtract() {
    check(math::subtract(&JValue::Int(100), &short(20)), 80.0, "java.lang.Integer");
    check(math::subtract(&byte(100), &short(20)), 80.0, "java.lang.Short");
    check(math::subtract(&JValue::Float(100.0), &short(20)), 80.0, "java.lang.Float");
    check(math::subtract(&byte(100), &JValue::Double(20.0)), 80.0, "java.lang.Double");
    check(math::subtract(&big_integer(100), &JValue::Int(20)), 80.0, "java.math.BigInteger");
    check(math::subtract(&JValue::Int(100), &big_decimal_of(20.0)), 80.0, "java.math.BigDecimal");
    check(math::subtract(&JValue::Int(i32::MIN), &short(20)), i32::MIN as f64 - 20.0, "java.lang.Long");
    check(math::subtract(&JValue::Int(-20), &JValue::Long(i64::MAX)), -20.0 - i64::MAX as f64, "java.math.BigInteger");
    check(math::subtract(&JValue::Int(i32::MAX), &JValue::Long(i64::MIN)), i64::MAX as f64 + i32::MAX as f64, "java.math.BigInteger");
}

#[test]
fn test_multiply() {
    check(math::multiply(&JValue::Int(10), &short(20)), 200.0, "java.lang.Integer");
    check(math::multiply(&byte(100), &short(20)), 2000.0, "java.lang.Short");
    check(math::multiply(&byte(100), &short(2000)), 200000.0, "java.lang.Integer");
    check(math::multiply(&JValue::Float(100.0), &short(20)), 2000.0, "java.lang.Float");
    check(math::multiply(&byte(100), &JValue::Double(20.0)), 2000.0, "java.lang.Double");
    check(math::multiply(&big_integer(100), &JValue::Int(20)), 2000.0, "java.math.BigInteger");
    check(math::multiply(&JValue::Int(100), &big_decimal_of(20.0)), 2000.0, "java.math.BigDecimal");
    check(math::multiply(&JValue::Int(i32::MAX), &short(10)), i32::MAX as f64 * 10.0, "java.lang.Long");
    check(math::multiply(&JValue::Int(i32::MAX), &short(-10)), i32::MAX as f64 * -10.0, "java.lang.Long");
    check(math::multiply(&JValue::Int(20), &JValue::Long(i64::MAX)), 20.0 * i64::MAX as f64, "java.math.BigInteger");
}

#[test]
fn test_divide() {
    check(math::divide(&JValue::Int(10), &short(2)), 5.0, "java.lang.Integer");
    check(math::divide(&byte(10), &short(2)), 5.0, "java.lang.Short");
    check(math::divide(&big_integer(10), &short(2)), 5.0, "java.math.BigInteger");
    check(math::divide(&JValue::Int(10), &short(4)), 2.0, "java.lang.Integer");
    check(math::divide(&JValue::Int(10), &JValue::Float(2.5)), 4.0, "java.lang.Float");
    check(math::divide(&JValue::Int(10), &JValue::Double(2.5)), 4.0, "java.lang.Double");
    // `new BigDecimal(2.5)`: exact binary value, 2.5.
    check(math::divide(&JValue::Int(10), &big_decimal("2.5")), 4.0, "java.math.BigDecimal");
}

#[test]
fn test_modulo() {
    check(math::modulo(&JValue::Int(10), &short(2)).unwrap(), 0.0, "java.lang.Integer");
    check(math::modulo(&byte(10), &short(3)).unwrap(), 1.0, "java.lang.Short");
    check(math::modulo(&big_integer(10), &short(4)).unwrap(), 2.0, "java.math.BigInteger");
    check(math::modulo(&JValue::Int(10), &JValue::Float(5.5)).unwrap(), 4.5, "java.lang.Float");
    assert!(math::modulo(&JValue::Int(10), &big_decimal("2.5")).is_err(), "Modulo with BigDecimal is not allowed");
}

#[test]
fn test_compare() {
    use std::cmp::Ordering::*;
    assert_eq!(math::compare(&JValue::Int(10), &short(10)), Equal);
    assert_eq!(math::compare(&JValue::Int(10), &short(11)), Less);
    assert_eq!(math::compare(&big_integer(10), &short(11)), Less);
    assert_eq!(math::compare(&byte(10), &short(3)), Greater);
    assert_eq!(math::compare(&JValue::Float(10.0), &short(11)), Less);
    assert_eq!(math::compare(&JValue::Double(10.0), &short(11)), Less);
}

#[test]
fn test_negate() {
    check(math::negate(&byte(1)), -1.0, "java.lang.Byte");
    check(math::negate(&short(1)), -1.0, "java.lang.Short");
    check(math::negate(&JValue::Int(1)), -1.0, "java.lang.Integer");
    check(math::negate(&JValue::Long(1)), -1.0, "java.lang.Long");
    check(math::negate(&big_integer(1)), -1.0, "java.math.BigInteger");
    check(math::negate(&big_decimal_of(1.0)), -1.0, "java.math.BigDecimal");
    check(math::negate(&JValue::Long(i64::MIN)), 9223372036854775808.0, "java.math.BigInteger");
}

#[test]
fn test_is_zero() {
    assert!(math::is_zero(&JValue::Int(0)));
    assert!(!math::is_zero(&JValue::Int(1)));
    assert!(!math::is_zero(&JValue::Int(-1)));
    assert!(math::is_zero(&JValue::Float(0.0)));
    assert!(!math::is_zero(&JValue::Float(0.00001)));
    assert!(!math::is_zero(&JValue::Float(-0.00001)));
}
