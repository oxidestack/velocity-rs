//! `org.apache.velocity.tools.generic.MathTool` (`$math`): arithmetic in `double`, results typed after the
//! arguments (`matchType`), numbers from strings through the tool's format and locale.

use std::sync::{Arc, Weak};

use java_compat::lang::{JArray, JError, JObject, JValue};
use java_compat::reflect::{ClassDef, JType};

use super::bean::get_property;
use super::classes::{Annotation, JavaClass};
use super::num;
use super::safe::{self, FormatConfig, HasFormat, HasLocale, LocaleConfig, SafeConfig, Tool};
use super::values::ValueParser;
use super::{c, identity, o, s, tool_object, val};

/// The class name of `org.apache.velocity.tools.generic.MathTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.MathTool";

/// Port of `org.apache.velocity.tools.generic.MathTool` (`$math`): arithmetic over numbers or strings parsed in the
/// tool's format and locale, `None` (Java `null`) for any argument that is not a number.
pub struct MathTool {
    config: FormatConfig,
    me: Weak<MathTool>,
}

/// `Math.max(double, double)`: `NaN` wins, `-0.0 < 0.0`.
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() && b.is_sign_negative() { -0.0 } else { 0.0 };
    }
    if a >= b { a } else { b }
}

fn java_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() || b.is_sign_negative() { -0.0 } else { 0.0 };
    }
    if a <= b { a } else { b }
}

/// `Math.pow`: where Java and C disagree (`NaN` exponents, `±1` to infinite powers), Java's answer.
pub fn java_pow(a: f64, b: f64) -> f64 {
    if b.is_nan() || (a.is_nan() && b != 0.0) {
        return f64::NAN;
    }
    if a.abs() == 1.0 && b.is_infinite() {
        return f64::NAN;
    }
    a.powf(b)
}

/// `hasFloatingPoint(value)`.
fn has_floating_point(v: &str) -> bool {
    v.contains('.')
}

/// `matchType(out, in...)`: an `Integer` (or `Long` beyond its range) when `out` is integral and no input printed
/// with a decimal point, else a `Double`.
pub fn match_type(out: f64, inputs: &[Option<&JValue>]) -> JValue {
    let mut integral = num::rint(out) == out;
    if integral {
        for n in inputs {
            match n {
                None => break,
                Some(n) if has_floating_point(&n.to_java_string()) => {
                    integral = false;
                    break;
                }
                _ => {}
            }
        }
    }
    if !integral {
        JValue::Double(out)
    } else if out > i32::MAX as f64 || out < i32::MIN as f64 {
        JValue::Long(out as i64)
    } else {
        JValue::Int(out as i32)
    }
}

fn all(values: &[JValue]) -> Vec<Option<&JValue>> {
    values.iter().map(Some).collect()
}

impl MathTool {
    /// `new MathTool()`: the default format and locale.
    pub fn new() -> Arc<MathTool> {
        Arc::new_cyclic(|me| MathTool { config: FormatConfig::default(), me: me.clone() })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: reads `locale` and `format`; nothing once the configuration is locked.
    pub fn configure(&self, params: &java_compat::lang::JMap) {
        safe::configure(self, params)
    }

    /// `getFormat()`: the number format arguments parse with, `default` unless configured.
    pub fn get_format(&self) -> String {
        self.config.format()
    }

    /// `getLocale()`: the locale arguments parse with.
    pub fn get_locale(&self) -> java_compat::text::Locale {
        self.config.locale()
    }

    /// `toNumber(num)` (deprecated but public): through the tool's format and locale.
    pub fn to_number(&self, num: &JValue) -> Option<JValue> {
        super::conversion::to_number_in(num, &self.get_format(), &self.get_locale()).filter(|_| !num.is_null())
    }

    fn numbers(&self, nums: &[JValue]) -> Option<Vec<JValue>> {
        nums.iter().map(|n| self.to_number(n)).collect()
    }

    /// `add(nums...)`: the sum, an `Integer`/`Long` when integral and no input had a decimal point, else a `Double`.
    pub fn add(&self, nums: &[JValue]) -> Option<JValue> {
        let ns = self.numbers(nums)?;
        let value = ns.iter().fold(0.0, |v, n| v + num::double_value(n));
        Some(match_type(value, &all(&ns)))
    }

    /// `sub(nums...)`: the first number minus the rest, typed as `add`'s result.
    pub fn sub(&self, nums: &[JValue]) -> Option<JValue> {
        let ns = self.numbers(nums)?;
        let mut value = 0.0;
        for (i, n) in ns.iter().enumerate() {
            if i == 0 {
                value = num::double_value(n);
            } else {
                value -= num::double_value(n);
            }
        }
        Some(match_type(value, &all(&ns)))
    }

    /// `mul(nums...)`: the product, typed as `add`'s result.
    pub fn mul(&self, nums: &[JValue]) -> Option<JValue> {
        let ns = self.numbers(nums)?;
        let value = ns.iter().fold(1.0, |v, n| v * num::double_value(n));
        Some(match_type(value, &all(&ns)))
    }

    /// `div(nums...)`: the first number divided by the rest; `None` on division by zero.
    pub fn div(&self, nums: &[JValue]) -> Option<JValue> {
        let mut ns = Vec::new();
        let mut value = 0.0;
        for (i, n) in nums.iter().enumerate() {
            let n = self.to_number(n)?;
            if i == 0 {
                value = num::double_value(&n);
            } else {
                let d = num::double_value(&n);
                if d == 0.0 {
                    return None;
                }
                value /= d;
            }
            ns.push(n);
        }
        Some(match_type(value, &all(&ns)))
    }

    /// `max(nums...)`: the largest, starting from `Double.MIN_VALUE` as upstream does (so never below it).
    pub fn max(&self, nums: &[JValue]) -> Option<JValue> {
        let ns = self.numbers(nums)?;
        let value = ns.iter().fold(f64::from_bits(1), |v, n| java_max(v, num::double_value(n)));
        Some(match_type(value, &all(&ns)))
    }

    /// `min(nums...)`: the smallest, starting from `Double.MAX_VALUE`.
    pub fn min(&self, nums: &[JValue]) -> Option<JValue> {
        let ns = self.numbers(nums)?;
        let value = ns.iter().fold(f64::MAX, |v, n| java_min(v, num::double_value(n)));
        Some(match_type(value, &all(&ns)))
    }

    fn longs(&self, nums: &[JValue]) -> Option<Vec<JValue>> {
        nums.iter().map(|n| self.to_long(n).map(JValue::Long)).collect()
    }

    /// `bitwiseOr(nums...)`: the arguments as `long`s, or-ed.
    pub fn bitwise_or(&self, nums: &[JValue]) -> Option<JValue> {
        let ns = self.longs(nums)?;
        let value = ns.iter().fold(0i64, |v, n| v | num::long_value(n));
        Some(match_type(value as f64, &all(&ns)))
    }

    /// `bitwiseAnd(nums...)`: the arguments as `long`s, and-ed.
    pub fn bitwise_and(&self, nums: &[JValue]) -> Option<JValue> {
        let ns = self.longs(nums)?;
        let value = ns.iter().fold(-1i64, |v, n| v & num::long_value(n));
        Some(match_type(value as f64, &all(&ns)))
    }

    /// `bitwiseXOr(nums...)`: the arguments as `long`s, xor-ed.
    pub fn bitwise_xor(&self, nums: &[JValue]) -> Option<JValue> {
        let ns = self.longs(nums)?;
        let value = ns.iter().fold(0i64, |v, n| v ^ num::long_value(n));
        Some(match_type(value as f64, &all(&ns)))
    }

    /// `pow(num1, num2)`: `Math.pow`, typed as `add`'s result.
    pub fn pow(&self, num1: &JValue, num2: &JValue) -> Option<JValue> {
        let (n1, n2) = (self.to_number(num1)?, self.to_number(num2)?);
        let value = java_pow(num::double_value(&n1), num::double_value(&n2));
        Some(match_type(value, &[Some(&n1), Some(&n2)]))
    }

    /// `idiv(num1, num2)`: integer division of the `int` values; `None` for a zero divisor.
    pub fn idiv(&self, num1: &JValue, num2: &JValue) -> Option<i32> {
        let (n1, n2) = (self.to_number(num1)?, self.to_number(num2)?);
        let d = num::int_value(&n2);
        if d == 0 {
            return None;
        }
        Some(num::int_value(&n1).wrapping_div(d))
    }

    /// `mod(num1, num2)`: the `int` remainder; `None` for a zero divisor.
    pub fn modulo(&self, num1: &JValue, num2: &JValue) -> Option<i32> {
        let (n1, n2) = (self.to_number(num1)?, self.to_number(num2)?);
        let d = num::int_value(&n2);
        if d == 0 {
            return None;
        }
        Some(num::int_value(&n1).wrapping_rem(d))
    }

    /// `abs(num)`: the absolute value, typed as `add`'s result.
    pub fn abs(&self, n: &JValue) -> Option<JValue> {
        let n = self.to_number(n)?;
        let value = num::double_value(&n).abs();
        Some(match_type(value, &[Some(&n)]))
    }

    /// `ceil(num)`: rounded up to an `int`.
    pub fn ceil(&self, n: &JValue) -> Option<i32> {
        self.to_number(n).map(|n| num::double_value(&n).ceil() as i32)
    }

    /// `floor(num)`: rounded down to an `int`.
    pub fn floor(&self, n: &JValue) -> Option<i32> {
        self.to_number(n).map(|n| num::double_value(&n).floor() as i32)
    }

    /// `round(num)`: `Math.rint` (half to even) to an `int`.
    pub fn round(&self, n: &JValue) -> Option<i32> {
        self.to_number(n).map(|n| num::rint(num::double_value(&n)) as i32)
    }

    /// `roundTo(decimals, num)`: `num` rounded half up to `decimals` places (at least one).
    pub fn round_to(&self, decimals: &JValue, n: &JValue) -> Option<f64> {
        let (i, d) = (self.to_number(decimals)?, self.to_number(n)?);
        let places = num::int_value(&i);
        let value = num::double_value(&d);
        let mut delta: i32 = 10;
        for _ in 1..places.max(1) {
            delta = delta.wrapping_mul(10);
        }
        Some(num::round(value * delta as f64) as f64 / delta as f64)
    }

    /// `getRandom()`: `Math.random()`, in `[0, 1)`.
    pub fn get_random(&self) -> f64 {
        super::random()
    }

    /// `random(num1, num2)`: a random number in `[num1, num2)`, an integer unless either bound has a decimal point.
    pub fn random(&self, num1: &JValue, num2: &JValue) -> Option<JValue> {
        let (n1, n2) = (self.to_number(num1)?, self.to_number(num2)?);
        let diff = num::double_value(&n2) - num::double_value(&n1);
        let random = diff * super::random() + num::double_value(&n1);
        let joined = n1.to_java_string() + &n2.to_java_string();
        if !joined.contains('.') {
            return Some(match_type(random.floor(), &[Some(&n1), Some(&n2)]));
        }
        Some(JValue::Double(random))
    }

    /// `toInteger(num)`: the parsed number narrowed to `int`.
    pub fn to_integer(&self, n: &JValue) -> Option<i32> {
        self.to_number(n).map(|n| num::int_value(&n))
    }

    /// `toLong(num)`: the parsed number narrowed to `long`.
    pub fn to_long(&self, n: &JValue) -> Option<i64> {
        self.to_number(n).map(|n| num::long_value(&n))
    }

    /// `toDouble(num)`: the parsed number as a `double`.
    pub fn to_double(&self, n: &JValue) -> Option<f64> {
        self.to_number(n).map(|n| num::double_value(&n))
    }

    /// `getTotal(collection, field)`: each element's property; `null` when anything throws.
    pub fn get_total_of(&self, items: &[JValue], field: Option<&str>) -> Option<JValue> {
        let field = field?;
        let mut result = 0.0;
        let mut first: Option<JValue> = None;
        for item in items {
            let property = get_property(item, field).ok()?;
            if let Some(v) = self.to_number(&property) {
                result += num::double_value(&v);
                first.get_or_insert(v);
            }
        }
        Some(match_type(result, &[first.as_ref()]))
    }

    /// `getAverage(collection, field)`: the elements' `field` property averaged over all elements; `None` when anything
    /// throws.
    pub fn get_average_of(&self, items: &[JValue], field: Option<&str>) -> Option<JValue> {
        let result = self.get_total_of(items, field)?;
        let avg = num::double_value(&result) / items.len() as f64;
        Some(match_type(avg, &[Some(&result)]))
    }

    /// `getTotal(collection)`: `null` as soon as an element is not a number.
    pub fn get_total(&self, items: &[JValue]) -> Option<JValue> {
        let mut result = 0.0;
        let mut first: Option<JValue> = None;
        for item in items {
            let v = self.to_number(item)?;
            result += num::double_value(&v);
            first.get_or_insert(v);
        }
        Some(match_type(result, &[first.as_ref()]))
    }

    /// `getAverage(collection)`: the total over the element count; `None` as soon as an element is not a number.
    pub fn get_average(&self, items: &[JValue]) -> Option<JValue> {
        let result = self.get_total(items)?;
        let avg = num::double_value(&result) / items.len() as f64;
        Some(match_type(avg, &[Some(&result)]))
    }

    /// `getTotal(double...)`.
    pub fn get_total_doubles(&self, values: &[f64]) -> f64 {
        values.iter().sum()
    }

    /// `getAverage(double...)`: `NaN` for no values.
    pub fn get_average_doubles(&self, values: &[f64]) -> f64 {
        self.get_total_doubles(values) / values.len() as f64
    }

    /// `getTotal(long...)`.
    pub fn get_total_longs(&self, values: &[i64]) -> i64 {
        values.iter().fold(0i64, |a, b| a.wrapping_add(*b))
    }

    /// `getAverage(long...)`: typed as `add`'s result.
    pub fn get_average_longs(&self, values: &[i64]) -> JValue {
        let total = JValue::Long(self.get_total_longs(values));
        let avg = num::double_value(&total) / values.len() as f64;
        match_type(avg, &[Some(&total)])
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for MathTool {
    fn safe(&self) -> &SafeConfig {
        &self.config.locale.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        self.config.configure(values);
    }
}

impl HasLocale for MathTool {
    fn locale_config(&self) -> &LocaleConfig {
        &self.config.locale
    }
}

impl HasFormat for MathTool {
    fn format_config(&self) -> &FormatConfig {
        &self.config
    }
}

fn opt(v: Option<JValue>) -> super::Ret {
    val(v.unwrap_or(JValue::Null))
}

fn int(v: Option<i32>) -> super::Ret {
    val(v.map(JValue::Int).unwrap_or(JValue::Null))
}

/// The elements of a varargs `Object[]` argument (`null` is a one-element array holding `null`… as Velocity
/// passes it: an explicit `null` array).
fn varargs(v: &JValue) -> Result<Vec<JValue>, JError> {
    match v {
        JValue::Array(a) => Ok(a.items()),
        JValue::Null => Err(super::npe()),
        other => Ok(vec![other.clone()]),
    }
}

fn collection_items(v: &JValue) -> Result<Option<Vec<JValue>>, JError> {
    match v {
        JValue::Null => Ok(None),
        other => Ok(Some(other.iterate().unwrap_or_default())),
    }
}

fn primitive_items<T>(v: &JValue, f: impl Fn(&JValue) -> T) -> Option<Vec<T>> {
    match v {
        JValue::Array(a) => Some(a.items().iter().map(f).collect()),
        _ => None,
    }
}

pub(crate) fn def() -> Arc<ClassDef> {
    type M = MathTool;
    let number = || c("java.lang.Number");
    let objects = || JType::array(o());
    let collection = || c("java.util.Collection");
    let integer = || c("java.lang.Integer");
    let double = || c("java.lang.Double");
    let mut def = ClassDef::new(CLASS).extends(&[safe::FORMAT_CONFIG, safe::LOCALE_CONFIG, safe::SAFE_CONFIG]).implements(&["java.io.Serializable"]);
    type Op = fn(&MathTool, &[JValue]) -> Option<JValue>;
    let ops: [(&str, Op); 9] = [
        ("add", MathTool::add),
        ("sub", MathTool::sub),
        ("mul", MathTool::mul),
        ("div", MathTool::div),
        ("max", MathTool::max),
        ("min", MathTool::min),
        ("bitwiseOr", MathTool::bitwise_or),
        ("bitwiseAnd", MathTool::bitwise_and),
        ("bitwiseXOr", MathTool::bitwise_xor),
    ];
    for (name, f) in ops {
        def = def.method::<M>(name, &[o(), o()], Some(number()), move |t, _, a| opt(f(t, a)));
    }
    for (name, f) in ops {
        def = def.method::<M>(name, &[objects()], Some(number()), move |t, _, a| opt(f(t, &varargs(&a[0])?)));
    }
    let def = def
        .method::<M>("pow", &[o(), o()], Some(number()), |t, _, a| opt(t.pow(&a[0], &a[1])))
        .method::<M>("idiv", &[o(), o()], Some(integer()), |t, _, a| int(t.idiv(&a[0], &a[1])))
        .method::<M>("mod", &[o(), o()], Some(integer()), |t, _, a| int(t.modulo(&a[0], &a[1])))
        .method::<M>("abs", &[o()], Some(number()), |t, _, a| opt(t.abs(&a[0])))
        .method::<M>("ceil", &[o()], Some(integer()), |t, _, a| int(t.ceil(&a[0])))
        .method::<M>("floor", &[o()], Some(integer()), |t, _, a| int(t.floor(&a[0])))
        .method::<M>("round", &[o()], Some(integer()), |t, _, a| int(t.round(&a[0])))
        .method::<M>("roundTo", &[o(), o()], Some(double()), |t, _, a| opt(t.round_to(&a[0], &a[1]).map(JValue::Double)))
        .method::<M>("getRandom", &[], Some(double()), |t, _, _| val(JValue::Double(t.get_random())))
        .method::<M>("random", &[o(), o()], Some(number()), |t, _, a| opt(t.random(&a[0], &a[1])))
        .method::<M>("toInteger", &[o()], Some(integer()), |t, _, a| int(t.to_integer(&a[0])))
        .method::<M>("toLong", &[o()], Some(c("java.lang.Long")), |t, _, a| opt(t.to_long(&a[0]).map(JValue::Long)))
        .method::<M>("toDouble", &[o()], Some(double()), |t, _, a| opt(t.to_double(&a[0]).map(JValue::Double)))
        .method::<M>("toNumber", &[o()], Some(number()), |t, _, a| opt(t.to_number(&a[0])))
        .method::<M>("getTotal", &[collection(), s()], Some(number()), |t, _, a| match collection_items(&a[0])? {
            Some(items) => opt(t.get_total_of(&items, a[1].as_str())),
            None => val(JValue::Null),
        })
        .method::<M>("getAverage", &[collection(), s()], Some(number()), |t, _, a| match collection_items(&a[0])? {
            Some(items) => opt(t.get_average_of(&items, a[1].as_str())),
            None => val(JValue::Null),
        })
        .method::<M>("getTotal", &[objects(), s()], Some(number()), |t, _, a| opt(t.get_total_of(&varargs(&a[0])?, a[1].as_str())))
        .method::<M>("getAverage", &[objects(), s()], Some(number()), |t, _, a| opt(t.get_average_of(&varargs(&a[0])?, a[1].as_str())))
        .method::<M>("getTotal", &[collection()], Some(number()), |t, _, a| match collection_items(&a[0])? {
            Some(items) => opt(t.get_total(&items)),
            None => val(JValue::Null),
        })
        .method::<M>("getAverage", &[collection()], Some(number()), |t, _, a| match collection_items(&a[0])? {
            Some(items) => opt(t.get_average(&items)),
            None => val(JValue::Null),
        })
        .method::<M>("getTotal", &[objects()], Some(number()), |t, _, a| opt(t.get_total(&varargs(&a[0])?)))
        .method::<M>("getAverage", &[objects()], Some(number()), |t, _, a| opt(t.get_average(&varargs(&a[0])?)))
        .method::<M>("getTotal", &[JType::array(JType::Double)], Some(number()), |t, _, a| match primitive_items(&a[0], num::double_value) {
            Some(v) => val(JValue::Double(t.get_total_doubles(&v))),
            None => val(JValue::Null),
        })
        .method::<M>("getAverage", &[JType::array(JType::Double)], Some(number()), |t, _, a| match primitive_items(&a[0], num::double_value) {
            Some(v) => val(JValue::Double(t.get_average_doubles(&v))),
            None => val(JValue::Null),
        })
        .method::<M>("getTotal", &[JType::array(JType::Long)], Some(number()), |t, _, a| match primitive_items(&a[0], num::long_value) {
            Some(v) => val(JValue::Long(t.get_total_longs(&v))),
            None => val(JValue::Null),
        })
        .method::<M>("getAverage", &[JType::array(JType::Long)], Some(number()), |t, _, a| match primitive_items(&a[0], num::long_value) {
            Some(v) => val(t.get_average_longs(&v)),
            None => val(JValue::Null),
        });
    super::object_methods::<M>(safe::format_config_methods::<M>(def)).register()
}

tool_object!(MathTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || MathTool::new().value()).annotated(Annotation::default_key("math")).annotated(Annotation::valid_scope(&[super::scope::APPLICATION]))
}

/// A `double[]` / `long[]` argument for the Rust API.
pub fn doubles(values: &[f64]) -> JValue {
    JValue::Array(Arc::new(JArray::new(JType::Double, values.iter().map(|d| JValue::Double(*d)).collect())))
}
