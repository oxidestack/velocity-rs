//! `Alternator` and the deprecated `AlternatorTool` (`$alternator`): values taken in turn, shifting on each
//! `toString()` when automatic.

use std::sync::{Arc, Weak};

use java_compat::lang::{JMap, JObject, JValue};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;

use super::classes::{Annotation, JavaClass};
use super::safe::{self, SafeConfig, Tool};
use super::values::ValueParser;
use super::{c, identity, o, s, tool_object, val};

/// The class name of `org.apache.velocity.tools.generic.Alternator`.
pub const ALTERNATOR: &str = "org.apache.velocity.tools.generic.Alternator";
/// The class name of `org.apache.velocity.tools.generic.AlternatorTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.AlternatorTool";
/// `AlternatorTool.AUTO_ALTERNATE_DEFAULT_KEY`: the configuration key for the default auto mode of `make(list...)`.
pub const AUTO_ALTERNATE_DEFAULT_KEY: &str = "autoAlternate";

/// `Alternator`.
pub struct Alternator {
    list: Vec<JValue>,
    state: Mutex<(usize, bool)>,
}

fn is_collection(v: &JValue) -> bool {
    matches!(v, JValue::List(_)) || (matches!(v, JValue::Object(_)) && java_compat::reflect::class_def(v).is_some_and(|d| d.interfaces.iter().any(|i| &**i == "java.util.Collection")))
}

impl Alternator {
    /// `new Alternator(auto, list...)`: a lone collection stands for its elements.
    pub fn new(auto: bool, list: &[JValue]) -> Arc<Alternator> {
        let list = if list.len() == 1 && is_collection(&list[0]) { list[0].iterate().unwrap_or_default() } else { list.to_vec() };
        Arc::new(Alternator { list, state: Mutex::new((0, auto)) })
    }

    /// This alternator as a template value.
    pub fn value(self: &Arc<Self>) -> JValue {
        JValue::Object(self.clone() as Arc<dyn JObject>)
    }

    /// `isAuto()`: whether `toString()` shifts to the next value.
    pub fn is_auto(&self) -> bool {
        self.state.lock().1
    }

    /// `setAuto(auto)`: turns shifting on `toString()` on or off.
    pub fn set_auto(&self, auto: bool) {
        self.state.lock().1 = auto;
    }

    /// `shift()`: `ArithmeticException` for an empty list (`% 0`).
    pub fn shift(&self) {
        let mut st = self.state.lock();
        if !self.list.is_empty() {
            st.0 = (st.0 + 1) % self.list.len();
        }
    }

    /// `getCurrent()`: the current value, without shifting; `null` for an empty list.
    pub fn get_current(&self) -> JValue {
        self.list.get(self.state.lock().0).cloned().unwrap_or(JValue::Null)
    }

    /// `getNext()`: the current value, then shifts to the next one whatever the auto mode.
    pub fn get_next(&self) -> JValue {
        let o = self.get_current();
        self.shift();
        o
    }

    /// `toString()`: the current value's, shifting when automatic; `null` for a `null` value.
    pub fn java_string_opt(&self) -> Option<String> {
        let o = self.get_current();
        if self.is_auto() {
            self.shift();
        }
        (!o.is_null()).then(|| o.to_java_string())
    }

    fn java_string(&self) -> String {
        self.java_string_opt().unwrap_or_else(|| "null".into())
    }
}

fn alternator_def() -> Arc<ClassDef> {
    type A = Alternator;
    let def = ClassDef::new(ALTERNATOR)
        .method::<A>("isAuto", &[], Some(JType::Boolean), |a, _, _| val(JValue::Bool(a.is_auto())))
        .method::<A>("setAuto", &[JType::Boolean], None, |a, _, v| {
            a.set_auto(matches!(v[0], JValue::Bool(true)));
            Ok(None)
        })
        .method::<A>("shift", &[], None, |a, _, _| {
            if a.list.is_empty() {
                return Err(java_compat::lang::JError::new("java.lang.ArithmeticException", "/ by zero"));
            }
            a.shift();
            Ok(None)
        })
        .method::<A>("getCurrent", &[], Some(o()), |a, _, _| val(a.get_current()))
        .method::<A>("getNext", &[], Some(o()), |a, _, _| val(a.get_next()))
        .method::<A>("toString", &[], Some(s()), |a, _, _| super::text(a.java_string_opt()));
    super::object_methods::<A>(def).register()
}

tool_object!(Alternator, alternator_def);

/// `AlternatorTool` (deprecated).
pub struct AlternatorTool {
    safe: SafeConfig,
    auto_default: Mutex<bool>,
    me: Weak<AlternatorTool>,
}

impl AlternatorTool {
    /// `new AlternatorTool()`: unconfigured, auto-alternating by default.
    pub fn new() -> Arc<AlternatorTool> {
        Arc::new_cyclic(|me| AlternatorTool { safe: SafeConfig::default(), auto_default: Mutex::new(true), me: me.clone() })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: reads `autoAlternate`; nothing once the configuration is locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    /// `getAutoAlternateDefault()`: the auto mode `make(list...)` gives its alternators, `true` unless configured.
    pub fn get_auto_alternate_default(&self) -> bool {
        *self.auto_default.lock()
    }

    /// `setAutoAlternateDefault(b)`.
    pub fn set_auto_alternate_default(&self, b: bool) {
        *self.auto_default.lock() = b;
    }

    /// `make(list...)`: with the default auto mode.
    pub fn make(&self, list: Option<&[JValue]>) -> Option<Arc<Alternator>> {
        self.make_with(self.get_auto_alternate_default(), list)
    }

    /// `make(auto, list...)`: `null` for nothing (or one empty collection) to alternate.
    pub fn make_with(&self, auto: bool, list: Option<&[JValue]>) -> Option<Arc<Alternator>> {
        let list = list.filter(|l| !l.is_empty())?;
        if list.len() == 1 && is_collection(&list[0]) && list[0].iterate().unwrap_or_default().is_empty() {
            return None;
        }
        Some(Alternator::new(auto, list))
    }

    /// `auto(list...)`: an automatic alternator, `None` for nothing to alternate.
    pub fn auto(&self, list: Option<&[JValue]>) -> Option<Arc<Alternator>> {
        self.make_with(true, list)
    }

    /// `manual(list...)`: an alternator that shifts only on `getNext()`/`shift()`, `None` for nothing to alternate.
    pub fn manual(&self, list: Option<&[JValue]>) -> Option<Arc<Alternator>> {
        self.make_with(false, list)
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for AlternatorTool {
    fn safe(&self) -> &SafeConfig {
        &self.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        self.set_auto_alternate_default(values.get_boolean_or(AUTO_ALTERNATE_DEFAULT_KEY, true));
    }
}

fn varargs(v: &JValue) -> Option<Vec<JValue>> {
    match v {
        JValue::Array(a) => Some(a.items()),
        JValue::Null => None,
        other => Some(vec![other.clone()]),
    }
}

fn alt(a: Option<Arc<Alternator>>) -> super::Ret {
    val(a.map(|a| a.value()).unwrap_or(JValue::Null))
}

fn def() -> Arc<ClassDef> {
    type T = AlternatorTool;
    let objects = || JType::array(o());
    let ret = || c(ALTERNATOR);
    let def = ClassDef::new(CLASS)
        .extends(&[safe::SAFE_CONFIG])
        .implements(&["java.io.Serializable"])
        .method::<T>("getAutoAlternateDefault", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.get_auto_alternate_default())))
        .method::<T>("make", &[objects()], Some(ret()), |t, _, a| alt(t.make(varargs(&a[0]).as_deref())))
        .method::<T>("make", &[JType::Boolean, objects()], Some(ret()), |t, _, a| alt(t.make_with(matches!(a[0], JValue::Bool(true)), varargs(&a[1]).as_deref())))
        .method::<T>("auto", &[objects()], Some(ret()), |t, _, a| alt(t.auto(varargs(&a[0]).as_deref())))
        .method::<T>("manual", &[objects()], Some(ret()), |t, _, a| alt(t.manual(varargs(&a[0]).as_deref())));
    super::object_methods::<T>(safe::safe_config_methods::<T>(def)).register()
}

tool_object!(AlternatorTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || AlternatorTool::new().value())
        .annotated(Annotation::valid_scope(&[super::scope::APPLICATION]))
        .annotated(Annotation::default_key("alternator"))
        .annotated(Annotation::deprecated())
}
