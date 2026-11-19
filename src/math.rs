//! `org.apache.velocity.runtime.parser.node.MathUtils`, with the `java.math.BigDecimal` / `BigInteger`
//! arithmetic it falls back to.

use std::cmp::Ordering;

use java_compat::lang::JValue;
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{Signed, ToPrimitive, Zero};

/// A `java.math.BigDecimal`: `unscaled × 10^-scale`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Decimal {
    /// The digits, as an integer.
    pub unscaled: BigInt,
    /// The number of digits after the decimal point; negative for multiples of a power of ten.
    pub scale: i64,
}

impl Decimal {
    /// `new BigDecimal(String)`: `None` on a `NumberFormatException`.
    pub fn parse(text: &str) -> Option<Decimal> {
        let (negative, rest) = match text.as_bytes().first()? {
            b'-' => (true, &text[1..]),
            b'+' => (false, &text[1..]),
            _ => (false, text),
        };
        let (mantissa, exponent) = match rest.find(['e', 'E']) {
            Some(i) => (&rest[..i], rest[i + 1..].parse::<i64>().ok()?),
            None => (rest, 0),
        };
        let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        if int.is_empty() && frac.is_empty() || !int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit()) {
            return None;
        }
        let mut unscaled: BigInt = format!("{int}{frac}").parse().ok()?;
        if negative {
            unscaled = -unscaled;
        }
        Some(Decimal { unscaled, scale: frac.len() as i64 - exponent })
    }

    /// `new BigDecimal(String)`.
    pub fn parse_java(text: &str) -> Option<Decimal> {
        Decimal::parse(text)
    }

    /// Whether the value has no fractional part.
    pub fn is_integral(&self) -> bool {
        self.scale <= 0 || (&self.unscaled % BigInt::from(10).pow(self.scale as u32)).is_zero()
    }

    /// `new BigDecimal(BigInteger)`: the integer at scale 0.
    pub fn from_int(i: BigInt) -> Decimal {
        Decimal { unscaled: i, scale: 0 }
    }

    /// `BigDecimal.valueOf(double)`: through `Double.toString`.
    pub fn from_f64(d: f64) -> Decimal {
        Decimal::parse(&java_compat::double_to_string(d)).unwrap_or(Decimal { unscaled: BigInt::zero(), scale: 0 })
    }

    fn rescale(&self, scale: i64) -> BigInt {
        if scale >= self.scale { &self.unscaled * BigInt::from(10).pow((scale - self.scale) as u32) } else { self.unscaled.clone() }
    }

    /// `add(o)`: the exact sum, at the larger of the two scales.
    pub fn add(&self, o: &Decimal) -> Decimal {
        let s = self.scale.max(o.scale);
        Decimal { unscaled: self.rescale(s) + o.rescale(s), scale: s }
    }

    /// `subtract(o)`: the exact difference, at the larger of the two scales.
    pub fn sub(&self, o: &Decimal) -> Decimal {
        let s = self.scale.max(o.scale);
        Decimal { unscaled: self.rescale(s) - o.rescale(s), scale: s }
    }

    /// `multiply(o)`: the exact product, its scale the sum of the two.
    pub fn mul(&self, o: &Decimal) -> Decimal {
        Decimal { unscaled: &self.unscaled * &o.unscaled, scale: self.scale + o.scale }
    }

    /// `divide(divisor, ROUND_HALF_DOWN)`: the quotient at this number's scale.
    pub fn divide_half_down(&self, o: &Decimal) -> Decimal {
        // r = u1 × 10^(s2 - s1 + s1) / u2, at scale s1.
        let mut num = self.unscaled.clone();
        let mut den = o.unscaled.clone();
        if o.scale >= 0 {
            num *= BigInt::from(10).pow(o.scale as u32);
        } else {
            den *= BigInt::from(10).pow((-o.scale) as u32);
        }
        let (q, r) = num.div_rem(&den);
        let twice = (r.abs() * BigInt::from(2)).cmp(&den.abs());
        let away = twice == Ordering::Greater;
        let negative = num.is_negative() != den.is_negative();
        let q = if away { if negative { q - 1 } else { q + 1 } } else { q };
        Decimal { unscaled: q, scale: self.scale }
    }

    /// `negate()`.
    pub fn negate(&self) -> Decimal {
        Decimal { unscaled: -self.unscaled.clone(), scale: self.scale }
    }

    /// `compareTo(o)`: by value, ignoring scale (`2.0` equals `2.00`).
    pub fn compare(&self, o: &Decimal) -> Ordering {
        let s = self.scale.max(o.scale);
        self.rescale(s).cmp(&o.rescale(s))
    }

    /// `signum() == 0`.
    pub fn is_zero(&self) -> bool {
        self.unscaled.is_zero()
    }

    /// `doubleValue()`: the nearest `f64`, through the decimal string.
    pub fn to_f64(&self) -> f64 {
        java_string(self).parse().unwrap_or(0.0)
    }

    /// `toBigInteger()`: truncation towards zero.
    pub fn to_big_int(&self) -> BigInt {
        if self.scale <= 0 {
            self.rescale(0)
        } else {
            let (q, _) = self.unscaled.div_rem(&BigInt::from(10).pow(self.scale as u32));
            q
        }
    }
}

/// `BigDecimal.toString()`.
pub fn java_string(d: &Decimal) -> String {
    java_compat::big_decimal_to_string(&format!("{}E{}", d.unscaled, -d.scale)).unwrap_or_else(|| d.unscaled.to_string())
}

/// This decimal as a Java `BigDecimal` value.
pub fn decimal_value(d: &Decimal) -> JValue {
    JValue::BigDecimal(java_string(d).into())
}

fn big_int_value(i: BigInt) -> JValue {
    JValue::BigInteger(i.to_string().into())
}

/// Whether `v` is a `java.lang.Number` the math nodes handle: the boxed primitives other than `Character`,
/// `BigInteger` and `BigDecimal`.
pub fn is_number(v: &JValue) -> bool {
    matches!(v, JValue::Byte(_) | JValue::Short(_) | JValue::Int(_) | JValue::Long(_) | JValue::Float(_) | JValue::Double(_) | JValue::BigInteger(_) | JValue::BigDecimal(_))
}

/// `isInteger`: `Byte`, `Short`, `Integer`, `Long`, `BigInteger`.
fn is_integer(v: &JValue) -> bool {
    matches!(v, JValue::Byte(_) | JValue::Short(_) | JValue::Int(_) | JValue::Long(_) | JValue::BigInteger(_))
}

/// `BigInteger.longValue()`: the low 64 bits, two's complement.
fn low_long(b: &BigInt) -> i64 {
    let m = b.mod_floor(&(BigInt::from(1) << 64));
    m.to_u64().unwrap_or(0) as i64
}

/// `Number.longValue()`.
pub fn long_value(v: &JValue) -> i64 {
    match v {
        JValue::Byte(b) => *b as i64,
        JValue::Short(v) => *v as i64,
        JValue::Int(i) => *i as i64,
        JValue::Long(l) => *l,
        JValue::Char(c) => *c as i64,
        JValue::Float(f) => *f as i64,
        JValue::Double(d) => *d as i64,
        JValue::BigInteger(s) => s.parse::<BigInt>().map(|b| low_long(&b)).unwrap_or(0),
        JValue::BigDecimal(s) => Decimal::parse(s).map(|d| low_long(&d.to_big_int())).unwrap_or(0),
        _ => 0,
    }
}

/// `BigInteger`/`BigDecimal.{byte,short,int,long}ValueExact()`: `None` when out of range or fractional.
pub fn exact_integral(v: &JValue, bits: u8) -> Option<i64> {
    let i = match v {
        JValue::BigInteger(s) => s.parse::<BigInt>().ok()?,
        JValue::BigDecimal(s) => {
            let d = Decimal::parse(s)?;
            if !d.is_integral() {
                return None;
            }
            d.to_big_int()
        }
        other => BigInt::from(long_value(other)),
    };
    let v = i.to_i64()?;
    let ok = match bits {
        8 => i8::try_from(v).is_ok(),
        16 => i16::try_from(v).is_ok(),
        32 => i32::try_from(v).is_ok(),
        _ => true,
    };
    ok.then_some(v)
}

/// `BigDecimal.toBigIntegerExact()`.
pub fn big_decimal_to_big_integer_exact(v: &JValue) -> Option<String> {
    let d = to_decimal(v);
    d.is_integral().then(|| d.to_big_int().to_string())
}

/// `new BigInteger(String)`.
pub fn parse_big_integer(s: &str) -> Result<String, java_compat::lang::JError> {
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        let message = if s.is_empty() { "Zero length BigInteger".to_string() } else { format!("For input string: \"{s}\"") };
        return Err(java_compat::lang::JError::new("java.lang.NumberFormatException", message));
    }
    Ok(s.parse::<BigInt>().map(|b| b.to_string()).unwrap_or_default())
}

/// `Number.doubleValue()`; `0.0` for a non-number.
pub fn double_value(v: &JValue) -> f64 {
    match v {
        JValue::Byte(b) => *b as f64,
        JValue::Short(v) => *v as f64,
        JValue::Int(i) => *i as f64,
        JValue::Char(c) => *c as u32 as f64,
        JValue::Long(l) => *l as f64,
        JValue::Float(f) => *f as f64,
        JValue::Double(d) => *d,
        JValue::BigInteger(s) | JValue::BigDecimal(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn float_value(v: &JValue) -> f32 {
    match v {
        JValue::Float(f) => *f,
        other => double_value(other) as f32,
    }
}

/// `intValue()`.
pub fn int_value(v: &JValue) -> i32 {
    long_value(v) as i32
}

/// `MathUtils.toBigDecimal`: `v` as a `BigDecimal`; `0` for an unparsable one.
pub fn to_decimal(v: &JValue) -> Decimal {
    match v {
        JValue::BigDecimal(s) => Decimal::parse(s).unwrap_or(Decimal { unscaled: BigInt::zero(), scale: 0 }),
        JValue::BigInteger(s) => Decimal::from_int(s.parse().unwrap_or_default()),
        other => Decimal::from_f64(double_value(other)),
    }
}

fn to_big_int(v: &JValue) -> BigInt {
    match v {
        JValue::BigInteger(s) => s.parse().unwrap_or_default(),
        other => BigInt::from(long_value(other)),
    }
}

#[derive(PartialEq)]
enum Base {
    Long,
    Float,
    Double,
    BigInteger,
    BigDecimal,
}

fn base(a: &JValue, b: &JValue) -> Base {
    let (ai, bi) = (is_integer(a), is_integer(b));
    let big_int = matches!(a, JValue::BigInteger(_)) || matches!(b, JValue::BigInteger(_));
    if matches!(a, JValue::BigDecimal(_)) || matches!(b, JValue::BigDecimal(_)) || (!ai || !bi) && big_int {
        return Base::BigDecimal;
    }
    if ai && bi {
        return if big_int { Base::BigInteger } else { Base::Long };
    }
    if matches!(a, JValue::Double(_)) || matches!(b, JValue::Double(_)) { Base::Double } else { Base::Float }
}

/// `typesBySize.indexOf(class)`: `-1` for `BigInteger` and `BigDecimal`.
fn size_index(v: &JValue) -> i32 {
    match v {
        JValue::Byte(_) => 0,
        JValue::Short(_) => 1,
        JValue::Int(_) => 2,
        JValue::Long(_) => 3,
        JValue::Float(_) => 4,
        JValue::Double(_) => 5,
        _ => -1,
    }
}

/// `wrapPrimitive(value, type)`: the value in `type`, or the next wider type it fits.
fn wrap_as(value: i64, index: i32) -> JValue {
    if index == 0 && value >= i8::MIN as i64 && value <= i8::MAX as i64 {
        return JValue::Byte(value as i8);
    }
    if (0..=1).contains(&index) && value >= i16::MIN as i64 && value <= i16::MAX as i64 {
        return JValue::Short(value as i16);
    }
    if (0..=2).contains(&index) && value >= i32::MIN as i64 && value <= i32::MAX as i64 {
        return JValue::Int(value as i32);
    }
    if (0..=3).contains(&index) {
        return JValue::Long(value);
    }
    JValue::BigInteger(value.to_string().into())
}

/// `wrapPrimitive(value, op1, op2)`: the result in the larger of the operands' types (the second on a tie).
fn wrap(value: i64, a: &JValue, b: &JValue) -> JValue {
    let index = if size_index(a) > size_index(b) { size_index(a) } else { size_index(b) };
    wrap_as(value, index)
}

/// `MathUtils.isZero`: whether a number equals zero.
pub fn is_zero(v: &JValue) -> bool {
    match v {
        JValue::Byte(b) => *b == 0,
        JValue::Short(s) => *s == 0,
        JValue::Int(i) => *i == 0,
        JValue::Long(l) => *l == 0,
        JValue::BigInteger(s) => to_big_int(&JValue::BigInteger(s.clone())).is_zero(),
        JValue::Float(f) => *f == 0.0,
        JValue::Double(d) => *d == 0.0,
        other => to_decimal(other).is_zero(),
    }
}

/// `MathUtils.add`: `a + b` in the wider of the two types; integral sums that overflow `long` become `BigInteger`.
pub fn add(a: &JValue, b: &JValue) -> JValue {
    match base(a, b) {
        Base::BigInteger => big_int_value(to_big_int(a) + to_big_int(b)),
        Base::Long => match long_value(a).checked_add(long_value(b)) {
            Some(r) => wrap(r, a, b),
            None => big_int_value(to_big_int(a) + to_big_int(b)),
        },
        Base::Float => JValue::Float(float_value(a) + float_value(b)),
        Base::Double => JValue::Double(double_value(a) + double_value(b)),
        Base::BigDecimal => decimal_value(&to_decimal(a).add(&to_decimal(b))),
    }
}

/// `MathUtils.subtract`: `a - b` in the wider of the two types; integral results that overflow `long` become
/// `BigInteger`.
pub fn subtract(a: &JValue, b: &JValue) -> JValue {
    match base(a, b) {
        Base::BigInteger => big_int_value(to_big_int(a) - to_big_int(b)),
        Base::Long => match long_value(a).checked_sub(long_value(b)) {
            Some(r) => wrap(r, a, b),
            None => big_int_value(to_big_int(a) - to_big_int(b)),
        },
        Base::Float => JValue::Float(float_value(a) - float_value(b)),
        Base::Double => JValue::Double(double_value(a) - double_value(b)),
        Base::BigDecimal => decimal_value(&to_decimal(a).sub(&to_decimal(b))),
    }
}

/// `MathUtils.multiply`: `a * b` in the wider of the two types; integral products that overflow `long` become
/// `BigInteger`.
pub fn multiply(a: &JValue, b: &JValue) -> JValue {
    match base(a, b) {
        Base::BigInteger => big_int_value(to_big_int(a) * to_big_int(b)),
        Base::Long => {
            // `(l2 != 0) && (result / l2 != l1)`, with Java's wrapping arithmetic.
            let (l1, l2) = (long_value(a), long_value(b));
            let r = l1.wrapping_mul(l2);
            if l2 != 0 && r.wrapping_div(l2) != l1 { big_int_value(to_big_int(a) * to_big_int(b)) } else { wrap(r, a, b) }
        }
        Base::Float => JValue::Float(float_value(a) * float_value(b)),
        Base::Double => JValue::Double(double_value(a) * double_value(b)),
        Base::BigDecimal => decimal_value(&to_decimal(a).mul(&to_decimal(b))),
    }
}

/// `divide`; the caller has checked the divisor is not zero.
pub fn divide(a: &JValue, b: &JValue) -> JValue {
    match base(a, b) {
        Base::BigInteger => big_int_value(to_big_int(a) / to_big_int(b)),
        Base::Long => {
            let (x, y) = (long_value(a), long_value(b));
            // `Long.MIN_VALUE / -1` overflows to `Long.MIN_VALUE` in Java.
            wrap(x.wrapping_div(y), a, b)
        }
        Base::Float => JValue::Float(float_value(a) / float_value(b)),
        Base::Double => JValue::Double(double_value(a) / double_value(b)),
        Base::BigDecimal => decimal_value(&to_decimal(a).divide_half_down(&to_decimal(b))),
    }
}

/// `modulo`: `Err` for `BigDecimal`s (an `ArithmeticException`).
pub fn modulo(a: &JValue, b: &JValue) -> Result<JValue, String> {
    Ok(match base(a, b) {
        Base::BigInteger => {
            let m = to_big_int(b);
            big_int_value(to_big_int(a).mod_floor(&m.abs()))
        }
        Base::Long => wrap(long_value(a).wrapping_rem(long_value(b)), a, b),
        Base::Float => JValue::Float(float_value(a) % float_value(b)),
        Base::Double => JValue::Double(double_value(a) % double_value(b)),
        Base::BigDecimal => return Err("Cannot calculate the modulo of BigDecimals.".into()),
    })
}

/// `MathUtils.compare`: `a` against `b` in the wider of the two types; `NaN` compares equal.
pub fn compare(a: &JValue, b: &JValue) -> Ordering {
    match base(a, b) {
        Base::BigInteger => to_big_int(a).cmp(&to_big_int(b)),
        Base::Long => long_value(a).cmp(&long_value(b)),
        Base::Float => float_value(a).partial_cmp(&float_value(b)).unwrap_or(Ordering::Equal),
        Base::Double => double_value(a).partial_cmp(&double_value(b)).unwrap_or(Ordering::Equal),
        Base::BigDecimal => to_decimal(a).compare(&to_decimal(b)),
    }
}

/// `MathUtils.negate`: `-v` in its own type; `-Long.MIN_VALUE` becomes `BigInteger`.
pub fn negate(v: &JValue) -> JValue {
    match v {
        JValue::BigInteger(_) => big_int_value(-to_big_int(v)),
        JValue::Byte(_) | JValue::Short(_) | JValue::Int(_) | JValue::Long(_) => {
            let l = long_value(v);
            if l == i64::MIN {
                return big_int_value(-BigInt::from(l));
            }
            wrap_as(-l, size_index(v))
        }
        JValue::Float(f) => JValue::Float(-f),
        JValue::Double(d) => JValue::Double(-d),
        other => decimal_value(&to_decimal(other).negate()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_arithmetic_widens_like_java() {
        assert_eq!(add(&JValue::Int(i32::MAX), &JValue::Int(1)).to_java_string(), "2147483648");
        assert!(matches!(add(&JValue::Int(1), &JValue::Int(2)), JValue::Int(3)));
        assert!(matches!(divide(&JValue::Int(7), &JValue::Int(2)), JValue::Int(3)));
        assert_eq!(add(&JValue::Long(i64::MAX), &JValue::Int(1)).to_java_string(), "9223372036854775808");
    }

    #[test]
    fn big_decimals() {
        let five = JValue::BigDecimal("5".into());
        assert_eq!(subtract(&five, &JValue::Int(1)).to_java_string(), "4.0");
        assert_eq!(divide(&JValue::BigDecimal("10".into()), &JValue::Int(3)).to_java_string(), "3");
        assert_eq!(divide(&JValue::BigDecimal("1.00".into()), &JValue::BigDecimal("8".into())).to_java_string(), "0.12");
        assert_eq!(multiply(&JValue::BigDecimal("1.5".into()), &JValue::Double(2.0)).to_java_string(), "3.00");
    }
}
