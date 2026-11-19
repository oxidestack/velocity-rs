//! How `UberspectImpl` finds and calls a method on a class with a typed definition ([`ClassDef`]): `ClassMap`'s
//! method cache, `MethodMap.getBestMatch` with its specificity and applicability rules,
//! `IntrospectionUtils`' invocation conversions, `TypeConversionHandlerImpl`'s explicit conversions, and
//! `VelMethodImpl.invoke`'s varargs packing, then the unboxing and widening `Method.invoke` does itself.

use std::collections::HashMap;
use std::sync::Arc;

use java_compat::lang::{JArray, JError, JValue};
use java_compat::reflect::{ClassDef, JMethod, JType, class_of};

use crate::math;

const NOT_CONVERTIBLE: u8 = 0;
const EXPLICITLY_CONVERTIBLE: u8 = 1;
const IMPLICITLY_CONVERTIBLE: u8 = 2;
const STRICTLY_CONVERTIBLE: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Specificity {
    Incomparable,
    MoreSpecific,
    Equivalent,
    LessSpecific,
}

thread_local! {
    static EXPLICIT_CONVERSIONS: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// While alive, method lookups on this thread consider (or not) `TypeConversionHandlerImpl`'s explicit
/// conversions: `SecureUberspector` builds its introspector without a conversion handler, so `MethodMap` then
/// only matches strictly or implicitly convertible arguments.
pub struct ConversionsGuard(bool);

impl ConversionsGuard {
    /// Turns explicit conversions on or off on this thread; the previous setting comes back when the guard drops.
    pub fn new(enabled: bool) -> ConversionsGuard {
        ConversionsGuard(EXPLICIT_CONVERSIONS.with(|c| c.replace(enabled)))
    }
}

impl Drop for ConversionsGuard {
    fn drop(&mut self) {
        EXPLICIT_CONVERSIONS.with(|c| c.set(self.0));
    }
}

/// `MethodMap.isExplicitlyConvertible`: `false` without a conversion handler.
fn method_map_explicit(formal: &JType, actual: Option<&JType>, possible_vararg: bool) -> bool {
    EXPLICIT_CONVERSIONS.with(|c| c.get()) && is_explicitly_convertible(formal, actual, possible_vararg)
}

/// `MethodMap.AmbiguousException`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ambiguous;

/// A converter an application adds (`org.apache.velocity.util.introspection.Converter`).
pub type ConverterFn = Arc<dyn Fn(&JValue) -> Result<JValue, JError> + Send + Sync>;

/// The conversion `getNeededConverter` found.
#[derive(Clone)]
pub enum Conv {
    /// One of `TypeConversionHandlerImpl`'s standard converters.
    Standard(Converter),
    /// A converter the application added with `addConverter`.
    Custom(ConverterFn),
}

impl Conv {
    /// `Converter.convert(o)`: `v` converted to the parameter type; `Err` when the conversion throws.
    pub fn convert(&self, v: &JValue) -> Result<JValue, JError> {
        match self {
            Conv::Standard(c) => c.convert(v),
            Conv::Custom(f) => f(v),
        }
    }
}

/// `TypeConversionHandler`.
pub trait TypeConversionHandler: Send + Sync {
    /// `isExplicitlyConvertible(formal, actual, possibleVarArg)`: whether a value of class `actual` (`None` for
    /// null) can be converted to `formal`.
    fn is_explicitly_convertible(&self, formal: &JType, actual: Option<&JType>, possible_vararg: bool) -> bool;
    /// `getNeededConverter(formal, actual)`: the converter from `actual` (`None` for null) to `formal`, if any.
    fn needed_converter(&self, formal: &JType, actual: Option<&JType>) -> Option<Conv>;
    /// `addConverter(formal, actual, converter)`: makes `converter` the conversion from `actual` to `formal`.
    fn add_converter(&self, formal: &JType, actual: &JType, converter: ConverterFn) -> Result<(), JError>;
    /// The handler as `Any`, so callers can downcast to its concrete type.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// `TypeConversionHandlerImpl`: the standard conversions, then the ones added or found before (`converterCacheMap`),
/// then conversion to `String`.
#[derive(Default)]
pub struct TypeConversionHandlerImpl {
    cache: parking_lot::RwLock<HashMap<(String, String), Option<Conv>>>,
}

impl TypeConversionHandler for TypeConversionHandlerImpl {
    fn is_explicitly_convertible(&self, formal: &JType, actual: Option<&JType>, possible_vararg: bool) -> bool {
        if actual.is_some_and(|a| a == formal) || is_convertible(formal, actual, possible_vararg) || self.needed_converter(formal, actual).is_some() {
            return true;
        }
        if possible_vararg {
            if let (Some(component), Some(actual)) = (formal.component(), actual) {
                let actual = actual.component().unwrap_or(actual);
                return self.is_explicitly_convertible(component, Some(actual), false);
            }
        }
        false
    }

    fn needed_converter(&self, formal: &JType, actual: Option<&JType>) -> Option<Conv> {
        let actual = actual?;
        let key = (formal.type_name(), actual.type_name());
        if let Some(c) = standard_converter(&key.0, &key.1) {
            return Some(Conv::Standard(c));
        }
        if let Some(c) = self.cache.read().get(&key) {
            return c.clone();
        }
        let c = if *formal == JType::string() {
            Some(Conv::Standard(Converter::ToString))
        } else {
            // `Enum.valueOf(enumClass, string)`.
            match java_compat::reflect::class_named(&formal.name()) {
                Some(def) if def.is_enum() && *actual == JType::string() => {
                    Some(Conv::Custom(Arc::new(move |v: &JValue| if v.is_null() { Ok(JValue::Null) } else { java_compat::reflect::EnumConstant::value_of(&def, &v.to_java_string()) })))
                }
                _ => None,
            }
        };
        self.cache.write().insert(key, c.clone());
        c
    }

    fn add_converter(&self, formal: &JType, actual: &JType, converter: ConverterFn) -> Result<(), JError> {
        let mut cache = self.cache.write();
        let actual = actual.type_name();
        cache.insert((formal.type_name(), actual.clone()), Some(Conv::Custom(converter.clone())));
        // The primitive and its box share converters.
        let other = if formal.is_primitive() { formal.boxed() } else { formal.unboxed() };
        if other != *formal {
            cache.insert((other.type_name(), actual), Some(Conv::Custom(converter)));
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

thread_local! {
    /// The conversion handler of the engine rendering on this thread (`UberspectImpl.conversionHandler`).
    static HANDLER: std::cell::RefCell<Option<Arc<dyn TypeConversionHandler>>> = std::cell::RefCell::new(Some(default_handler()));
}

/// The handler engines get by default (`introspector.conversion_handler.class`), shared.
pub fn default_handler() -> Arc<dyn TypeConversionHandler> {
    static DEFAULT: std::sync::LazyLock<Arc<dyn TypeConversionHandler>> = std::sync::LazyLock::new(|| Arc::new(TypeConversionHandlerImpl::default()));
    DEFAULT.clone()
}

/// Makes `handler` the current one until the guard drops.
pub fn with_handler(handler: Option<Arc<dyn TypeConversionHandler>>) -> HandlerGuard {
    HandlerGuard(HANDLER.with(|h| h.replace(handler)))
}

/// Restores the previous conversion handler on this thread when dropped; see [`with_handler`].
pub struct HandlerGuard(Option<Arc<dyn TypeConversionHandler>>);

impl Drop for HandlerGuard {
    fn drop(&mut self) {
        let previous = self.0.take();
        HANDLER.with(|h| *h.borrow_mut() = previous);
    }
}

/// `TypeConversionHandlerImpl`'s class name, the default `introspector.conversion_handler.class`.
pub const TYPE_CONVERSION_HANDLER_IMPL: &str = "org.apache.velocity.util.introspection.TypeConversionHandlerImpl";

type HandlerFactory = Arc<dyn Fn() -> Arc<dyn TypeConversionHandler> + Send + Sync>;

static HANDLER_CLASSES: std::sync::LazyLock<parking_lot::RwLock<HashMap<String, HandlerFactory>>> = std::sync::LazyLock::new(|| {
    let mut m: HashMap<String, HandlerFactory> = HashMap::new();
    m.insert(TYPE_CONVERSION_HANDLER_IMPL.into(), Arc::new(|| Arc::new(TypeConversionHandlerImpl::default())));
    parking_lot::RwLock::new(m)
});

/// Makes a conversion handler class known to `introspector.conversion_handler.class`.
pub fn register_conversion_handler_class(class: &str, factory: impl Fn() -> Arc<dyn TypeConversionHandler> + Send + Sync + 'static) {
    HANDLER_CLASSES.write().insert(class.to_string(), Arc::new(factory));
}

/// `UberspectImpl.init`'s handler: `introspector.conversion_handler.instance` (an `Arc<dyn TypeConversionHandler>`),
/// else a new instance of `introspector.conversion_handler.class`; none for `none`.
pub(crate) fn configured_handler(props: &crate::props::ExtProperties) -> Result<Option<Arc<dyn TypeConversionHandler>>, crate::VelocityError> {
    if let Some(crate::props::PropValue::Object(o)) = props.get(crate::config::CONVERSION_HANDLER_INSTANCE) {
        return match o.downcast_ref::<Arc<dyn TypeConversionHandler>>() {
            Some(h) => Ok(Some(h.clone())),
            None => Err(crate::VelocityError::velocity(format!(
                "The specified class or provided instance for the conversion handler ({}) does not implement org.apache.velocity.util.introspection.TypeConversionHandler; Velocity is not initialized correctly.",
                "java.lang.Object"
            ))),
        };
    }
    match props.string(crate::config::CONVERSION_HANDLER_CLASS) {
        None => Ok(None),
        Some(c) if c == "none" => Ok(None),
        Some(c) => match HANDLER_CLASSES.read().get(c.as_str()) {
            Some(f) => Ok(Some(f())),
            None => Err(crate::VelocityError::velocity(format!("The specified class for ConversionHandler ({c}) does not exist or is not accessible to the current classloader."))),
        },
    }
}

fn handler() -> Option<Arc<dyn TypeConversionHandler>> {
    HANDLER.with(|h| h.borrow().clone())
}

/// Primitive widening that both strict and implicit conversion accept.
fn primitive_widening(formal: &JType, actual: &JType) -> bool {
    use JType::*;
    match formal {
        Short => matches!(actual, Byte),
        Int => matches!(actual, Byte | Short),
        Long => matches!(actual, Byte | Short | Int),
        Float => matches!(actual, Byte | Short | Int | Long),
        Double => matches!(actual, Byte | Short | Int | Long | Float),
        _ => false,
    }
}

/// `IntrospectionUtils.isStrictMethodInvocationConvertible`.
pub fn is_strict_convertible(formal: &JType, actual: Option<&JType>, possible_vararg: bool) -> bool {
    let Some(actual) = actual else { return !formal.is_primitive() };
    if formal.is_assignable_from(actual) {
        return true;
    }
    if formal.is_primitive() && primitive_widening(formal, actual) {
        return true;
    }
    if possible_vararg {
        if let Some(component) = formal.component() {
            let actual = actual.component().unwrap_or(actual);
            return is_strict_convertible(component, Some(actual), false);
        }
    }
    false
}

/// `IntrospectionUtils.isMethodInvocationConvertible`.
pub fn is_convertible(formal: &JType, actual: Option<&JType>, possible_vararg: bool) -> bool {
    use JType::*;
    let Some(actual) = actual else { return !formal.is_primitive() };
    if formal.is_assignable_from(actual) {
        return true;
    }
    if !formal.is_primitive() && actual.is_primitive() {
        let boxed = actual.boxed();
        if boxed == *formal || formal.is_assignable_from(&boxed) {
            return true;
        }
    }
    if formal.is_primitive() {
        if actual.is_primitive() {
            if primitive_widening(formal, actual) {
                return true;
            }
        } else {
            let a = actual.type_name();
            let boxed = |names: &[&str]| names.iter().any(|n| a == format!("java.lang.{n}"));
            let ok = match formal {
                Boolean => boxed(&["Boolean"]),
                Char => boxed(&["Character"]),
                Byte => boxed(&["Byte"]),
                Short => boxed(&["Short", "Byte"]),
                Int => boxed(&["Integer", "Short", "Byte"]),
                Long => boxed(&["Long", "Integer", "Short", "Byte"]),
                Float => boxed(&["Float", "Long", "Integer", "Short", "Byte"]),
                Double => boxed(&["Double", "Float", "Long", "Integer", "Short", "Byte"]),
                _ => false,
            };
            if ok {
                return true;
            }
        }
    }
    if possible_vararg {
        if let Some(component) = formal.component() {
            let actual = actual.component().unwrap_or(actual);
            return is_convertible(component, Some(actual), false);
        }
    }
    false
}

/// A `TypeConversionHandlerImpl` converter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Converter {
    /// A number to `boolean`: `false` only for zero (`-0.0` and `NaN` are `true`).
    NumberToBoolean,
    /// A `char` to `boolean`: `false` only for `'\0'`.
    CharToBoolean,
    /// A string to `boolean`: `true` for `true` in any case, else `false`.
    StringToBoolean,
    /// Narrowing (bounds-checked) or widening to an integral type of this many bits.
    ToIntegral(u8, bool),
    /// A string to an integral type of this many bits; `Err` (`NumberFormatException`) when it does not parse.
    StringToIntegral(u8),
    /// A `boolean` to an integral type of this many bits: `1` or `0`.
    BooleanToIntegral(u8),
    /// A number to `BigInteger`; a `BigDecimal` with a fraction is an `ArithmeticException`.
    ToBigInteger,
    /// A string to `BigInteger`; `Err` when it does not parse.
    StringToBigInteger,
    /// A number to `float`.
    ToFloat,
    /// A string to `float`; `Err` when it does not parse.
    StringToFloat,
    /// A number to `double`.
    ToDouble,
    /// A string to `double`; `Err` when it does not parse.
    StringToDouble,
    /// An integral number to `BigDecimal`, exactly.
    ExactToBigDecimal,
    /// A `float` or `double` to `BigDecimal`, through `Double.toString`.
    ApproxToBigDecimal,
    /// A `BigInteger` to `BigDecimal`.
    BigIntegerToBigDecimal,
    /// A string to `BigDecimal`; `Err` (`NumberFormatException`) when it does not parse.
    StringToBigDecimal,
    /// A string to `Locale` (`LocaleUtils.toLocale`).
    StringToLocale,
    /// Anything to `String`, through `toString()`.
    ToString,
}

fn numeric_class(n: &str) -> bool {
    matches!(
        n,
        "java.lang.Byte" | "java.lang.Short" | "java.lang.Integer" | "java.lang.Long" | "java.math.BigInteger" | "java.lang.Float" | "java.lang.Double" | "java.math.BigDecimal" | "java.lang.Number"
    )
}

fn numeric_prim(n: &str) -> bool {
    matches!(n, "byte" | "short" | "int" | "long" | "float" | "double")
}

/// `standardConverterMap`, keyed like it by (formal, actual) type names.
fn standard_converter(formal: &str, actual: &str) -> Option<Converter> {
    use Converter::*;
    let num = numeric_class(actual) || numeric_prim(actual);
    let (bits, boxed) = match formal {
        "java.lang.Byte" => (8, true),
        "byte" => (8, false),
        "java.lang.Short" => (16, true),
        "short" => (16, false),
        "java.lang.Integer" => (32, true),
        "int" => (32, false),
        "java.lang.Long" => (64, true),
        "long" => (64, false),
        _ => (0, false),
    };
    if bits > 0 {
        if actual == "java.lang.String" {
            return Some(StringToIntegral(bits));
        }
        if actual == "java.lang.Boolean" || actual == "boolean" {
            return Some(BooleanToIntegral(bits));
        }
        // Narrowing from wider types (and `Number`), widening from narrower ones only towards the wrapper class.
        let width = |n: &str| match n {
            "java.lang.Byte" | "byte" => 8,
            "java.lang.Short" | "short" => 16,
            "java.lang.Integer" | "int" => 32,
            "java.lang.Long" | "long" => 64,
            _ => 128,
        };
        if !num {
            return None;
        }
        let w = width(actual);
        if w > bits {
            return Some(ToIntegral(bits, true));
        }
        if w < bits && boxed {
            return Some(ToIntegral(bits, false));
        }
        return None;
    }
    Some(match formal {
        "java.lang.Boolean" | "boolean" => match actual {
            "java.lang.Character" | "char" => CharToBoolean,
            "java.lang.String" => StringToBoolean,
            _ if num => NumberToBoolean,
            _ => return None,
        },
        "java.math.BigInteger" => match actual {
            "java.lang.Byte" | "java.lang.Short" | "java.lang.Integer" | "java.lang.Long" | "byte" | "short" | "int" | "long" => ToBigInteger,
            "java.math.BigDecimal" => ToBigInteger,
            "java.lang.String" => StringToBigInteger,
            _ => return None,
        },
        "java.lang.Float" | "float" => match actual {
            "java.math.BigInteger" | "java.lang.Double" | "java.math.BigDecimal" | "java.lang.Number" | "double" => ToFloat,
            "java.lang.Byte" | "java.lang.Short" | "java.lang.Integer" | "java.lang.Long" | "byte" | "short" | "int" | "long" if formal == "java.lang.Float" => ToFloat,
            "java.lang.String" => StringToFloat,
            _ => return None,
        },
        "java.lang.Double" => match actual {
            "java.lang.String" => StringToDouble,
            "java.lang.Double" | "double" => return None,
            _ if num => ToDouble,
            _ => return None,
        },
        "double" => match actual {
            "java.math.BigInteger" | "java.math.BigDecimal" | "java.lang.Number" => ToDouble,
            "java.lang.String" => StringToDouble,
            _ => return None,
        },
        "java.math.BigDecimal" => match actual {
            "java.lang.Byte" | "java.lang.Short" | "java.lang.Integer" | "java.lang.Long" | "byte" | "short" | "int" | "long" => ExactToBigDecimal,
            "java.math.BigInteger" => BigIntegerToBigDecimal,
            "java.lang.Float" | "java.lang.Double" | "float" | "double" => ApproxToBigDecimal,
            "java.lang.String" => StringToBigDecimal,
            _ => return None,
        },
        "java.util.Locale" if actual == "java.lang.String" => StringToLocale,
        _ => return None,
    })
}

/// `getNeededConverter` of the current handler (none without one).
pub fn needed_converter(formal: &JType, actual: Option<&JType>) -> Option<Conv> {
    handler().and_then(|h| h.needed_converter(formal, actual))
}

/// `isExplicitlyConvertible` of the current handler (`false` without one).
pub fn is_explicitly_convertible(formal: &JType, actual: Option<&JType>, possible_vararg: bool) -> bool {
    handler().is_some_and(|h| h.is_explicitly_convertible(formal, actual, possible_vararg))
}

fn convert_level(formal: &JType, actual: Option<&JType>, vararg: bool) -> u8 {
    if is_strict_convertible(formal, actual, vararg) {
        STRICTLY_CONVERTIBLE
    } else if is_convertible(formal, actual, vararg) {
        IMPLICITLY_CONVERTIBLE
    } else if method_map_explicit(formal, actual, vararg) {
        EXPLICITLY_CONVERTIBLE
    } else {
        NOT_CONVERTIBLE
    }
}

/// `MethodMap.getApplicability`.
fn applicability(params: &[JType], classes: &[Option<JType>]) -> u8 {
    let mut ret = STRICTLY_CONVERTIBLE;
    let fold = |ret: &mut u8, formal: &JType, actual: Option<&JType>, vararg: bool| -> bool {
        let level = convert_level(formal, actual, vararg);
        if level == NOT_CONVERTIBLE {
            return false;
        }
        *ret = (*ret).min(level);
        true
    };
    if params.len() > classes.len() {
        if params.len() == classes.len() + 1 && params.last().is_some_and(JType::is_array) {
            for (i, c) in classes.iter().enumerate() {
                if !fold(&mut ret, &params[i], c.as_ref(), false) {
                    return NOT_CONVERTIBLE;
                }
            }
            return ret;
        }
        return NOT_CONVERTIBLE;
    }
    if params.len() == classes.len() {
        for (i, c) in classes.iter().enumerate() {
            let possible_vararg = i == classes.len() - 1 && params[i].is_array();
            if !fold(&mut ret, &params[i], c.as_ref(), possible_vararg) {
                return NOT_CONVERTIBLE;
            }
        }
        return ret;
    }
    if let Some(last) = params.last() {
        let Some(vararg) = last.component() else { return NOT_CONVERTIBLE };
        for i in 0..params.len() - 1 {
            if !fold(&mut ret, &params[i], classes[i].as_ref(), false) {
                return NOT_CONVERTIBLE;
            }
        }
        for c in &classes[params.len() - 1..] {
            if !fold(&mut ret, vararg, c.as_ref(), false) {
                return NOT_CONVERTIBLE;
            }
        }
        return ret;
    }
    NOT_CONVERTIBLE
}

fn step_down(level: &mut u8, formal: &JType, actual: &JType, last: bool) {
    if *level == STRICTLY_CONVERTIBLE {
        if is_strict_convertible(formal, Some(actual), last) {
            return;
        }
        *level = IMPLICITLY_CONVERTIBLE;
    }
    if *level == IMPLICITLY_CONVERTIBLE {
        if is_convertible(formal, Some(actual), last) {
            return;
        }
        *level = EXPLICITLY_CONVERTIBLE;
    }
    if *level == EXPLICITLY_CONVERTIBLE {
        if method_map_explicit(formal, Some(actual), last) {
            return;
        }
        *level = NOT_CONVERTIBLE;
    }
}

/// `MethodMap.compare`: how the parameter list `t1` relates to `t2`.
fn compare(t1: &[Option<JType>], t2: &[Option<JType>]) -> Specificity {
    let (mut t1, mut t2) = (t1.to_vec(), t2.to_vec());
    let (mut t1_vararg, mut t2_vararg, mut fixed_lengths) = (false, false, false);
    if t1.len() > t2.len() {
        let l2 = t2.len();
        if l2 == 0 {
            return Specificity::MoreSpecific;
        }
        let item = t2[l2 - 1].as_ref().and_then(|t| t.component().cloned());
        t2.resize(t1.len(), None);
        match item {
            None => {
                t1_vararg = true;
                let n = t1.len();
                t2[n - 1] = None;
            }
            Some(item) => {
                t2_vararg = true;
                for slot in &mut t2[l2 - 1..] {
                    *slot = Some(item.clone());
                }
            }
        }
        fixed_lengths = true;
    } else if t2.len() > t1.len() {
        let l1 = t1.len();
        if l1 == 0 {
            return Specificity::LessSpecific;
        }
        let item = t1[l1 - 1].as_ref().and_then(|t| t.component().cloned());
        t1.resize(t2.len(), None);
        match item {
            None => {
                t2_vararg = true;
                let n = t2.len();
                t1[n - 1] = None;
            }
            Some(item) => {
                t1_vararg = true;
                for slot in &mut t1[l1 - 1..] {
                    *slot = Some(item.clone());
                }
            }
        }
        fixed_lengths = true;
    }
    let mut c1_to_c2 = STRICTLY_CONVERTIBLE;
    let mut c2_to_c1 = STRICTLY_CONVERTIBLE;
    for i in 0..t1.len() {
        let last = !fixed_lengths && i == t1.len() - 1;
        match (&t1[i], &t2[i]) {
            (a, b) if a == b => {}
            (None, Some(b)) => {
                c2_to_c1 = NOT_CONVERTIBLE;
                if b.is_primitive() {
                    c1_to_c2 = NOT_CONVERTIBLE;
                }
            }
            (Some(a), None) => {
                c1_to_c2 = NOT_CONVERTIBLE;
                if a.is_primitive() {
                    c2_to_c1 = NOT_CONVERTIBLE;
                }
            }
            (Some(a), Some(b)) => {
                step_down(&mut c1_to_c2, b, a, last);
                step_down(&mut c2_to_c1, a, b, last);
            }
            (None, None) => {}
        }
    }
    if c1_to_c2 == NOT_CONVERTIBLE && c2_to_c1 == NOT_CONVERTIBLE {
        return Specificity::Incomparable;
    }
    if c1_to_c2 > c2_to_c1 {
        return Specificity::MoreSpecific;
    }
    if c2_to_c1 > c1_to_c2 {
        return Specificity::LessSpecific;
    }
    let last_array = |t: &[Option<JType>]| t.last().and_then(|x| x.as_ref()).is_some_and(JType::is_array);
    let last1 = t1_vararg || !fixed_lengths && last_array(&t1);
    let last2 = t2_vararg || !fixed_lengths && last_array(&t2);
    if last1 && !last2 {
        return Specificity::LessSpecific;
    }
    if !last1 && last2 {
        return Specificity::MoreSpecific;
    }
    Specificity::Equivalent
}

struct Match<'a> {
    method: &'a JMethod,
    types: Vec<Option<JType>>,
    specificity: Specificity,
    applicability: u8,
    varargs: bool,
}

/// `MethodMap.getBestMatch`.
fn best_match<'a>(methods: &[&'a JMethod], args: &[Option<JType>]) -> Result<Option<&'a JMethod>, Ambiguous> {
    let unboxed: Vec<Option<JType>> = args.iter().map(|a| a.as_ref().map(JType::unboxed)).collect();
    let only_null_or_objects = !args.is_empty() && args.iter().all(|a| a.as_ref().is_none_or(|t| *t == JType::object()));
    let mut best: Vec<Match<'a>> = Vec::new();
    for m in methods {
        let applicability = applicability(&m.params, &unboxed);
        if applicability == NOT_CONVERTIBLE {
            continue;
        }
        let types: Vec<Option<JType>> = m.params.iter().cloned().map(Some).collect();
        let specificity = compare(&types, &unboxed);
        let varargs = m.params.last().is_some_and(JType::is_array);
        let candidate = Match { method: m, types, specificity, applicability, varargs };
        if best.is_empty() {
            best.push(candidate);
            continue;
        }
        let mut keep = true;
        let mut i = 0;
        while keep && i < best.len() {
            let b = &best[i];
            let mut remove = false;
            if b.specificity == Specificity::LessSpecific && (candidate.specificity as u8) < (Specificity::Equivalent as u8) {
                keep = false;
            } else if candidate.specificity == Specificity::LessSpecific && (b.specificity as u8) < (Specificity::Equivalent as u8) {
                remove = true;
            } else if b.applicability > candidate.applicability {
                keep = false;
            } else if b.applicability < candidate.applicability {
                remove = true;
            } else if only_null_or_objects {
                if candidate.varargs != b.varargs {
                    if candidate.varargs {
                        keep = false;
                    } else if b.varargs {
                        remove = true;
                    }
                }
            } else {
                match compare(&candidate.types, &b.types) {
                    Specificity::LessSpecific => keep = false,
                    Specificity::MoreSpecific => remove = true,
                    Specificity::Incomparable => {
                        if candidate.varargs != b.varargs {
                            if candidate.varargs {
                                keep = false;
                            } else if b.varargs {
                                remove = true;
                            }
                        }
                    }
                    Specificity::Equivalent => {}
                }
            }
            if remove {
                best.remove(i);
            } else {
                i += 1;
            }
        }
        if keep {
            best.push(candidate);
        }
    }
    match best.len() {
        0 => Ok(None),
        1 => Ok(Some(best[0].method)),
        _ => Err(Ambiguous),
    }
}

/// `ClassMap.MethodCache.makeMethodKey(Method)`: primitives spelled as their wrapper classes.
fn method_key(m: &JMethod) -> String {
    let mut key = m.name.to_string();
    for p in &m.params {
        key.push_str(&if p.is_primitive() { p.boxed().name() } else { p.name() });
    }
    key
}

/// A class's method cache (`ClassMap`): methods deduplicated by key (the first declaration wins), and
/// lookups by the arguments' classes.
pub struct ClassMethods {
    by_key: HashMap<String, usize>,
    methods: Vec<JMethod>,
    /// `MethodCache`: earlier lookups by name and argument classes (a null argument counts as `Object`), misses
    /// and ambiguities included.
    lookups: parking_lot::Mutex<HashMap<String, Option<JMethod>>>,
}

impl ClassMethods {
    /// The method cache of a class: its public methods (only those of public supertypes for a non-public class),
    /// deduplicated by key.
    pub fn new(def: &ClassDef) -> ClassMethods {
        let mut by_key = HashMap::new();
        let mut methods = Vec::new();
        for m in &def.methods {
            // `ClassMap`: a class that is not public only contributes what public supertypes declare.
            if !def.public && *m.declaring_class == *def.name {
                continue;
            }
            let key = method_key(m);
            if let std::collections::hash_map::Entry::Vacant(e) = by_key.entry(key) {
                e.insert(methods.len());
                methods.push(m.clone());
            }
        }
        ClassMethods { by_key, methods, lookups: parking_lot::Mutex::default() }
    }

    /// `ClassMap.findMethod(name, params)`.
    pub fn find(&self, name: &str, args: &[JValue]) -> Result<Option<JMethod>, Ambiguous> {
        let classes: Vec<Option<JType>> = args.iter().map(class_of).collect();
        let mut key = name.to_string();
        for c in &classes {
            key.push_str(&c.as_ref().map(JType::name).unwrap_or_else(|| "java.lang.Object".into()));
        }
        if let Some(&i) = self.by_key.get(&key) {
            return Ok(Some(self.methods[i].clone()));
        }
        let candidates: Vec<&JMethod> = self.methods.iter().filter(|m| &*m.name == name).collect();
        best_match(&candidates, &classes).map(|m| m.cloned())
    }

    /// `MethodCache.get(name, params)`: `find`, remembering the answer (a miss after an ambiguity too).
    pub fn find_cached(&self, name: &str, args: &[JValue]) -> Result<Option<JMethod>, Ambiguous> {
        let mut key = name.to_string();
        for a in args {
            key.push_str(&class_of(a).as_ref().map(JType::name).unwrap_or_else(|| "java.lang.Object".into()));
        }
        if let Some(cached) = self.lookups.lock().get(&key) {
            return Ok(cached.clone());
        }
        let found = self.find(name, args);
        self.lookups.lock().insert(key, found.clone().ok().flatten());
        found
    }
}

/// The method cache of a class definition, built once per definition.
pub fn class_methods(def: &Arc<ClassDef>) -> Arc<ClassMethods> {
    /// The caches built, by the definition's address; the definition is held so that the address is not reused.
    type Built = HashMap<usize, (Arc<ClassDef>, Arc<ClassMethods>)>;
    static CACHE: std::sync::LazyLock<parking_lot::Mutex<Built>> =
        std::sync::LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));
    let id = Arc::as_ptr(def) as usize;
    let mut cache = CACHE.lock();
    cache.entry(id).or_insert_with(|| (def.clone(), Arc::new(ClassMethods::new(def)))).1.clone()
}

fn number_format(message: String) -> JError {
    JError::new("java.lang.NumberFormatException", message)
}

fn for_input(s: &str) -> JError {
    number_format(format!("For input string: \"{s}\""))
}

/// `Integer.parseInt` / `Long.parseLong` family, radix 10.
pub fn parse_integral(s: &str, bits: u8) -> Result<i64, JError> {
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(if s.is_empty() { number_format("For input string: \"\"".into()) } else { for_input(s) });
    }
    let v: i128 = s.parse().map_err(|_| for_input(s))?;
    let (min, max) = match bits {
        8 => (i8::MIN as i128, i8::MAX as i128),
        16 => (i16::MIN as i128, i16::MAX as i128),
        32 => (i32::MIN as i128, i32::MAX as i128),
        _ => (i64::MIN as i128, i64::MAX as i128),
    };
    if v < min || v > max {
        return Err(match bits {
            8 | 16 => number_format(format!("Value out of range. Value:\"{s}\" Radix:10")),
            _ => for_input(s),
        });
    }
    Ok(v as i64)
}

/// `Double.parseDouble`: trimmed, with Java's float literal syntax (`1e3`, `NaN`, `Infinity`, `1f`, `0x1p3`).
pub fn parse_double(s: &str) -> Result<f64, JError> {
    let t = s.trim_matches(|c: char| c <= ' ');
    let body = t.strip_suffix(['d', 'D', 'f', 'F']).unwrap_or(t);
    let unsigned = body.strip_prefix(['+', '-']).unwrap_or(body);
    let negative = body.starts_with('-');
    let v = match unsigned {
        "NaN" => Some(f64::NAN),
        "Infinity" => Some(f64::INFINITY),
        _ if t.len() != body.len() && (unsigned == "NaN" || unsigned == "Infinity") => None,
        u if u.starts_with("0x") || u.starts_with("0X") => parse_hex_float(&u[2..]),
        u if !u.is_empty() && u.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-')) && u.chars().any(|c| c.is_ascii_digit()) => u.parse::<f64>().ok(),
        _ => None,
    };
    match v {
        Some(v) if unsigned == "NaN" => Ok(v),
        Some(v) => Ok(if negative { -v } else { v }),
        None => Err(if t.is_empty() { number_format("empty String".into()) } else { for_input(s) }),
    }
}

fn parse_hex_float(s: &str) -> Option<f64> {
    let (mantissa, exp) = s.split_once(['p', 'P'])?;
    let exp: i32 = exp.parse().ok()?;
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut v = 0f64;
    for c in int.chars() {
        v = v * 16.0 + c.to_digit(16)? as f64;
    }
    let mut scale = 1.0 / 16.0;
    for c in frac.chars() {
        v += c.to_digit(16)? as f64 * scale;
        scale /= 16.0;
    }
    Some(v * 2f64.powi(exp))
}

fn range_error(n: &JValue) -> JError {
    number_format(format!("value out of range: {}", n.to_java_string()))
}

fn arithmetic(message: &str) -> JError {
    JError::new("java.lang.ArithmeticException", message)
}

/// `Number.longValue()` semantics for the integral narrowing converters.
fn integral_value(n: &JValue) -> i64 {
    match n {
        JValue::Int(i) => *i as i64,
        JValue::Long(l) => *l,
        JValue::Float(f) => *f as i64,
        JValue::Double(d) => *d as i64,
        other => math::long_value(other),
    }
}

fn truncate(v: i64, bits: u8) -> i64 {
    match bits {
        8 => v as i8 as i64,
        16 => v as i16 as i64,
        32 => v as i32 as i64,
        _ => v,
    }
}

fn integral(v: i64, bits: u8) -> JValue {
    match bits {
        8 => JValue::Byte(v as i8),
        16 => JValue::Short(v as i16),
        32 => JValue::Int(v as i32),
        _ => JValue::Long(v),
    }
}

impl Converter {
    /// `Converter.convert`. A failing `*ValueExact` throws `ArithmeticException`, which `VelMethodImpl`
    /// rethrows as `IllegalArgumentException`.
    pub fn convert(self, v: &JValue) -> Result<JValue, JError> {
        use Converter::*;
        if v.is_null() && !matches!(self, ToString) {
            return Ok(JValue::Null);
        }
        Ok(match self {
            NumberToBoolean => JValue::Bool(match v {
                JValue::BigInteger(_) | JValue::BigDecimal(_) => !math::is_zero(v),
                // `Double.compare(d, 0.0) != 0`: `-0.0` and `NaN` are true.
                other => {
                    let d = math::double_value(other);
                    !(d == 0.0 && d.is_sign_positive())
                }
            }),
            CharToBoolean => JValue::Bool(!matches!(v, JValue::Char('\0'))),
            StringToBoolean => JValue::Bool(v.to_java_string().eq_ignore_ascii_case("true")),
            ToIntegral(bits, narrowing) => {
                if matches!(v, JValue::BigInteger(_) | JValue::BigDecimal(_)) {
                    let exact = math::exact_integral(v, bits).ok_or_else(|| {
                        arithmetic(match (v, bits) {
                            (JValue::BigInteger(_), 8) => "BigInteger out of byte range",
                            (JValue::BigInteger(_), 16) => "BigInteger out of short range",
                            (JValue::BigInteger(_), 32) => "BigInteger out of int range",
                            (JValue::BigInteger(_), _) => "BigInteger out of long range",
                            _ => "Overflow",
                        })
                    })?;
                    return Ok(integral(exact, bits));
                }
                if narrowing {
                    let d = math::double_value(v);
                    let (min, max) = match bits {
                        8 => (i8::MIN as f64, i8::MAX as f64),
                        16 => (i16::MIN as f64, i16::MAX as f64),
                        32 => (i32::MIN as f64, i32::MAX as f64),
                        _ => (i64::MIN as f64, i64::MAX as f64),
                    };
                    if d < min || d > max {
                        return Err(range_error(v));
                    }
                }
                integral(truncate(integral_value(v), bits), bits)
            }
            StringToIntegral(bits) => integral(parse_integral(&v.to_java_string(), bits)?, bits),
            BooleanToIntegral(bits) => integral(matches!(v, JValue::Bool(true)) as i64, bits),
            ToBigInteger => match v {
                JValue::BigDecimal(_) => JValue::BigInteger(math::big_decimal_to_big_integer_exact(v).ok_or_else(|| arithmetic("Rounding necessary"))?.into()),
                other => JValue::BigInteger(integral_value(other).to_string().into()),
            },
            StringToBigInteger => JValue::BigInteger(math::parse_big_integer(&v.to_java_string())?.into()),
            ToFloat => JValue::Float(math::double_value(v) as f32),
            StringToFloat => JValue::Float(parse_double(&v.to_java_string())? as f32),
            ToDouble => JValue::Double(math::double_value(v)),
            StringToDouble => JValue::Double(parse_double(&v.to_java_string())?),
            ExactToBigDecimal => JValue::BigDecimal(integral_value(v).to_string().into()),
            ApproxToBigDecimal => math::decimal_value(&math::Decimal::parse(&java_compat::double_to_string(math::double_value(v))).unwrap_or_default()),
            BigIntegerToBigDecimal => JValue::BigDecimal(v.to_java_string().into()),
            StringToBigDecimal => {
                let s = v.to_java_string();
                match math::Decimal::parse_java(&s) {
                    Some(d) => math::decimal_value(&d),
                    None => return Err(number_format(format!("Character {} is neither a decimal digit number, decimal point, nor \"e\" notation exponential mark.", s.chars().find(|c| !c.is_ascii_digit()).unwrap_or(' ')))),
                }
            }
            // `LocaleUtils.toLocale`, resolving its data as the JVM does.
            StringToLocale => java_compat::text::Locale::parse(&v.to_java_string())?.with_data(crate::tools::locale_data()).value(),
            ToString => JValue::str(v.to_java_string()),
        })
    }
}

/// Why a call failed before or inside the method.
#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    /// `IllegalArgumentException` from `Method.invoke` (a mismatched argument) or a converter.
    IllegalArgument(JError),
    /// The method threw (`InvocationTargetException`'s cause).
    Threw(JError),
}

/// `Method.invoke`'s own argument checks: unboxing and primitive widening for primitive parameters,
/// assignability for the others.
fn reflect_arg(param: &JType, arg: JValue) -> Result<JValue, JError> {
    let mismatch = || JError::new("java.lang.IllegalArgumentException", "argument type mismatch");
    if !param.is_primitive() {
        // Generic parameters are checked erased.
        let erased = match param {
            JType::Class(name) if name.contains('<') => JType::class(&name[..name.find('<').expect("checked")]),
            other => other.clone(),
        };
        return match class_of(&arg) {
            None => Ok(arg),
            Some(c) if erased.is_assignable_from(&c) => Ok(arg),
            Some(_) => Err(mismatch()),
        };
    }
    let actual = class_of(&arg).map(|c| c.unboxed());
    let Some(actual) = actual else { return Err(JError::without_message("java.lang.IllegalArgumentException")) };
    // `Method.invoke` unboxes, then widens.
    if actual == *param || primitive_widening(param, &actual) || matches!((param, &actual), (JType::Int | JType::Long | JType::Float | JType::Double, JType::Char)) {
        use JType::*;
        let integral = |v: &JValue| match v {
            JValue::Byte(b) => *b as i64,
            JValue::Short(x) => *x as i64,
            JValue::Int(i) => *i as i64,
            JValue::Long(l) => *l,
            JValue::Char(c) => *c as i64,
            _ => 0,
        };
        return Ok(match (param, arg) {
            (Short, v @ JValue::Byte(_)) => JValue::Short(integral(&v) as i16),
            (Int, v @ (JValue::Byte(_) | JValue::Short(_) | JValue::Char(_))) => JValue::Int(integral(&v) as i32),
            (Long, v @ (JValue::Byte(_) | JValue::Short(_) | JValue::Int(_) | JValue::Char(_))) => JValue::Long(integral(&v)),
            (Float, v @ (JValue::Byte(_) | JValue::Short(_) | JValue::Int(_) | JValue::Long(_) | JValue::Char(_))) => JValue::Float(integral(&v) as f32),
            (Double, v @ (JValue::Byte(_) | JValue::Short(_) | JValue::Int(_) | JValue::Long(_) | JValue::Char(_))) => JValue::Double(integral(&v) as f64),
            (Double, JValue::Float(f)) => JValue::Double(f as f64),
            (_, v) => v,
        });
    }
    Err(mismatch())
}

/// `Array.set` of an element into the array `VelMethodImpl` packs for a varargs parameter: a reference array takes
/// null or an instance of its component type, a primitive array unboxes and widens as `Method.invoke` does.
fn array_set(component: &JType, item: JValue) -> Result<JValue, JError> {
    if component.is_primitive() {
        return reflect_arg(component, item);
    }
    let erased = match component {
        JType::Class(name) if name.contains('<') => JType::class(&name[..name.find('<').expect("checked")]),
        other => other.clone(),
    };
    match class_of(&item) {
        Some(c) if !erased.is_assignable_from(&c) => Err(JError::new("java.lang.IllegalArgumentException", "array element type mismatch")),
        _ => Ok(item),
    }
}

/// `VelMethodImpl.invoke`: packs varargs, applies the conversions `getNeededConverters` chose, then calls.
pub fn invoke(method: &JMethod, target: &JValue, args: &[JValue]) -> Result<Option<JValue>, CallError> {
    let converters: Vec<Option<Conv>> =
        (0..method.params.len()).map(|i| args.get(i).filter(|a| !a.is_null()).and_then(|a| needed_converter(&method.params[i], class_of(a).as_ref()))).collect();
    let mut actual = args.to_vec();
    if let Some(JType::Array(component)) = method.params.last() {
        let index = method.params.len() - 1;
        if actual.len() == index {
            actual.push(JValue::Array(Arc::new(JArray::new((**component).clone(), Vec::new()))));
        } else if actual.len() == index + 1 && !actual[index].is_null() {
            let class = class_of(&actual[index]).expect("not null");
            if !class.is_array() && is_convertible(component, Some(&class), false) {
                let item = array_set(component, actual.pop().expect("an argument")).map_err(CallError::IllegalArgument)?;
                actual.push(JValue::Array(Arc::new(JArray::new((**component).clone(), vec![item]))));
            }
        } else if actual.len() > index + 1 {
            let rest = actual.split_off(index).into_iter().map(|a| array_set(component, a)).collect::<Result<Vec<_>, _>>().map_err(CallError::IllegalArgument)?;
            actual.push(JValue::Array(Arc::new(JArray::new((**component).clone(), rest))));
        }
    }
    for (i, a) in actual.iter_mut().enumerate() {
        if let Some(Some(c)) = converters.get(i) {
            *a = c.convert(a).map_err(|e| {
                if e.class == "java.lang.ArithmeticException" {
                    CallError::IllegalArgument(JError::new("java.lang.IllegalArgumentException", e.to_string()))
                } else {
                    CallError::Threw(e)
                }
            })?;
        }
    }
    if actual.len() != method.params.len() {
        return Err(CallError::IllegalArgument(JError::new(
            "java.lang.IllegalArgumentException",
            format!("wrong number of arguments: {} expected: {}", actual.len(), method.params.len()),
        )));
    }
    let mut checked = Vec::with_capacity(actual.len());
    for (p, a) in method.params.iter().zip(actual) {
        checked.push(reflect_arg(p, a).map_err(CallError::IllegalArgument)?);
    }
    (method.invoke)(target, &checked).map_err(CallError::Threw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(name: &str, params: &[JType]) -> JMethod {
        let n = name.to_string();
        let sig: Vec<String> = params.iter().map(JType::type_name).collect();
        let label = format!("{n}({})", sig.join(","));
        JMethod {
            name: name.into(),
            params: params.to_vec(),
            returns: Some(JType::string()),
            declaring_class: "T".into(),
            is_static: false,
            varargs: params.last().is_some_and(JType::is_array),
            invoke: Arc::new(move |_, _| Ok(Some(JValue::str(label.as_str())))),
        }
    }

    fn pick(methods: &[JMethod], args: &[JValue]) -> Result<Option<String>, Ambiguous> {
        let mut def = ClassDef::new("T");
        def.methods = methods.to_vec();
        let cm = ClassMethods::new(&def);
        cm.find(&methods[0].name, args).map(|m| m.map(|m| invoke(&m, &JValue::Null, args).unwrap().unwrap().to_java_string()))
    }

    #[test]
    fn fixed_arity_beats_varargs() {
        let o = JType::object();
        let methods = [m("add", &[o.clone(), o.clone()]), m("add", &[JType::array(o.clone())])];
        assert_eq!(pick(&methods, &[JValue::Int(1), JValue::Int(2)]).unwrap().unwrap(), "add(java.lang.Object,java.lang.Object)");
        assert_eq!(pick(&methods, &[JValue::Int(1), JValue::Int(2), JValue::Int(3)]).unwrap().unwrap(), "add(java.lang.Object[])");
    }

    #[test]
    fn primitive_and_string_conversions() {
        let o = JType::object();
        let methods = [m("truncate", &[o.clone(), JType::Int]), m("truncate", &[o.clone(), JType::string()])];
        assert_eq!(pick(&methods, &[JValue::str("x"), JValue::Int(8)]).unwrap().unwrap(), "truncate(java.lang.Object,int)");
        assert_eq!(pick(&methods, &[JValue::str("x"), JValue::str("8")]).unwrap().unwrap(), "truncate(java.lang.Object,java.lang.String)");
        let methods = [m("space", &[JType::Int])];
        assert_eq!(pick(&methods, &[JValue::str("8")]).unwrap().unwrap(), "space(int)");
        assert_eq!(pick(&methods, &[JValue::Long(8)]).unwrap().unwrap(), "space(int)");
    }

    #[test]
    fn packing_varargs_checks_element_types_as_array_set() {
        let join = m("join", &[JType::array(JType::string())]);
        let call = |args: &[JValue]| invoke(&join, &JValue::Null, args);
        assert!(call(&[JValue::str("a"), JValue::str("b")]).is_ok());
        assert!(call(&[JValue::str("a"), JValue::Null]).is_ok());
        let mismatch = CallError::IllegalArgument(JError::new("java.lang.IllegalArgumentException", "array element type mismatch"));
        assert_eq!(call(&[JValue::str("a"), JValue::Int(1)]).unwrap_err(), mismatch);
        // A primitive array unboxes and widens each element; null has no primitive value.
        let seen = Arc::new(parking_lot::Mutex::new(None));
        let s = seen.clone();
        let mut sum = m("sum", &[JType::array(JType::Long)]);
        sum.invoke = Arc::new(move |_, a| {
            *s.lock() = Some(a[0].clone());
            Ok(None)
        });
        assert!(invoke(&sum, &JValue::Null, &[JValue::Int(1), JValue::Long(2)]).is_ok());
        let JValue::Array(packed) = seen.lock().clone().unwrap() else { panic!("an array") };
        assert!(matches!(packed.items()[..], [JValue::Long(1), JValue::Long(2)]));
        assert_eq!(
            invoke(&sum, &JValue::Null, &[JValue::Int(1), JValue::Null]).unwrap_err(),
            CallError::IllegalArgument(JError::without_message("java.lang.IllegalArgumentException"))
        );
        assert_eq!(
            invoke(&sum, &JValue::Null, &[JValue::Int(1), JValue::str("2")]).unwrap_err(),
            CallError::IllegalArgument(JError::new("java.lang.IllegalArgumentException", "argument type mismatch"))
        );
    }

    #[test]
    fn parses_like_java() {
        assert_eq!(parse_integral("+12", 32).unwrap(), 12);
        assert!(parse_integral(" 1", 32).is_err());
        assert!(parse_integral("2147483648", 32).is_err());
        assert_eq!(parse_double(" 1e3 ").unwrap(), 1000.0);
        assert_eq!(parse_double("2f").unwrap(), 2.0);
        assert!(parse_double("1,5").is_err());
        assert_eq!(parse_double("0x1p3").unwrap(), 8.0);
    }
}
