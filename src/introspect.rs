//! `SecureUberspector` over Java values: property getters (`PropertyExecutor`, `MapGetExecutor`,
//! `GetExecutor`, `BooleanPropertyExecutor`), method calls, iteration, and `DuckType`'s coercions.

use std::sync::Arc;

use java_compat::lang::{JArray, JError, JList, JObject, JValue, ListKind};
use java_compat::methods;

use crate::math;

/// A method call's outcome: `None` when there is no such method.
pub type Found = Option<Result<Option<JValue>, JError>>;

/// A `java.lang.Class`: its name, its type, and its definition when it has one (for its static methods).
pub struct JClass {
    /// `getName()`.
    pub name: String,
    /// The class's definition, for its static methods, when it has one.
    pub def: Option<Arc<java_compat::reflect::ClassDef>>,
    /// The class as a type, when it is one the JDK model knows.
    pub ty: Option<java_compat::reflect::JType>,
}

impl JClass {
    /// The class object of a class definition (`Foo.class`).
    pub fn of(def: Arc<java_compat::reflect::ClassDef>) -> JValue {
        JValue::Object(Arc::new(JClass { name: def.name.to_string(), ty: Some(java_compat::reflect::JType::class(&def.name)), def: Some(def) }))
    }

    /// The class object of a type (`int.class`, `Integer.class`).
    pub fn of_type(ty: java_compat::reflect::JType) -> JValue {
        let def = java_compat::reflect::class_named(&ty.name());
        JValue::Object(Arc::new(JClass { name: ty.name(), def, ty: Some(ty) }))
    }
}

impl JObject for JClass {
    fn class_name(&self) -> String {
        "java.lang.Class".into()
    }

    fn invoke(&self, name: &str, args: &[JValue]) -> Found {
        match (name, args) {
            ("getName", []) => Some(Ok(Some(JValue::str(self.name.as_str())))),
            ("isPrimitive", []) => Some(Ok(Some(JValue::Bool(self.ty.as_ref().is_some_and(|t| t.is_primitive()))))),
            ("toString", []) => Some(Ok(Some(JValue::str(self.to_java_string())))),
            _ => None,
        }
    }

    /// `Class.toString()`: `int`, `interface java.util.List`, `class java.lang.String`.
    fn to_java_string(&self) -> String {
        if self.ty.as_ref().is_some_and(|t| t.is_primitive()) {
            return self.name.clone();
        }
        if self.def.as_ref().is_some_and(|d| d.is_interface) {
            return format!("interface {}", self.name);
        }
        format!("class {}", self.name)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// What looking up and invoking a method gave (`VelMethod.invoke`'s outcomes).
#[derive(Debug, Clone)]
pub enum Outcome {
    /// No public method takes these arguments (or the call is ambiguous).
    NotFound,
    /// The result; `None` for a `void` method.
    Returned(Option<JValue>),
    /// The method threw (`InvocationTargetException`'s target).
    Threw(JError),
    /// `Method.invoke` refused the arguments, or a conversion failed (`IllegalArgumentException`).
    IllegalArgument(JError),
}

impl Outcome {
    fn found(self) -> Found {
        match self {
            Outcome::NotFound => None,
            Outcome::Returned(v) => Some(Ok(v)),
            Outcome::Threw(e) | Outcome::IllegalArgument(e) => Some(Err(e)),
        }
    }

    fn from_found(f: Found) -> Outcome {
        match f {
            None => Outcome::NotFound,
            Some(Ok(v)) => Outcome::Returned(v),
            Some(Err(e)) => Outcome::Threw(e),
        }
    }

    /// Whether a method was found (it returned or threw).
    pub fn is_found(&self) -> bool {
        !matches!(self, Outcome::NotFound)
    }
}

/// `SecureIntrospectorImpl.checkObjectExecutePermission` plus `introspector.getMethod`, then the invocation.
pub fn method(target: &JValue, name: &str, args: &[JValue]) -> Outcome {
    if let JValue::Object(o) = target {
        if let Some(c) = o.as_any().downcast_ref::<JClass>() {
            // `Class`'s own methods, then the class's static ones (VELOCITY-102).
            let found = o.invoke(name, args);
            if found.is_some() {
                return Outcome::from_found(found);
            }
            let Some(def) = &c.def else { return Outcome::NotFound };
            let mut statics = (**def).clone();
            statics.methods.retain(|m| m.is_static);
            return match crate::uberspect::ClassMethods::new(&statics).find(name, args) {
                Ok(Some(m)) => match crate::uberspect::invoke(&m, &JValue::Null, args) {
                    Ok(v) => Outcome::Returned(v),
                    Err(crate::uberspect::CallError::Threw(e)) => Outcome::Threw(e),
                    Err(crate::uberspect::CallError::IllegalArgument(e)) => Outcome::IllegalArgument(e),
                },
                Ok(None) | Err(_) => Outcome::NotFound,
            };
        }
    }
    if (name, args.len()) == ("getClass", 0) && !target.is_null() {
        return Outcome::Returned(Some(JValue::Object(Arc::new(JClass { name: target.class_name(), def: java_compat::reflect::class_def(target), ty: java_compat::reflect::class_of(target) }))));
    }
    if let Some(def) = java_compat::reflect::class_def(target) {
        // A typed class: `ClassMap` / `MethodMap` resolution; ambiguity finds nothing (`Introspector.getMethod`).
        return match crate::uberspect::class_methods(&def).find(name, args) {
            Ok(Some(m)) => match crate::uberspect::invoke(&m, target, args) {
                Ok(v) => Outcome::Returned(v),
                Err(crate::uberspect::CallError::Threw(e)) => Outcome::Threw(e),
                Err(crate::uberspect::CallError::IllegalArgument(e)) => Outcome::IllegalArgument(e),
            },
            Ok(None) | Err(_) => Outcome::NotFound,
        };
    }
    let found = methods::invoke(target, name, args);
    if found.is_some() {
        return Outcome::from_found(found);
    }
    // Arrays are introspected through `ArrayListWrapper`.
    if let JValue::Array(a) = target {
        return array_wrapper(a, name, args);
    }
    Outcome::NotFound
}

/// `ArrayListWrapper`'s `get`, `set`, `size` and `isEmpty` over a Java array, then `AbstractList`'s methods.
fn array_wrapper(a: &Arc<java_compat::lang::JArray>, name: &str, args: &[JValue]) -> Outcome {
    let index = |v: &JValue| methods::as_int(v);
    let out_of_bounds = |i: i64| JError::new("java.lang.ArrayIndexOutOfBoundsException", format!("Index {i} out of bounds for length {}", a.len()));
    match (name, args) {
        ("get", [i]) => match index(i) {
            Some(i) => match usize::try_from(i).ok().and_then(|u| a.get(u)) {
                Some(v) => Outcome::Returned(Some(v)),
                None => Outcome::Threw(out_of_bounds(i)),
            },
            None => Outcome::NotFound,
        },
        ("set", [i, v]) => match index(i) {
            Some(i) => {
                let old = usize::try_from(i).ok().and_then(|u| a.get(u));
                match old {
                    Some(old) if a.set(i as usize, v.clone()) => Outcome::Returned(Some(old)),
                    _ => Outcome::Threw(out_of_bounds(i)),
                }
            }
            None => Outcome::NotFound,
        },
        ("size", []) => Outcome::Returned(Some(JValue::Int(a.len() as i32))),
        ("isEmpty", []) => Outcome::Returned(Some(JValue::Bool(a.is_empty()))),
        _ => Outcome::from_found(methods::invoke(&JValue::List(JList::new(ListKind::Fixed, a.items())), name, args)),
    }
}

/// `SecureIntrospectorImpl.checkObjectExecutePermission` plus `introspector.getMethod`: invokes `name` on
/// `target` if a public method takes these arguments.
pub fn call(target: &JValue, name: &str, args: &[JValue]) -> Found {
    method(target, name, args).found()
}

/// Whether `target` is a `java.util.Map` (`MapGetExecutor`).
pub(crate) fn map_get(target: &JValue, key: &str) -> Option<JValue> {
    match target {
        JValue::Map(m) => Some(m.get_str(key).unwrap_or(JValue::Null)),
        JValue::Object(o) => o.as_any().downcast_ref::<crate::scope::Scope>().map(|s| s.get_str(key)),
        _ => None,
    }
}

fn flip_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_lowercase() => c.to_uppercase().chain(chars).collect(),
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Whether `GetExecutor` accepts the `get` method this object resolves for a `String` argument: not one taking a
/// primitive or a `Number`.
fn get_executor_allowed(target: &JValue) -> bool {
    match java_compat::reflect::class_def(target) {
        Some(def) => match crate::uberspect::class_methods(&def).find("get", &[JValue::str("")]) {
            Ok(Some(m)) => !(m.params.len() == 1 && (m.params[0].is_primitive() || java_compat::reflect::JType::class("java.lang.Number").is_assignable_from(&m.params[0]))),
            _ => true,
        },
        None => !matches!(target, JValue::List(_) | JValue::Array(_)),
    }
}

/// `getPropertyGet(obj, identifier).invoke(obj)`: the outcome and the executor's method name (`getMethodName()`).
pub fn getter(target: &JValue, name: &str) -> (Outcome, String) {
    if target.is_null() {
        return (Outcome::NotFound, String::new());
    }
    // `PropertyExecutor`: `get<name>`, `get<Name>` (the first character's case flipped), then `<name>()`.
    for m in [format!("get{name}"), format!("get{}", flip_first(name)), name.to_string()] {
        let o = method(target, &m, &[]);
        if o.is_found() {
            return (o, m);
        }
    }
    if let Some(v) = map_get(target, name) {
        return (Outcome::Returned(Some(v)), "get".into());
    }
    // `GetExecutor`: `get(String)`, unless its parameter is numeric (a list's `get(int)`).
    if get_executor_allowed(target) {
        let o = method(target, "get", &[JValue::str(name)]);
        if o.is_found() {
            return (o, "get".into());
        }
    }
    // `BooleanPropertyExecutor`: `is<name>` / `is<Name>` returning a boolean.
    for m in [format!("is{name}"), format!("is{}", flip_first(name))] {
        let o = method(target, &m, &[]);
        if matches!(o, Outcome::Returned(Some(JValue::Bool(_))) | Outcome::Threw(_) | Outcome::IllegalArgument(_)) {
            return (o, m);
        }
    }
    (Outcome::NotFound, String::new())
}


/// `getPropertySet(obj, identifier, value).invoke(obj, value)`: `false` when nothing can set it.
pub fn set_property(target: &JValue, name: &str, value: JValue) -> Result<bool, JError> {
    for method in [format!("set{name}"), format!("set{}", flip_first(name))] {
        if let Some(r) = call(target, &method, std::slice::from_ref(&value)) {
            return r.map(|_| true);
        }
    }
    if let JValue::Map(m) = target {
        m.put(JValue::str(name), value);
        return Ok(true);
    }
    if let Some(r) = call(target, "put", &[JValue::str(name), value]) {
        return r.map(|_| true);
    }
    Ok(false)
}

/// `DuckType.get(value, type)`: the no-argument method a public class in the value's hierarchy declares, invoked;
/// `None` for `NO_METHOD`. A method that returns `null` (or is `void`) gives `Some(Null)`.
fn duck(v: &JValue, method: &str) -> Option<JValue> {
    if let Some(def) = java_compat::reflect::class_def(v) {
        let m = def.methods.iter().find(|m| &*m.name == method && m.params.is_empty() && !(!def.public && *m.declaring_class == *def.name))?;
        return Some(crate::uberspect::invoke(m, v, &[]).ok().flatten().unwrap_or(JValue::Null));
    }
    match v {
        // `Object`'s and the boxed types' own methods are not duck-typing hooks.
        JValue::Object(o) => match o.invoke(method, &[]) {
            Some(Ok(Some(r))) => Some(r),
            Some(Ok(None)) => Some(JValue::Null),
            _ => None,
        },
        _ => match methods::invoke(v, method, &[]) {
            Some(Ok(Some(r))) => Some(r),
            Some(Ok(None)) => Some(JValue::Null),
            _ => None,
        },
    }
}

/// `DuckType.asString(value)`: arrays print their elements; `getAsString()` when declared; else `toString()`.
pub fn as_string(v: &JValue) -> Option<String> {
    match v {
        JValue::Null => None,
        JValue::Str(s) => Some(s.to_string()),
        JValue::Array(a) => Some(array_string(a)),
        other => match duck(other, "getAsString") {
            Some(JValue::Null) => None,
            Some(s) => Some(s.to_java_string()),
            None => java_to_string(other),
        },
    }
}

/// `toString()`, which a class may make return null.
pub fn java_to_string(v: &JValue) -> Option<String> {
    if let Some(def) = java_compat::reflect::class_def(v) {
        if let Ok(Some(m)) = crate::uberspect::class_methods(&def).find("toString", &[]) {
            return match crate::uberspect::invoke(&m, v, &[]) {
                Ok(Some(JValue::Null)) | Ok(None) => None,
                Ok(Some(s)) => Some(s.to_java_string()),
                Err(_) => Some(v.to_java_string()),
            };
        }
    }
    Some(v.to_java_string())
}

fn array_string(a: &JArray) -> String {
    let items: Vec<String> = a.items().iter().map(|i| as_string(i).unwrap_or_else(|| "null".into())).collect();
    format!("[{}]", items.join(", "))
}

/// `DuckType.asString(value, false)`: only strings (and `getAsString()` objects) are strings.
pub fn as_string_strict(v: &JValue) -> Option<String> {
    match v {
        JValue::Null => None,
        JValue::Str(s) => Some(s.to_string()),
        other => match duck(other, "getAsString") {
            Some(JValue::Null) | None => None,
            Some(s) => Some(s.to_java_string()),
        },
    }
}

/// `DuckType.asEmpty`.
pub fn as_empty(v: &JValue) -> bool {
    match v {
        JValue::Null => return true,
        JValue::Array(a) => return a.items().is_empty(),
        _ => {}
    }
    if let Some(e) = duck(v, "isEmpty") {
        return matches!(e, JValue::Bool(true));
    }
    if let Some(n) = duck(v, "length").filter(math::is_number) {
        return math::is_zero(&n);
    }
    if let Some(n) = duck(v, "size").filter(math::is_number) {
        return math::is_zero(&n);
    }
    if math::is_number(v) {
        return math::is_zero(v);
    }
    match duck(v, "getAsString") {
        Some(JValue::Null) => return true,
        Some(s) => return s.to_java_string().is_empty(),
        None => {}
    }
    match duck(v, "getAsNumber") {
        Some(JValue::Null) => true,
        Some(n) if math::is_number(&n) => math::is_zero(&n),
        _ => false,
    }
}

/// `DuckType.asBoolean(value, coerceType)`.
pub fn as_boolean(v: &JValue, coerce: bool) -> bool {
    match v {
        JValue::Null => false,
        JValue::Bool(b) => *b,
        other => match duck(other, "getAsBoolean") {
            Some(b) => matches!(b, JValue::Bool(true)),
            None => !coerce || !as_empty(other),
        },
    }
}

/// `DuckType.asNumber(value)`: numbers, `getAsNumber()`, else `new BigDecimal(asString(value))`.
pub fn as_number(v: &JValue) -> Option<JValue> {
    if v.is_null() {
        return None;
    }
    if math::is_number(v) {
        return Some(v.clone());
    }
    if let Some(n) = duck(v, "getAsNumber") {
        return (!n.is_null()).then_some(n);
    }
    let s = as_string(v)?;
    math::Decimal::parse(&s).map(|d| math::decimal_value(&d))
}

/// `DuckType.asNull`: `null`, or a duck null (`getAsString()`/`getAsNumber()` returning null).
pub fn as_null(v: &JValue) -> bool {
    v.is_null() || matches!(duck(v, "getAsString"), Some(JValue::Null)) || matches!(duck(v, "getAsNumber"), Some(JValue::Null))
}

/// `isAssignableFrom` either way, for `==` between two non-numbers: same class family.
pub fn same_class_family(a: &JValue, b: &JValue) -> bool {
    use JValue::*;
    match (a, b) {
        (Str(_), Str(_)) | (Bool(_), Bool(_)) | (Char(_), Char(_)) => true,
        (List(_), List(_)) | (Map(_), Map(_)) => a.class_name() == b.class_name(),
        (Object(_), Object(_)) => a.class_name() == b.class_name(),
        _ => false,
    }
}
