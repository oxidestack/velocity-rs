//! `org.apache.velocity.runtime.directive.Scope` (and `ForeachScope`): the map a scope-providing directive or
//! template puts in the context under its scope name, chained to the scope it replaced, and `Scope.Info`. Also the
//! `Renderable` values `#define` and block macros put in the context.

use std::sync::{Arc, Weak};

use java_compat::lang::{JError, JMap, JObject, JValue, MapKind};
use parking_lot::Mutex;

use crate::ast::Block;
use crate::error::Owner;

/// Thrown by `Scope.stop()` through a method call (not reachable from templates: `stop()` is protected).
pub const STOP_COMMAND: &str = "org.apache.velocity.runtime.directive.StopCommand";

/// What `Scope.Info` reports about the scope's owner.
#[derive(Debug, Clone)]
pub struct OwnerInfo {
    /// `getName()`: the directive's name, or the template's.
    pub name: String,
    /// A directive (`#name[type:block …]`) rather than a template.
    pub directive: bool,
    /// `getType()`: `block` / `line` for a directive, the template's encoding otherwise.
    pub kind: String,
    pub template: String,
    pub line: i32,
    pub column: i32,
}

struct Foreach {
    index: i32,
    has_next: bool,
}

pub struct Scope {
    pub owner: Owner,
    pub info: OwnerInfo,
    parent: Option<JValue>,
    replaced: Option<JValue>,
    storage: JMap,
    foreach: Option<Mutex<Foreach>>,
    me: Weak<Scope>,
}

impl Scope {
    /// `new Scope(owner, previous)`: a previous `Scope` becomes the parent, anything else is what it replaced.
    pub fn new(owner: Owner, info: OwnerInfo, previous: Option<JValue>, foreach: bool) -> Arc<Scope> {
        let (parent, replaced) = match previous {
            Some(p @ JValue::Object(_)) if is_scope(&p) => (Some(p), None),
            Some(JValue::Null) | None => (None, None),
            Some(other) => (None, Some(other)),
        };
        Arc::new_cyclic(|me| Scope {
            owner,
            info,
            parent,
            replaced,
            storage: JMap::new(MapKind::Hash),
            foreach: foreach.then(|| Mutex::new(Foreach { index: -1, has_next: false })),
            me: me.clone(),
        })
    }

    fn value(&self) -> JValue {
        self.me.upgrade().map(|s| JValue::Object(s as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    fn parent_scope(&self) -> Option<&Scope> {
        match &self.parent {
            Some(JValue::Object(o)) => o.as_any().downcast_ref::<Scope>(),
            _ => None,
        }
    }

    /// `ForeachScope`'s counters, before each iteration.
    pub fn advance(&self, has_next: bool) {
        if let Some(f) = &self.foreach {
            let mut s = f.lock();
            s.index += 1;
            s.has_next = has_next;
        }
    }

    /// `Scope.get(key)`: its own storage, else the parent's.
    pub fn get(&self, key: &JValue) -> JValue {
        match self.storage.get(key) {
            Some(v) => v,
            None if !self.storage.contains_key(key) => self.parent_scope().map(|p| p.get(key)).unwrap_or(JValue::Null),
            None => JValue::Null,
        }
    }

    pub fn get_str(&self, key: &str) -> JValue {
        self.get(&JValue::str(key))
    }

    pub fn parent(&self) -> Option<JValue> {
        self.parent.clone()
    }

    fn topmost(&self) -> JValue {
        match self.parent_scope() {
            Some(p) => p.topmost(),
            None => self.value(),
        }
    }

    /// `getReplaced()`: what this scope (or its topmost ancestor) replaced in the context.
    pub fn replaced(&self) -> Option<JValue> {
        if self.replaced.is_some() {
            return self.replaced.clone();
        }
        self.parent_scope().and_then(Scope::replaced)
    }

    fn depth(&self) -> i32 {
        match self.parent_scope() {
            Some(p) => p.depth() + 1,
            None => 1,
        }
    }

    /// `Scope.Info.toString()`.
    pub fn info_string(&self) -> String {
        let i = &self.info;
        let mut s = String::new();
        if i.directive {
            s.push('#');
        }
        s.push_str(&i.name);
        s.push_str(&format!("[type:{}", i.kind));
        let depth = self.depth();
        if depth > 1 {
            s.push_str(&format!(" depth:{depth}"));
        }
        if i.directive {
            s.push_str(" template:");
            if i.template.contains(' ') {
                s.push_str(&format!("\"{}\"", i.template));
            } else {
                s.push_str(&i.template);
            }
            s.push_str(&format!(" line:{} column:{}", i.line, i.column));
        }
        s.push(']');
        s
    }
}

pub fn is_scope(v: &JValue) -> bool {
    matches!(v, JValue::Object(o) if o.as_any().is::<Scope>())
}

pub fn as_scope(v: &JValue) -> Option<&Scope> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<Scope>(),
        _ => None,
    }
}

/// `Scope.Info`.
struct ScopeInfo(Arc<dyn JObject>);

impl JObject for ScopeInfo {
    fn class_name(&self) -> String {
        "org.apache.velocity.runtime.directive.Scope$Info".into()
    }

    fn invoke(&self, name: &str, args: &[JValue]) -> Option<Result<Option<JValue>, JError>> {
        let scope = self.0.as_any().downcast_ref::<Scope>()?;
        let i = &scope.info;
        let ok = |v: JValue| Some(Ok(Some(v)));
        match (name, args) {
            ("getName", []) => ok(JValue::str(i.name.as_str())),
            ("getType", []) => ok(JValue::str(i.kind.as_str())),
            ("getDepth", []) => ok(JValue::Int(scope.depth())),
            ("getTemplate", []) => ok(JValue::str(i.template.as_str())),
            ("getLine", []) => ok(JValue::Int(if i.directive { i.line } else { 0 })),
            ("getColumn", []) => ok(JValue::Int(if i.directive { i.column } else { 0 })),
            ("toString", []) => ok(JValue::str(scope.info_string())),
            _ => None,
        }
    }

    fn to_java_string(&self) -> String {
        self.0.as_any().downcast_ref::<Scope>().map(Scope::info_string).unwrap_or_default()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl JObject for Scope {
    fn class_name(&self) -> String {
        if self.foreach.is_some() { "org.apache.velocity.runtime.directive.ForeachScope".into() } else { "org.apache.velocity.runtime.directive.Scope".into() }
    }

    fn invoke(&self, name: &str, args: &[JValue]) -> Option<Result<Option<JValue>, JError>> {
        let ok = |v: JValue| Some(Ok(Some(v)));
        if let Some(f) = &self.foreach {
            let s = f.lock();
            match (name, args) {
                ("getIndex", []) => return ok(JValue::Int(s.index)),
                ("getCount", []) => return ok(JValue::Int(s.index + 1)),
                ("hasNext" | "getHasNext", []) => return ok(JValue::Bool(s.has_next)),
                ("isFirst" | "getFirst", []) => return ok(JValue::Bool(s.index < 1)),
                ("isLast" | "getLast", []) => return ok(JValue::Bool(!s.has_next)),
                _ => {}
            }
        }
        match (name, args) {
            ("getParent", []) => ok(self.parent.clone().unwrap_or(JValue::Null)),
            ("getReplaced", []) => ok(self.replaced().unwrap_or(JValue::Null)),
            ("getTopmost", []) => ok(self.topmost()),
            ("getInfo", []) => ok(JValue::Object(Arc::new(ScopeInfo(self.me.upgrade()? as Arc<dyn JObject>)))),
            ("get", [k]) => ok(self.get(k)),
            ("put", [k, v]) => ok(self.storage.put(k.clone(), v.clone())),
            ("set", [k, v]) => {
                self.storage.put(k.clone(), v.clone());
                ok(JValue::str(""))
            }
            ("size", []) => ok(JValue::Int(self.storage.len() as i32)),
            ("isEmpty", []) => ok(JValue::Bool(self.storage.is_empty())),
            ("containsKey", [k]) => ok(JValue::Bool(self.storage.contains_key(k))),
            ("remove", [k]) => ok(self.storage.remove(k)),
            ("keySet", []) => ok(JValue::List(java_compat::lang::JList::new(java_compat::lang::ListKind::Set, self.storage.entries().into_iter().map(|(k, _)| k).collect()))),
            ("values", []) => ok(JValue::List(java_compat::lang::JList::new(java_compat::lang::ListKind::Collection, self.storage.entries().into_iter().map(|(_, v)| v).collect()))),
            ("toString", []) => ok(JValue::str(self.to_java_string())),
            ("hashCode", []) => ok(JValue::Int(java_compat::lang::java_hash_code(&JValue::Map(self.storage.clone())))),
            ("equals", [o]) => ok(JValue::Bool(match o {
                JValue::Object(x) => x.as_any().downcast_ref::<Scope>().is_some_and(|s| java_compat::lang::java_equals(&JValue::Map(s.storage.clone()), &JValue::Map(self.storage.clone()))),
                JValue::Map(_) => java_compat::lang::java_equals(o, &JValue::Map(self.storage.clone())),
                _ => false,
            })),
            _ => None,
        }
    }

    fn to_java_string(&self) -> String {
        JValue::Map(self.storage.clone()).to_java_string()
    }

    fn iterate(&self) -> Option<Vec<JValue>> {
        Some(self.storage.entries().into_iter().map(|(_, v)| v).collect())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// `Block.Reference`: a `#define`d block or a block macro's `$bodyContent`, rendered where it is referenced, with
/// the scope of the directive that made it.
pub struct BlockRef {
    pub block: Arc<Block>,
    /// `maxDepth`: `directive.define.max_depth` or `velocimacro.max_depth`.
    pub max_depth: usize,
    pub depth: Mutex<usize>,
    /// The `Block` directive: its scope (`define`, or the block macro's name), owner and location.
    pub scope_name: String,
    pub owner: Owner,
    pub owner_info: OwnerInfo,
}

impl JObject for BlockRef {
    fn class_name(&self) -> String {
        "org.apache.velocity.runtime.directive.Block$Reference".into()
    }

    fn invoke(&self, name: &str, args: &[JValue]) -> Option<Result<Option<JValue>, JError>> {
        match (name, args) {
            ("getAsBoolean", []) => Some(Ok(Some(JValue::Bool(*self.depth.lock() <= self.max_depth)))),
            _ => None,
        }
    }

    fn to_java_string(&self) -> String {
        String::new()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
