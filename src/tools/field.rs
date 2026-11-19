//! `org.apache.velocity.tools.generic.FieldTool` (`$field`): the public static fields of classes, final ones
//! read once, the others each time they are asked for.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use java_compat::lang::{JError, JMap, JObject, JValue, MapKind};
use java_compat::reflect::ClassDef;
use parking_lot::Mutex;

use super::classes::{self, Annotation, Field, JavaClass, modifier::*};
use super::safe::{self, SafeConfig, Tool};
use super::values::ValueParser;
use super::{c, identity, o, s, tool_object, val};

/// The class name of `org.apache.velocity.tools.generic.FieldTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.FieldTool";
/// The class name of `FieldTool.FieldToolSub`.
pub const SUB: &str = "org.apache.velocity.tools.generic.FieldTool$FieldToolSub";
/// `FieldTool.INCLUDE_KEY`: the configuration key listing classes whose constants are loaded up front.
pub const INCLUDE_KEY: &str = "include";
/// `FieldTool.STORE_DYNAMIC_KEY`: the configuration key for keeping the constants of classes looked up later.
pub const STORE_DYNAMIC_KEY: &str = "storeDynamicLookups";

/// A field's value: read once (final) or on each lookup (`MutableField`).
#[derive(Clone)]
enum Entry {
    Value(JValue),
    Mutable(Field),
}

impl Entry {
    fn get(&self) -> JValue {
        match self {
            Entry::Value(v) => v.clone(),
            Entry::Mutable(f) => f.get.as_ref().map(|g| g()).unwrap_or(JValue::Null),
        }
    }
}

/// Port of `org.apache.velocity.tools.generic.FieldTool` (`$field`): public static fields of classes, by name or full
/// path.
pub struct FieldTool {
    safe: SafeConfig,
    storage: Mutex<HashMap<String, Entry>>,
    store_dynamic: Mutex<bool>,
    me: Weak<FieldTool>,
}

/// `inspect(clazz)`: the public static fields, the later ones (superclasses) overriding the earlier.
fn inspect(class: &JavaClass) -> Vec<(String, Entry)> {
    let mut out: Vec<(String, Entry)> = Vec::new();
    for f in class.public_fields() {
        if f.modifiers & STATIC != 0 && f.modifiers & PUBLIC != 0 {
            let e = if f.modifiers & FINAL != 0 { Entry::Value(f.get.as_ref().map(|g| g()).unwrap_or(JValue::Null)) } else { Entry::Mutable(f.clone()) };
            out.retain(|(n, _)| *n != f.name);
            out.push((f.name.clone(), e));
        }
    }
    out
}

impl FieldTool {
    /// `new FieldTool()`: nothing stored, dynamic lookups stored.
    pub fn new() -> Arc<FieldTool> {
        Arc::new_cyclic(|me| FieldTool { safe: SafeConfig::default(), storage: Mutex::new(HashMap::new()), store_dynamic: Mutex::new(true), me: me.clone() })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: loads the `include` classes (`RuntimeException` for one not found) and reads
    /// `storeDynamicLookups`; nothing once locked.
    pub fn configure(&self, params: &JMap) -> Result<(), JError> {
        safe::try_configure(self, params)
    }

    /// `get(name)`: a stored field, else a full `package.Class.FIELD` path.
    pub fn get(&self, name: &str) -> JValue {
        if let Some(e) = self.storage.lock().get(name).cloned() {
            return e.get();
        }
        if name.find('.').is_some_and(|i| i > 0) {
            match classes::field_value(name) {
                Ok(v) => return v,
                Err(e) => super::log::debug(CLASS, &format!("Unable to retrieve value of field at {name}: {e}")),
            }
        }
        JValue::Null
    }

    /// `in(classname)`: `null` for an unknown class.
    pub fn in_class_named(&self, name: &str) -> Option<FieldToolSub> {
        classes::for_name(name).map(|c| self.in_class(&c))
    }

    /// `in(instance)`: the instance's class.
    pub fn in_object(&self, obj: &JValue) -> Option<FieldToolSub> {
        if obj.is_null() {
            return None;
        }
        classes::class_of(obj).map(|c| self.in_class(&c))
    }

    /// `in(clazz)`: the class's constants, stored for `get` when dynamic lookups are stored.
    pub fn in_class(&self, class: &JavaClass) -> FieldToolSub {
        let results = inspect(class);
        if *self.store_dynamic.lock() && !results.is_empty() {
            let mut s = self.storage.lock();
            for (k, v) in &results {
                s.insert(k.clone(), v.clone());
            }
        }
        FieldToolSub { results }
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for FieldTool {
    fn safe(&self) -> &SafeConfig {
        &self.safe
    }

    fn try_configure_values(&self, values: &ValueParser) -> Result<(), JError> {
        if let Some(names) = values.get_strings(INCLUDE_KEY) {
            for name in names.into_iter().flatten() {
                if self.in_class_named(&name).is_none() {
                    return Err(JError::new("java.lang.RuntimeException", format!("Could not find {name} in the classpath")));
                }
            }
        }
        let store = values.get_boolean_or(STORE_DYNAMIC_KEY, *self.store_dynamic.lock());
        *self.store_dynamic.lock() = store;
        Ok(())
    }
}

/// `FieldTool.FieldToolSub`: one class's constants.
#[derive(Clone)]
pub struct FieldToolSub {
    results: Vec<(String, Entry)>,
}

impl FieldToolSub {
    /// `get(name)`: the class's field of that name, `null` when it has none.
    pub fn get(&self, name: &str) -> JValue {
        self.results.iter().find(|(n, _)| n == name).map(|(_, e)| e.get()).unwrap_or(JValue::Null)
    }

    /// This class's constants as a template value.
    pub fn value(self) -> JValue {
        JValue::Object(Arc::new(self) as Arc<dyn JObject>)
    }

    /// `toString()`: the results map (`HashMap` order).
    fn java_string(&self) -> String {
        let m = JMap::new(MapKind::Hash);
        for (k, e) in &self.results {
            let v = match e {
                Entry::Value(v) => v.clone(),
                Entry::Mutable(_) => JValue::str(format!("org.apache.velocity.tools.generic.FieldTool$MutableField@{:x}", k.len())),
            };
            m.put_str(k, v);
        }
        JValue::Map(m).to_java_string()
    }
}

fn sub_def() -> Arc<ClassDef> {
    type F = FieldToolSub;
    let def = ClassDef::new(SUB).method::<F>("get", &[s()], Some(o()), |t, _, a| val(t.get(&a[0].to_java_string()))).method::<F>("toString", &[], Some(s()), |t, _, _| val(JValue::str(t.java_string())));
    super::object_methods::<F>(def).register()
}

tool_object!(FieldToolSub, sub_def);

fn sub(v: Option<FieldToolSub>) -> super::Ret {
    val(v.map(FieldToolSub::value).unwrap_or(JValue::Null))
}

fn def() -> Arc<ClassDef> {
    type T = FieldTool;
    let def = ClassDef::new(CLASS)
        .extends(&[safe::SAFE_CONFIG])
        .method::<T>("get", &[s()], Some(o()), |t, _, a| val(t.get(&a[0].to_java_string())))
        .method::<T>("in", &[s()], Some(c(SUB)), |t, _, a| sub(a[0].as_str().and_then(|n| t.in_class_named(n))))
        .method::<T>("in", &[o()], Some(c(SUB)), |t, _, a| sub(t.in_object(&a[0])))
        .method::<T>("in", &[c("java.lang.Class")], Some(c(SUB)), |t, _, a| sub(classes::as_class(&a[0]).map(|c| t.in_class(&c))));
    super::object_methods::<T>(safe::safe_config_methods::<T>(def)).register()
}

tool_object!(FieldTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || FieldTool::new().value()).annotated(Annotation::default_key("field")).annotated(Annotation::valid_scope(&[super::scope::APPLICATION]))
}
