//! `java.lang.Number`'s conversions (`intValue`, `longValue`, `doubleValue`) over the numeric [`JValue`]s, as the
//! tools apply them to whatever number a template or a parser hands them.

use java_compat::lang::JValue;
use num_bigint::BigInt;
use num_traits::ToPrimitive;

/// `obj instanceof Number`.
pub fn is_number(v: &JValue) -> bool {
    crate::math::is_number(v)
}

fn big_integer(v: &JValue) -> Option<BigInt> {
    match v {
        JValue::BigInteger(s) => s.parse().ok(),
        JValue::BigDecimal(s) => crate::math::Decimal::parse(s).map(|d| d.to_big_int()),
        _ => None,
    }
}

/// The low 64 bits of a `BigInteger`, two's complement.
fn low_bits(b: &BigInt) -> i64 {
    let m: BigInt = b & BigInt::from(u64::MAX);
    m.to_u64().unwrap_or(0) as i64
}

/// `Number.longValue()`: floating values saturate (`(long) d`), big values keep their low 64 bits.
pub fn long_value(v: &JValue) -> i64 {
    match v {
        JValue::Byte(b) => *b as i64,
        JValue::Short(s) => *s as i64,
        JValue::Int(i) => *i as i64,
        JValue::Long(l) => *l,
        JValue::Char(c) => *c as i64,
        JValue::Float(f) => *f as i64,
        JValue::Double(d) => *d as i64,
        other => big_integer(other).map(|b| low_bits(&b)).unwrap_or(0),
    }
}

/// `Number.intValue()`: `(int) d` saturates for floating values, integral values keep their low 32 bits.
pub fn int_value(v: &JValue) -> i32 {
    match v {
        JValue::Float(f) => *f as i32,
        JValue::Double(d) => *d as i32,
        other => long_value(other) as i32,
    }
}

/// `Number.doubleValue()`.
pub fn double_value(v: &JValue) -> f64 {
    crate::math::double_value(v)
}

/// `Math.rint`: round half to even.
pub fn rint(d: f64) -> f64 {
    d.round_ties_even()
}

/// `Math.round(double)`: the closest `long`, ties rounding up, saturating; `NaN` is 0.
pub fn round(d: f64) -> i64 {
    if d.is_nan() {
        return 0;
    }
    let f = d.floor();
    (if d - f >= 0.5 { f + 1.0 } else { f }) as i64
}
