//! `org.apache.velocity.util.introspection`'s pluggable half: the `Uberspect` the engine asks for methods, property
//! getters and setters, and iterators (`introspector.uberspect.class`), the executors it returns, `UberspectImpl`,
//! `SecureUberspector`, `DeprecatedCheckUberspector`, and how configured uberspectors chain or link.
//!
//! A value with a typed class definition ([`ClassDef`]) resolves exactly as `ClassMap` / `MethodMap` would. A Rust
//! value without one (strings, lists, maps, numbers) resolves when invoked; its getters, which have no side effects,
//! are resolved eagerly so that a missing one is `None` like in Velocity.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, OnceLock};

use java_compat::lang::JValue;
use java_compat::reflect::{ClassDef, JMethod, JType};
use parking_lot::RwLock;

use crate::events::Info;
pub use crate::introspect::Outcome;
use crate::introspect::{self, JClass};
use crate::log::{LogExt, Logger};
use crate::{Engine, VelocityError};

type R<T> = Result<T, VelocityError>;

/// `VelMethod`: a method found for some arguments.
pub trait VelMethod: Send + Sync {
    /// `invoke(o, params)`: calls the method on `target` with `args`.
    fn invoke(&self, target: &JValue, args: &[JValue]) -> Outcome;

    /// `getMethodName()`.
    fn method_name(&self) -> String;

    /// `getMethod()`: the reflected method, when the class is typed.
    fn method(&self) -> Option<&JMethod> {
        None
    }
}

/// `VelPropertyGet`: how `$obj.identifier` reads.
pub trait VelPropertyGet: Send + Sync {
    /// `invoke(o)`: reads the property from `target`.
    fn invoke(&self, target: &JValue) -> Outcome;

    /// `getMethodName()`.
    fn method_name(&self) -> String;

    /// `getMethod()`: the reflected getter, when the class is typed.
    fn method(&self) -> Option<&JMethod> {
        None
    }
}

/// `VelPropertySet`: how `#set($obj.identifier = …)` writes.
pub trait VelPropertySet: Send + Sync {
    /// `invoke(o, value)`: writes `value` to the property of `target`.
    fn invoke(&self, target: &JValue, value: &JValue) -> Outcome;

    /// `getMethodName()`.
    fn method_name(&self) -> String;

    /// `getMethod()`: the reflected setter, when the class is typed.
    fn method(&self) -> Option<&JMethod> {
        None
    }
}

/// `org.apache.velocity.util.introspection.Uberspect`. An `Err` is an exception the uberspector throws itself.
pub trait Uberspect: Send + Sync {
    /// `init()`, once the runtime exists (`RuntimeServicesAware`).
    fn init(&self, _rsvc: &Engine) -> R<()> {
        Ok(())
    }

    /// `getIterator(obj, info)`: the elements `#foreach` walks, or `None`.
    fn get_iterator(&self, obj: &JValue, info: &Info) -> R<Option<Iteration>>;

    /// `getMethod(obj, methodName, args, info)`: the method `name` that takes `args`, or `None`.
    fn get_method(&self, obj: &JValue, name: &str, args: &[JValue], info: &Info) -> R<Option<Arc<dyn VelMethod>>>;

    /// `getPropertyGet(obj, identifier, info)`: how `$obj.identifier` reads (a getter, `get("identifier")`, a field, ...),
    /// or `None`.
    fn get_property_get(&self, obj: &JValue, identifier: Option<&str>, info: &Info) -> R<Option<Arc<dyn VelPropertyGet>>>;

    /// `getPropertySet(obj, identifier, arg, info)`: how `identifier` is set to `arg`, or `None`.
    fn get_property_set(&self, obj: &JValue, identifier: Option<&str>, arg: &JValue, info: &Info) -> R<Option<Arc<dyn VelPropertySet>>>;
}

/// What `getIterator` returns: the elements, or an `Iterator` object walked through `hasNext()` / `next()`.
pub enum Iteration {
    /// The elements, already collected.
    Items(std::collections::VecDeque<JValue>),
    /// A Java `Iterator` object, walked through `hasNext()` / `next()` calls.
    Lazy(JValue),
    /// An `IntegerRange`'s iterator: `next` up to `last` by `step`, never materialised (`[1..2147483647]` holds
    /// two thousand million elements).
    Range {
        /// The next value.
        next: i64,
        /// The last value, inclusive.
        last: i64,
        /// The step, `1` or `-1`.
        step: i64,
    },
}

impl Iteration {
    /// The elements of `v`, in order.
    pub fn items(v: Vec<JValue>) -> Iteration {
        Iteration::Items(v.into())
    }

    /// `hasNext()`; `Err` when a lazy iterator's `hasNext()` throws.
    pub fn has_next(&mut self) -> R<bool> {
        match self {
            Iteration::Items(v) => Ok(!v.is_empty()),
            Iteration::Range { next, last, step } => Ok((*next - *last) * *step <= 0),
            Iteration::Lazy(o) => match introspect::call(o, "hasNext", &[]) {
                Some(Ok(Some(JValue::Bool(b)))) => Ok(b),
                Some(Err(e)) => Err(VelocityError::velocity(e.to_string())),
                _ => Ok(false),
            },
        }
    }

    /// Java's `Iterator.next()`, paired with `has_next` as `hasNext()`; it can throw, which `Iterator::next` cannot carry.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> R<JValue> {
        match self {
            Iteration::Items(v) => Ok(v.pop_front().unwrap_or(JValue::Null)),
            Iteration::Range { next, last, step } => {
                if (*next - *last) * *step > 0 {
                    return Ok(JValue::Null);
                }
                let value = *next;
                *next += *step;
                Ok(JValue::Int(value as i32))
            }
            Iteration::Lazy(o) => match introspect::call(o, "next", &[]) {
                Some(Ok(v)) => Ok(v.unwrap_or(JValue::Null)),
                Some(Err(e)) => Err(VelocityError::velocity(e.to_string())),
                None => Ok(JValue::Null),
            },
        }
    }
}

/// An object implementing `java.util.Iterator` through its typed class.
pub fn is_lazy_iterator(v: &JValue) -> bool {
    java_compat::reflect::class_def(v).is_some_and(|d| d.interfaces.iter().any(|i| &**i == "java.util.Iterator") && d.methods_named("hasNext").next().is_some())
}

// ---- executors -----------------------------------------------------------------------------------------------

fn outcome(r: Result<Option<JValue>, crate::uberspect::CallError>) -> Outcome {
    match r {
        Ok(v) => Outcome::Returned(v),
        Err(crate::uberspect::CallError::Threw(e)) => Outcome::Threw(e),
        Err(crate::uberspect::CallError::IllegalArgument(e)) => Outcome::IllegalArgument(e),
    }
}

/// `VelMethodImpl` over a reflected method (static ones are called without a target).
struct Reflected(JMethod);

impl VelMethod for Reflected {
    fn invoke(&self, target: &JValue, args: &[JValue]) -> Outcome {
        let target = if self.0.is_static { &JValue::Null } else { target };
        outcome(crate::uberspect::invoke(&self.0, target, args))
    }

    fn method_name(&self) -> String {
        self.0.name.to_string()
    }

    fn method(&self) -> Option<&JMethod> {
        Some(&self.0)
    }
}

/// A method of a Rust value, looked up when invoked (`NotFound` then if there is none).
struct Deferred(String);

impl VelMethod for Deferred {
    fn invoke(&self, target: &JValue, args: &[JValue]) -> Outcome {
        introspect::method(target, &self.0, args)
    }

    fn method_name(&self) -> String {
        self.0.clone()
    }
}

/// `PropertyExecutor` / `GetExecutor` / `BooleanPropertyExecutor` over a reflected method, with the arguments the
/// executor passes (the identifier for `get(String)`).
struct ReflectedGetter {
    method: JMethod,
    args: Vec<JValue>,
}

impl VelPropertyGet for ReflectedGetter {
    fn invoke(&self, target: &JValue) -> Outcome {
        outcome(crate::uberspect::invoke(&self.method, target, &self.args))
    }

    fn method_name(&self) -> String {
        self.method.name.to_string()
    }

    fn method(&self) -> Option<&JMethod> {
        Some(&self.method)
    }
}

/// `MapGetExecutor`.
struct MapGetter(String);

impl VelPropertyGet for MapGetter {
    fn invoke(&self, target: &JValue) -> Outcome {
        Outcome::Returned(Some(introspect::map_get(target, &self.0).unwrap_or(JValue::Null)))
    }

    fn method_name(&self) -> String {
        "get".into()
    }
}

/// A getter of a Rust value: the executor chain run again when invoked.
struct DeferredGetter {
    identifier: String,
    method: String,
}

impl VelPropertyGet for DeferredGetter {
    fn invoke(&self, target: &JValue) -> Outcome {
        introspect::getter(target, &self.identifier).0
    }

    fn method_name(&self) -> String {
        self.method.clone()
    }
}

/// `SetPropertyExecutor` / `PutExecutor` over a reflected method: `set(value)`, `put(value)` or `put(key, value)`.
struct ReflectedSetter {
    method: JMethod,
    key: Option<String>,
}

impl VelPropertySet for ReflectedSetter {
    fn invoke(&self, target: &JValue, value: &JValue) -> Outcome {
        let args = match &self.key {
            Some(k) => vec![JValue::str(k.as_str()), value.clone()],
            None => vec![value.clone()],
        };
        outcome(crate::uberspect::invoke(&self.method, target, &args))
    }

    fn method_name(&self) -> String {
        self.method.name.to_string()
    }

    fn method(&self) -> Option<&JMethod> {
        Some(&self.method)
    }
}

/// `MapSetExecutor`.
struct MapSetter(String);

impl VelPropertySet for MapSetter {
    fn invoke(&self, target: &JValue, value: &JValue) -> Outcome {
        match target {
            JValue::Map(m) => Outcome::Returned(Some(m.put(JValue::str(self.0.as_str()), value.clone()))),
            _ => Outcome::NotFound,
        }
    }

    fn method_name(&self) -> String {
        "put".into()
    }
}

/// A setter of a Rust value, looked up when invoked.
struct DeferredSetter(String);

impl VelPropertySet for DeferredSetter {
    fn invoke(&self, target: &JValue, value: &JValue) -> Outcome {
        match introspect::set_property(target, &self.0, value.clone()) {
            Ok(true) => Outcome::Returned(None),
            Ok(false) => Outcome::NotFound,
            Err(e) => Outcome::Threw(e),
        }
    }

    fn method_name(&self) -> String {
        format!("set{}", self.0)
    }
}

// ---- UberspectImpl -------------------------------------------------------------------------------------------

/// `Character.isLowerCase` flip of the character at `at` (the executors' second guess).
fn flip_at(name: &str, at: usize) -> String {
    let (head, tail) = name.split_at(at);
    let mut chars = tail.chars();
    match chars.next() {
        Some(c) if c.is_lowercase() => format!("{head}{}{}", c.to_uppercase(), chars.as_str()),
        Some(c) => format!("{head}{}{}", c.to_lowercase(), chars.as_str()),
        None => name.to_string(),
    }
}

/// `ClassMap`: a class's public methods and its method cache.
pub type ClassMap = crate::uberspect::ClassMethods;

/// The class maps built, by the definition's address; the definition is held so that the address is not reused.
type ClassMaps = HashMap<usize, (Arc<ClassDef>, Arc<ClassMap>)>;

/// `org.apache.velocity.util.introspection.Introspector`: class maps by class (`IntrospectorCache`), and ambiguity
/// logged rather than thrown.
#[derive(Default)]
pub struct Introspector {
    log: OnceLock<Arc<dyn Logger>>,
    classes: parking_lot::Mutex<ClassMaps>,
}

impl Introspector {
    /// An introspector logging ambiguous method calls to `log`, when given.
    pub fn new(log: Option<Arc<dyn Logger>>) -> Introspector {
        let i = Introspector::default();
        if let Some(l) = log {
            let _ = i.log.set(l);
        }
        i
    }

    /// The class map of `class`, created on first use.
    pub fn class_map(&self, class: &Arc<ClassDef>) -> Arc<ClassMap> {
        let mut classes = self.classes.lock();
        classes.entry(Arc::as_ptr(class) as usize).or_insert_with(|| (class.clone(), Arc::new(ClassMap::new(class)))).1.clone()
    }

    /// `getMethod(c, name, params)`: `None` when there is none or the call is ambiguous (logged).
    pub fn get_method(&self, class: &Arc<ClassDef>, name: &str, args: &[JValue]) -> Option<JMethod> {
        match self.class_map(class).find_cached(name, args) {
            Ok(m) => m,
            Err(_) => {
                if let Some(log) = self.log.get() {
                    log.debug(|| {
                        let params: Vec<String> = args.iter().map(|a| if a.is_null() { "null".to_string() } else { a.class_name() }).collect();
                        format!("Introspection Error : Ambiguous method invocation {name}({}) for class {}", params.join(", "), class.name)
                    });
                }
                None
            }
        }
    }
}

/// `Object.getClass()`.
fn get_class() -> JMethod {
    JMethod {
        name: "getClass".into(),
        params: Vec::new(),
        returns: Some(JType::class("java.lang.Class")),
        declaring_class: "java.lang.Object".into(),
        is_static: false,
        varargs: false,
        invoke: Arc::new(|target, _| {
            Ok(Some(JValue::Object(Arc::new(JClass { name: target.class_name(), def: java_compat::reflect::class_def(target), ty: java_compat::reflect::class_of(target) }))))
        }),
    }
}

/// `org.apache.velocity.util.introspection.UberspectImpl`.
#[derive(Default)]
pub struct UberspectImpl {
    log: OnceLock<Arc<dyn Logger>>,
    introspector: Introspector,
}

impl UberspectImpl {
    /// `new UberspectImpl()`: its logger set by `init`.
    pub fn new() -> UberspectImpl {
        UberspectImpl::default()
    }

    fn log(&self) -> Option<&Arc<dyn Logger>> {
        self.log.get()
    }

    /// `introspector.getMethod(clazz, name, params)` on a typed class; `Object.getClass()` is always there.
    fn find(&self, def: &Arc<ClassDef>, name: &str, args: &[JValue]) -> Option<JMethod> {
        self.introspector.get_method(def, name, args).or_else(|| ((name, args.len()) == ("getClass", 0)).then(get_class))
    }
}

impl Uberspect for UberspectImpl {
    fn init(&self, rsvc: &Engine) -> R<()> {
        let _ = self.log.set(rsvc.log_for("rendering"));
        let _ = self.introspector.log.set(rsvc.log_for("introspection"));
        Ok(())
    }

    fn get_iterator(&self, obj: &JValue, info: &Info) -> R<Option<Iteration>> {
        match obj {
            JValue::Array(a) => return Ok(Some(Iteration::items(a.items()))),
            JValue::List(l) => {
                if l.kind() == java_compat::lang::ListKind::Iterator {
                    if let Some(log) = self.log() {
                        log.debug(|| {
                            format!(
                                "The iterative object in the #foreach() loop at {} is of type java.util.Iterator.  Because it is not resettable, if used in more than once it may lead to unexpected results.",
                                info.java_string()
                            )
                        });
                    }
                }
                return Ok(Some(Iteration::items(l.items())));
            }
            JValue::Map(m) => return Ok(Some(Iteration::items(m.entries().into_iter().map(|(_, v)| v).collect()))),
            _ => {}
        }
        if let Some((from, to)) = crate::range::bounds(obj) {
            return Ok(Some(Iteration::Range { next: from as i64, last: to as i64, step: if from <= to { 1 } else { -1 } }));
        }
        if let Some(items) = obj.iterate() {
            return Ok(Some(Iteration::items(items)));
        }
        if is_lazy_iterator(obj) {
            return Ok(Some(Iteration::Lazy(obj.clone())));
        }
        // A public `iterator()` returning an `Iterator`, on a public class.
        if let Some(def) = java_compat::reflect::class_def(obj) {
            let Some(m) = def.methods.iter().find(|m| &*m.name == "iterator" && m.params.is_empty()).cloned() else { return Ok(None) };
            if !m.returns.as_ref().is_some_and(|r| JType::class("java.util.Iterator").is_assignable_from(r)) {
                if let Some(log) = self.log() {
                    log.debug(|| format!("iterator() method of reference in #foreach loop at {} does not return a true Iterator.", info.java_string()));
                }
                return Ok(None);
            }
            if !def.public {
                // `IllegalAccessException`: the class is not accessible.
                return Ok(None);
            }
            return match crate::uberspect::invoke(&m, obj, &[]) {
                Ok(Some(it)) => Ok(match it.iterate() {
                    Some(items) => Some(Iteration::items(items)),
                    None if is_lazy_iterator(&it) => Some(Iteration::Lazy(it)),
                    None => None,
                }),
                Ok(None) => Ok(None),
                Err(crate::uberspect::CallError::Threw(e) | crate::uberspect::CallError::IllegalArgument(e)) => Err(VelocityError::velocity(format!(
                    "Error invoking the method 'iterator' on class '{}'",
                    obj.class_name()
                ))
                .with_cause(e)),
            };
        }
        match introspect::method(obj, "iterator", &[]) {
            Outcome::Returned(Some(it @ JValue::List(_))) => Ok(it.iterate().map(Iteration::items)),
            Outcome::Returned(Some(o @ JValue::Object(_))) => Ok(match o.iterate() {
                Some(items) => Some(Iteration::items(items)),
                None if is_lazy_iterator(&o) => Some(Iteration::Lazy(o)),
                None => None,
            }),
            _ => {
                if let Some(log) = self.log() {
                    log.debug(|| format!("Could not determine type of iterator in #foreach loop at {}", info.java_string()));
                }
                Ok(None)
            }
        }
    }

    fn get_method(&self, obj: &JValue, name: &str, args: &[JValue], _: &Info) -> R<Option<Arc<dyn VelMethod>>> {
        if obj.is_null() {
            return Ok(None);
        }
        if let JValue::Object(o) = obj {
            if let Some(c) = o.as_any().downcast_ref::<JClass>() {
                // `Class`'s own methods, then the class's static ones (VELOCITY-102).
                if matches!((name, args.len()), ("getName", 0) | ("isPrimitive", 0) | ("toString", 0)) {
                    return Ok(Some(Arc::new(Deferred(name.into()))));
                }
                let Some(def) = &c.def else { return Ok(None) };
                let mut statics = (**def).clone();
                statics.methods.retain(|m| m.is_static);
                return Ok(crate::uberspect::ClassMethods::new(&statics).find(name, args).ok().flatten().map(|m| Arc::new(Reflected(m)) as Arc<dyn VelMethod>));
            }
        }
        if (name, args.len()) == ("getClass", 0) {
            return Ok(Some(Arc::new(Deferred(name.into()))));
        }
        if let Some(def) = java_compat::reflect::class_def(obj) {
            return Ok(self.find(&def, name, args).map(|m| Arc::new(Reflected(m)) as Arc<dyn VelMethod>));
        }
        Ok(Some(Arc::new(Deferred(name.into()))))
    }

    fn get_property_get(&self, obj: &JValue, identifier: Option<&str>, _: &Info) -> R<Option<Arc<dyn VelPropertyGet>>> {
        if obj.is_null() {
            return Ok(None);
        }
        let Some(def) = java_compat::reflect::class_def(obj) else {
            // A Rust value: the executor chain, run now to see whether it resolves.
            let Some(id) = identifier else { return Ok(None) };
            return Ok(match introspect::getter(obj, id) {
                (Outcome::NotFound, _) => None,
                (_, method) => Some(Arc::new(DeferredGetter { identifier: id.to_string(), method })),
            });
        };
        let getter = |method: JMethod, args: Vec<JValue>| Some(Arc::new(ReflectedGetter { method, args }) as Arc<dyn VelPropertyGet>);
        // `PropertyExecutor`: `get<id>`, `get<Id>` (the first character's case flipped), then `<id>()`.
        if let Some(id) = identifier.filter(|i| !i.is_empty()) {
            let get = format!("get{id}");
            for name in [get.clone(), flip_at(&get, 3), id.to_string()] {
                if let Some(m) = self.find(&def, &name, &[]) {
                    return Ok(getter(m, vec![]));
                }
            }
        }
        // `MapGetExecutor`.
        if let Some(id) = identifier {
            if introspect::map_get(obj, id).is_some() {
                return Ok(Some(Arc::new(MapGetter(id.to_string()))));
            }
        }
        // `GetExecutor`: `get(identifier)` (or `get()` without one), not taking a primitive or a `Number`.
        let args: Vec<JValue> = identifier.map(|i| vec![JValue::str(i)]).unwrap_or_default();
        if let Some(m) = self.find(&def, "get", &args) {
            let numeric = m.params.len() == 1 && (m.params[0].is_primitive() || JType::class("java.lang.Number").is_assignable_from(&m.params[0]));
            if !numeric {
                return Ok(getter(m, args));
            }
        }
        // `BooleanPropertyExecutor`: `is<id>` / `is<Id>` returning a boolean.
        if let Some(id) = identifier.filter(|i| !i.is_empty()) {
            let is = format!("is{id}");
            for name in [is.clone(), flip_at(&is, 2)] {
                if let Some(m) = self.find(&def, &name, &[]) {
                    if matches!(&m.returns, Some(JType::Boolean)) || m.returns.as_ref().is_some_and(|r| *r == JType::class("java.lang.Boolean")) {
                        return Ok(getter(m, vec![]));
                    }
                    break;
                }
            }
        }
        Ok(None)
    }

    fn get_property_set(&self, obj: &JValue, identifier: Option<&str>, arg: &JValue, _: &Info) -> R<Option<Arc<dyn VelPropertySet>>> {
        if obj.is_null() {
            return Ok(None);
        }
        let Some(def) = java_compat::reflect::class_def(obj) else {
            return Ok(match (obj, identifier) {
                (JValue::Map(_), Some(id)) => Some(Arc::new(MapSetter(id.to_string()))),
                (JValue::Map(_), None) => None,
                (_, Some(id)) => Some(Arc::new(DeferredSetter(id.to_string()))),
                (_, None) => None,
            });
        };
        // `SetPropertyExecutor`: `set<id>(arg)`, then `set<Id>(arg)`.
        if let Some(id) = identifier.filter(|i| !i.is_empty()) {
            let set = format!("set{id}");
            for name in [set.clone(), flip_at(&set, 3)] {
                if let Some(m) = self.find(&def, &name, std::slice::from_ref(arg)) {
                    return Ok(Some(Arc::new(ReflectedSetter { method: m, key: None })));
                }
            }
        }
        // `MapSetExecutor`: a class implementing `java.util.Map` directly.
        if identifier.is_some() && def.interfaces.iter().any(|i| &**i == "java.util.Map") {
            if let Some(m) = self.find(&def, "put", &[JValue::Null, JValue::Null]) {
                return Ok(Some(Arc::new(ReflectedSetter { method: m, key: identifier.map(str::to_string) })));
            }
        }
        // `PutExecutor`: `put(identifier, arg)`, or `put(arg)` without an identifier.
        let args: Vec<JValue> = match identifier {
            Some(id) => vec![JValue::str(id), arg.clone()],
            None => vec![arg.clone()],
        };
        Ok(self.find(&def, "put", &args).map(|m| Arc::new(ReflectedSetter { method: m, key: identifier.map(str::to_string) }) as Arc<dyn VelPropertySet>))
    }
}

// ---- SecureUberspector ---------------------------------------------------------------------------------------

/// `SecureUberspector`: `UberspectImpl` whose introspector refuses methods of restricted classes and packages
/// (`SecureIntrospectorImpl`).
#[derive(Default)]
pub struct SecureUberspector {
    inner: UberspectImpl,
    restrictions: OnceLock<(Vec<String>, Vec<String>)>,
}

impl SecureUberspector {
    /// `new SecureUberspector()`: its restricted classes and packages read by `init`.
    pub fn new() -> SecureUberspector {
        SecureUberspector::default()
    }

    /// The class of a value as the check sees it, with its supertypes.
    fn class_of(obj: &JValue) -> (String, Vec<String>) {
        match java_compat::reflect::class_def(obj) {
            Some(def) => (def.name.to_string(), def.superclasses.iter().chain(def.interfaces.iter()).map(|s| s.to_string()).collect()),
            None => (obj.class_name(), Vec::new()),
        }
    }

    /// `checkObjectExecutePermission(clazz, methodName)`.
    pub fn check_object_execute_permission(&self, obj: &JValue, method: Option<&str>) -> bool {
        if method.is_some_and(|m| m == "wait" || m == "notify") {
            return false;
        }
        // Numbers, booleans and strings are always allowed.
        if matches!(
            obj,
            JValue::Byte(_) | JValue::Short(_) | JValue::Int(_) | JValue::Long(_) | JValue::Float(_) | JValue::Double(_) | JValue::BigInteger(_) | JValue::BigDecimal(_) | JValue::Bool(_) | JValue::Str(_)
        ) {
            return true;
        }
        let (class, supers) = SecureUberspector::class_of(obj);
        let is = |c: &str| class == c || supers.iter().any(|s| s == c);
        if is("java.lang.Number") || is("java.lang.Boolean") || is("java.lang.String") {
            return true;
        }
        // Always `Class.getName()`; never `ClassLoader`, `Thread` or their subclasses.
        if is("java.lang.Class") && method == Some("getName") {
            return true;
        }
        if is("java.lang.ClassLoader") || is("java.lang.Thread") {
            return false;
        }
        let class = match class.strip_prefix("[L").and_then(|c| c.strip_suffix(';')) {
            Some(c) => c.to_string(),
            None => class,
        };
        let package = class.rfind('.').map(|i| &class[..i]).unwrap_or("");
        let (packages, classes) = self.restrictions.get().cloned().unwrap_or_default();
        !(packages.iter().any(|p| p == package) || classes.contains(&class))
    }

    fn refused(&self, obj: &JValue, method: &str) {
        if let Some(log) = self.inner.log() {
            log.warn(|| format!("Cannot retrieve method {method} from object of class {} due to security restrictions.", obj.class_name()));
        }
    }
}

impl Uberspect for SecureUberspector {
    fn init(&self, rsvc: &Engine) -> R<()> {
        let props = &rsvc.config.props;
        let _ = self.restrictions.set((props.string_array(crate::config::INTROSPECTOR_RESTRICT_PACKAGES), props.string_array(crate::config::INTROSPECTOR_RESTRICT_CLASSES)));
        self.inner.init(rsvc)
    }

    fn get_iterator(&self, obj: &JValue, info: &Info) -> R<Option<Iteration>> {
        if obj.is_null() {
            return Ok(None);
        }
        if self.check_object_execute_permission(obj, None) {
            return self.inner.get_iterator(obj, info);
        }
        if let Some(log) = self.inner.log() {
            log.warn(|| format!("Cannot retrieve iterator from {} due to security restrictions.", obj.class_name()));
        }
        Ok(None)
    }

    fn get_method(&self, obj: &JValue, name: &str, args: &[JValue], info: &Info) -> R<Option<Arc<dyn VelMethod>>> {
        if !obj.is_null() && !self.check_object_execute_permission(obj, Some(name)) {
            self.refused(obj, name);
            return Ok(None);
        }
        self.inner.get_method(obj, name, args, info)
    }

    fn get_property_get(&self, obj: &JValue, identifier: Option<&str>, info: &Info) -> R<Option<Arc<dyn VelPropertyGet>>> {
        let getter = self.inner.get_property_get(obj, identifier, info)?;
        Ok(getter.filter(|g| {
            // The executors look their methods up through the secure introspector (maps are read directly).
            let allowed = g.method().is_none() && matches!(obj, JValue::Map(_)) || self.check_object_execute_permission(obj, Some(&g.method_name()));
            if !allowed {
                self.refused(obj, &g.method_name());
            }
            allowed
        }))
    }

    fn get_property_set(&self, obj: &JValue, identifier: Option<&str>, arg: &JValue, info: &Info) -> R<Option<Arc<dyn VelPropertySet>>> {
        let setter = self.inner.get_property_set(obj, identifier, arg, info)?;
        Ok(setter.filter(|s| {
            let allowed = matches!(obj, JValue::Map(_)) || self.check_object_execute_permission(obj, Some(&s.method_name()));
            if !allowed {
                self.refused(obj, &s.method_name());
            }
            allowed
        }))
    }
}

// ---- chaining ------------------------------------------------------------------------------------------------

/// `LinkingUberspector`: the left uberspector's answer, else the right one's.
pub struct LinkingUberspector {
    left: Arc<dyn Uberspect>,
    right: Arc<dyn Uberspect>,
}

impl LinkingUberspector {
    /// `new LinkingUberspector(left, right)`.
    pub fn new(left: Arc<dyn Uberspect>, right: Arc<dyn Uberspect>) -> LinkingUberspector {
        LinkingUberspector { left, right }
    }
}

impl Uberspect for LinkingUberspector {
    fn init(&self, rsvc: &Engine) -> R<()> {
        self.left.init(rsvc)?;
        self.right.init(rsvc)
    }

    fn get_iterator(&self, obj: &JValue, info: &Info) -> R<Option<Iteration>> {
        match self.left.get_iterator(obj, info)? {
            Some(it) => Ok(Some(it)),
            None => self.right.get_iterator(obj, info),
        }
    }

    fn get_method(&self, obj: &JValue, name: &str, args: &[JValue], info: &Info) -> R<Option<Arc<dyn VelMethod>>> {
        match self.left.get_method(obj, name, args, info)? {
            Some(m) => Ok(Some(m)),
            None => self.right.get_method(obj, name, args, info),
        }
    }

    fn get_property_get(&self, obj: &JValue, identifier: Option<&str>, info: &Info) -> R<Option<Arc<dyn VelPropertyGet>>> {
        match self.left.get_property_get(obj, identifier, info)? {
            Some(g) => Ok(Some(g)),
            None => self.right.get_property_get(obj, identifier, info),
        }
    }

    fn get_property_set(&self, obj: &JValue, identifier: Option<&str>, arg: &JValue, info: &Info) -> R<Option<Arc<dyn VelPropertySet>>> {
        match self.left.get_property_set(obj, identifier, arg, info)? {
            Some(s) => Ok(Some(s)),
            None => self.right.get_property_set(obj, identifier, arg, info),
        }
    }
}

/// `DeprecatedCheckUberspector`: logs every use of a `@Deprecated` method, getter or setter, or one of a deprecated
/// class, then answers like the uberspector it wraps.
pub struct DeprecatedCheckUberspector {
    inner: Arc<dyn Uberspect>,
    log: OnceLock<Arc<dyn Logger>>,
}

impl DeprecatedCheckUberspector {
    /// `new DeprecatedCheckUberspector(inner)`: wraps `inner`, its logger set by `init`.
    pub fn wrap(inner: Arc<dyn Uberspect>) -> DeprecatedCheckUberspector {
        DeprecatedCheckUberspector { inner, log: OnceLock::new() }
    }

    fn check(&self, kind: &str, obj: &JValue, method: Option<&JMethod>, method_name: &str, info: &Info) {
        let Some(m) = method else { return };
        let obj_def = java_compat::reflect::class_def(obj);
        let declaring = java_compat::reflect::class_named(&m.declaring_class);
        let deprecated = |d: &Option<Arc<ClassDef>>| d.as_ref().is_some_and(|d| d.deprecated || d.deprecated_methods.iter().any(|n| **n == *m.name));
        let class_deprecated = obj_def.as_ref().is_some_and(|d| d.deprecated);
        if deprecated(&declaring) || deprecated(&obj_def) || class_deprecated {
            if let Some(log) = self.log.get() {
                // `getCanonicalName()`: nested classes joined with dots.
                let canonical = obj.class_name().replace('$', ".");
                log.warn(|| format!("Deprecated usage of {kind} [{canonical}.{method_name}] in {}@{},{}", info.template, info.line, info.column));
            }
        }
    }
}

impl Uberspect for DeprecatedCheckUberspector {
    fn init(&self, rsvc: &Engine) -> R<()> {
        self.inner.init(rsvc)?;
        let _ = self.log.set(rsvc.log_for("rendering"));
        Ok(())
    }

    fn get_iterator(&self, obj: &JValue, info: &Info) -> R<Option<Iteration>> {
        self.inner.get_iterator(obj, info)
    }

    fn get_method(&self, obj: &JValue, name: &str, args: &[JValue], info: &Info) -> R<Option<Arc<dyn VelMethod>>> {
        let m = self.inner.get_method(obj, name, args, info)?;
        if let Some(m) = &m {
            self.check("method", obj, m.method(), &m.method_name(), info);
        }
        Ok(m)
    }

    fn get_property_get(&self, obj: &JValue, identifier: Option<&str>, info: &Info) -> R<Option<Arc<dyn VelPropertyGet>>> {
        let g = self.inner.get_property_get(obj, identifier, info)?;
        if let Some(g) = &g {
            self.check("getter", obj, g.method(), &g.method_name(), info);
        }
        Ok(g)
    }

    fn get_property_set(&self, obj: &JValue, identifier: Option<&str>, arg: &JValue, info: &Info) -> R<Option<Arc<dyn VelPropertySet>>> {
        let s = self.inner.get_property_set(obj, identifier, arg, info)?;
        if let Some(s) = &s {
            self.check("setter", obj, s.method(), &s.method_name(), info);
        }
        Ok(s)
    }
}

// ---- configuration -------------------------------------------------------------------------------------------

/// Makes a plain `Uberspect`.
pub type PlainFactory = Arc<dyn Fn() -> Arc<dyn Uberspect> + Send + Sync>;

/// Makes a `ChainableUberspector` around the previous ones.
pub type ChainableFactory = Arc<dyn Fn(Arc<dyn Uberspect>) -> Arc<dyn Uberspect> + Send + Sync>;

/// How a configured uberspector class combines with the ones before it.
#[derive(Clone)]
pub enum UberspectClass {
    /// A plain `Uberspect`: the first one, or linked after the previous ones (`LinkingUberspector`).
    Plain(PlainFactory),
    /// A `ChainableUberspector`: wraps the previous ones.
    Chainable(ChainableFactory),
}

/// `UberspectImpl`'s class name, the default `introspector.uberspect.class`.
pub const UBERSPECT_IMPL: &str = "org.apache.velocity.util.introspection.UberspectImpl";
/// `DeprecatedCheckUberspector`'s class name.
pub const DEPRECATED_CHECK_UBERSPECTOR: &str = "org.apache.velocity.util.introspection.DeprecatedCheckUberspector";

static UBERSPECT_CLASSES: LazyLock<RwLock<HashMap<String, UberspectClass>>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(UBERSPECT_IMPL.to_string(), UberspectClass::Plain(Arc::new(|| Arc::new(UberspectImpl::new()) as Arc<dyn Uberspect>)));
    m.insert(crate::config::SECURE_UBERSPECTOR.to_string(), UberspectClass::Plain(Arc::new(|| Arc::new(SecureUberspector::new()) as Arc<dyn Uberspect>)));
    m.insert(DEPRECATED_CHECK_UBERSPECTOR.to_string(), UberspectClass::Chainable(Arc::new(|inner| Arc::new(DeprecatedCheckUberspector::wrap(inner)) as Arc<dyn Uberspect>)));
    RwLock::new(m)
});

/// Makes an uberspector class known to `introspector.uberspect.class`.
pub fn register_uberspect_class(class: &str, uberspect: UberspectClass) {
    UBERSPECT_CLASSES.write().insert(class.to_string(), uberspect);
}

/// `RuntimeInstance.initializeIntrospection`: each configured class in turn, chained or linked.
pub(crate) fn configured(classes: &[String]) -> R<Arc<dyn Uberspect>> {
    let mut uberspect: Option<Arc<dyn Uberspect>> = None;
    for class in classes {
        let Some(c) = UBERSPECT_CLASSES.read().get(class).cloned() else {
            return Err(VelocityError::velocity(format!("The specified class for Uberspect ({class}) does not exist or is not accessible to the current classloader.")));
        };
        uberspect = Some(match (c, uberspect) {
            (UberspectClass::Plain(f), None) => f(),
            (UberspectClass::Plain(f), Some(previous)) => Arc::new(LinkingUberspector::new(previous, f())),
            (UberspectClass::Chainable(f), Some(previous)) => f(previous),
            // A chainable one first wraps nothing (`inner` stays null in Velocity).
            (UberspectClass::Chainable(f), None) => f(Arc::new(UberspectImpl::new())),
        });
    }
    uberspect.ok_or_else(|| VelocityError::velocity("It appears that no class was specified as the Uberspect. Please ensure that all configuration information is correct."))
}


/// `ASTMethod.MethodCacheKey`: a method name, its arguments' classes (`None` for a null argument) and whether the
/// target is a `Class` object (whose static methods are looked up), the key of the context's introspection cache.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MethodCacheKey {
    /// The method's name.
    pub method_name: String,
    /// The arguments' class names, `None` for a null argument.
    pub params: Option<Vec<Option<String>>>,
    /// Whether the target is a `Class` object, whose static methods are looked up.
    pub class_object: bool,
}

impl MethodCacheKey {
    /// `new MethodCacheKey(methodName, params, classObject)`.
    pub fn new(method_name: &str, params: Option<Vec<Option<String>>>, class_object: bool) -> MethodCacheKey {
        MethodCacheKey { method_name: method_name.to_string(), params, class_object }
    }
}

/// `IntrospectionCacheData`: the class a method was found for, and the method.
#[derive(Clone)]
pub struct IntrospectionCacheData {
    /// The class name the method was found for (a `Class` target's own name).
    pub context_data: String,
    /// The method found.
    pub thingy: Arc<dyn VelMethod>,
}
