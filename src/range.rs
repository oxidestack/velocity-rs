//! `ASTIntegerRange.IntegerRange`: the immutable list `[n..m]` evaluates to (`runtime.immutable_ranges`).

use std::sync::Arc;

use java_compat::lang::{JError, JList, JObject, JValue, ListKind, java_equals};

/// The integers from `from` to `to`, either way, inclusive.
pub struct IntegerRange {
    from: i32,
    to: i32,
}

/// The class name `$range.class.name` shows.
pub const CLASS: &str = "org.apache.velocity.runtime.parser.node.ASTIntegerRange$IntegerRange";

/// Its place in the class hierarchy (an `AbstractList`), for assignability.
static HIERARCHY: std::sync::LazyLock<Arc<java_compat::reflect::ClassDef>> = std::sync::LazyLock::new(|| {
    java_compat::reflect::ClassDef::new(CLASS)
        .extends(&["java.util.AbstractList", "java.util.AbstractCollection"])
        .implements(&["java.util.List", "java.util.SequencedCollection", "java.util.Collection", "java.lang.Iterable"])
        .register()
});

/// The ends of an `IntegerRange`, for iterating it without materialising it.
pub fn bounds(v: &JValue) -> Option<(i32, i32)> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<IntegerRange>().map(|r| (r.from, r.to)),
        _ => None,
    }
}

impl IntegerRange {
    /// The range `[from..to]` as a context value, counting down when `from > to`.
    pub fn value(from: i32, to: i32) -> JValue {
        std::sync::LazyLock::force(&HIERARCHY);
        JValue::Object(Arc::new(IntegerRange { from, to }))
    }

    fn size(&self) -> i32 {
        (self.to - self.from).abs() + 1
    }

    fn items(&self) -> Vec<JValue> {
        if self.from <= self.to { (self.from..=self.to).map(JValue::Int).collect() } else { (self.to..=self.from).rev().map(JValue::Int).collect() }
    }
}

fn unsupported() -> Option<Result<Option<JValue>, JError>> {
    Some(Err(JError::without_message("java.lang.UnsupportedOperationException")))
}

impl JObject for IntegerRange {
    fn class_name(&self) -> String {
        CLASS.into()
    }

    fn invoke(&self, name: &str, args: &[JValue]) -> Option<Result<Option<JValue>, JError>> {
        let items = self.items();
        let ok = |v: JValue| Some(Ok(Some(v)));
        match (name, args) {
            ("size", []) => ok(JValue::Int(self.size())),
            ("isEmpty", []) => ok(JValue::Bool(false)),
            ("get", [JValue::Int(i)]) => match usize::try_from(*i).ok().and_then(|i| items.get(i).cloned()) {
                Some(v) => ok(v),
                None => Some(Err(JError::without_message("java.lang.IndexOutOfBoundsException"))),
            },
            ("contains", [v]) => ok(JValue::Bool(items.iter().any(|i| java_equals(i, v)))),
            ("indexOf", [v]) => ok(JValue::Int(items.iter().position(|i| java_equals(i, v)).map_or(-1, |p| p as i32))),
            ("iterator", []) | ("listIterator", []) => ok(JValue::List(JList::new(ListKind::Iterator, items))),
            ("toArray", []) => ok(java_compat::lang::JArray::objects(items)),
            ("toString", []) => ok(JValue::str(self.to_java_string())),
            ("hashCode", []) => ok(JValue::Int(java_compat::lang::java_hash_code(&JValue::List(JList::array_list(items))))),
            ("equals", [other]) => ok(JValue::Bool(java_equals(&JValue::List(JList::array_list(items)), other))),
            // `AbstractList` refuses every change.
            ("set" | "add" | "remove" | "clear" | "addAll" | "removeAll" | "retainAll", _) => unsupported(),
            _ => None,
        }
    }

    /// `AbstractCollection.toString()`.
    fn to_java_string(&self) -> String {
        format!("[{}]", self.items().iter().map(|v| v.to_java_string()).collect::<Vec<_>>().join(", "))
    }

    fn iterate(&self) -> Option<Vec<JValue>> {
        Some(self.items())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
