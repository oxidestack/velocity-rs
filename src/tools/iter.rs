//! A `java.util.Iterator` over values the tools hand out (`XmlTool.iterator()`, `NodeIterator`).

use std::sync::Arc;

use java_compat::lang::{JError, JObject, JValue};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;

/// A `java.util.Iterator` over a fixed list of values, reporting `class` as its class name.
pub struct JIterator {
    class: &'static str,
    state: Mutex<(Vec<JValue>, usize)>,
}

impl JIterator {
    /// An iterator over `items` whose class is `class`.
    pub fn new(class: &'static str, items: Vec<JValue>) -> Arc<JIterator> {
        Arc::new(JIterator { class, state: Mutex::new((items, 0)) })
    }

    /// An iterator over `items` whose class is `class`, as a template value.
    pub fn value(class: &'static str, items: Vec<JValue>) -> JValue {
        JValue::Object(JIterator::new(class, items) as Arc<dyn JObject>)
    }

    /// `hasNext()`: whether `next()` has a value left.
    pub fn has_next(&self) -> bool {
        let s = self.state.lock();
        s.1 < s.0.len()
    }

    /// `next()`: `NoSuchElementException` past the end.
    pub fn next(&self) -> Result<JValue, JError> {
        let mut s = self.state.lock();
        if s.1 >= s.0.len() {
            return Err(JError::without_message("java.util.NoSuchElementException"));
        }
        s.1 += 1;
        Ok(s.0[s.1 - 1].clone())
    }
}

fn def() -> Arc<ClassDef> {
    ClassDef::new("java.util.Iterator").interface().register();
    ClassDef::new("org.apache.velocity.tools.iterator")
        .implements(&["java.util.Iterator"])
        .method::<JIterator>("hasNext", &[], Some(JType::Boolean), |i, _, _| Ok(Some(JValue::Bool(i.has_next()))))
        .method::<JIterator>("next", &[], Some(JType::object()), |i, _, _| Ok(Some(i.next()?)))
        .method::<JIterator>("remove", &[], None, |_, _, _| Err(JError::new("java.lang.UnsupportedOperationException", "remove")))
        .register()
}

impl JObject for JIterator {
    fn class_name(&self) -> String {
        self.class.into()
    }

    fn invoke(&self, _name: &str, _args: &[JValue]) -> Option<Result<Option<JValue>, JError>> {
        None
    }

    fn to_java_string(&self) -> String {
        java_compat::reflect::identity_string(self)
    }

    /// What is left to iterate, consumed (as a `#foreach` over an `Iterator` consumes it).
    fn iterate(&self) -> Option<Vec<JValue>> {
        let mut s = self.state.lock();
        let rest = s.0[s.1..].to_vec();
        s.1 = s.0.len();
        Some(rest)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<ClassDef>> {
        static DEF: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(def);
        Some(DEF.get())
    }
}
