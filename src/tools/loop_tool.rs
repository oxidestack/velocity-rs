//! `org.apache.velocity.tools.generic.LoopTool` (`$loop`): `#foreach` iterators it watches, which can be
//! stopped, skipped, filtered and synced with other iterables, and asked about their position.

use std::sync::{Arc, Weak};

use java_compat::lang::{JError, JMap, JObject, JValue, MapKind, java_equals};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;

use super::classes::{Annotation, JavaClass};
use super::safe::{self, SafeConfig, Tool};
use super::{c, identity, o, s, tool_object, val};

/// The class name of `org.apache.velocity.tools.generic.LoopTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.LoopTool";
/// The class name of `LoopTool.ManagedIterator`.
pub const MANAGED_ITERATOR: &str = "org.apache.velocity.tools.generic.LoopTool$ManagedIterator";

/// An `Iterator` over what `ClassUtils.getIterator` accepts.
enum Source {
    Items(std::collections::VecDeque<JValue>),
    Managed(Arc<ManagedIterator>),
    /// Another `Iterator` object, called through `hasNext()` / `next()`.
    Object(JValue),
}

impl Source {
    fn has_next(&mut self) -> bool {
        match self {
            Source::Items(v) => !v.is_empty(),
            Source::Managed(m) => m.has_next(),
            Source::Object(o) => matches!(crate::introspect::call(o, "hasNext", &[]), Some(Ok(Some(JValue::Bool(true))))),
        }
    }

    fn next(&mut self) -> Result<JValue, JError> {
        match self {
            Source::Items(v) => v.pop_front().ok_or_else(|| JError::without_message("java.util.NoSuchElementException")),
            Source::Managed(m) => m.next(),
            Source::Object(o) => crate::introspect::call(o, "next", &[]).unwrap_or(Ok(None)).map(|v| v.unwrap_or(JValue::Null)),
        }
    }
}

/// `ClassUtils.getIterator(obj)`: arrays, collections, map values, iterators, iterables, or an object's public
/// `iterator()` method; `None` for anything else.
fn get_iterator(obj: &JValue) -> Option<Source> {
    match obj {
        JValue::Null => None,
        JValue::Array(a) => Some(Source::Items(a.items().into())),
        JValue::List(l) => Some(Source::Items(l.items().into())),
        JValue::Map(m) => Some(Source::Items(m.entries().into_iter().map(|(_, v)| v).collect())),
        JValue::Object(o) => {
            if let Some(m) = as_managed(obj) {
                return Some(Source::Managed(m));
            }
            let def = java_compat::reflect::class_def(obj);
            let is_iterator = def.as_ref().is_some_and(|d| d.interfaces.iter().any(|i| &**i == "java.util.Iterator"));
            if is_iterator {
                return Some(Source::Object(obj.clone()));
            }
            if let Some(items) = o.iterate() {
                return Some(Source::Items(items.into()));
            }
            match crate::introspect::call(obj, "iterator", &[]) {
                Some(Ok(Some(it))) => get_iterator(&it),
                _ => {
                    super::log::error(CLASS, &format!("Exception while getting Iterator: java.lang.NoSuchMethodException: {}.iterator()", obj.class_name()));
                    None
                }
            }
        }
        _ => {
            super::log::error(CLASS, &format!("Exception while getting Iterator: java.lang.NoSuchMethodException: {}.iterator()", obj.class_name()));
            None
        }
    }
}

/// `LoopTool.Equals`: equal, or of different classes with equal strings.
fn equals_condition(compare: &JValue, value: &JValue) -> bool {
    if value.is_null() {
        return false;
    }
    if java_equals(compare, value) {
        return true;
    }
    if value.class_name() == compare.class_name() {
        return false;
    }
    value.to_java_string() == compare.to_java_string()
}

/// `LoopTool.Action`: what a matching condition does to the loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// `Action.EXCLUDE`: skip the matching element.
    Exclude,
    /// `Action.STOP`: end the loop at the matching element.
    Stop,
}

struct Synced {
    source: Source,
    current: JValue,
}

struct ManagedState {
    source: Source,
    stopped: bool,
    first: Option<bool>,
    count: i32,
    next: Option<JValue>,
    conditions: Vec<(Action, JValue)>,
    synced: Vec<(String, Synced)>,
}

/// `LoopTool.ManagedIterator`.
pub struct ManagedIterator {
    name: String,
    owner: Weak<LoopTool>,
    st: Mutex<ManagedState>,
    me: Weak<ManagedIterator>,
}

fn as_managed(v: &JValue) -> Option<Arc<ManagedIterator>> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<ManagedIterator>().and_then(|m| m.me.upgrade()),
        _ => None,
    }
}

impl ManagedIterator {
    /// `getName()`: the name given to `watch`, else `loop` and the depth at which it was watched.
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// This iterator as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|m| JValue::Object(m as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `isFirst()`: whether the last `next()` returned the first element (also true before any).
    pub fn is_first(&self) -> bool {
        self.st.lock().first.is_none_or(|f| f)
    }

    /// `isLast()`: whether no element is left, without popping the loop.
    pub fn is_last(&self) -> bool {
        !self.has_next_popping(false)
    }

    /// `hasNext()`: when exhausted, the loop tool's current loop is popped and this one stopped.
    pub fn has_next(&self) -> bool {
        self.has_next_popping(true)
    }

    fn has_next_popping(&self, pop_when_done: bool) -> bool {
        {
            let st = self.st.lock();
            if st.stopped {
                return false;
            }
            if st.next.is_some() {
                return true;
            }
        }
        self.cache_next(pop_when_done)
    }

    /// `cacheNext(popWhenDone)`: the next element that is not excluded, or stop.
    fn cache_next(&self, pop_when_done: bool) -> bool {
        let mut st = self.st.lock();
        loop {
            if !st.source.has_next() {
                drop(st);
                if pop_when_done {
                    if let Some(owner) = self.owner.upgrade() {
                        owner.pop();
                    }
                    self.stop();
                }
                return false;
            }
            let next = match st.source.next() {
                Ok(v) => v,
                Err(_) => return false,
            };
            match st.conditions.iter().find(|(_, c)| equals_condition(c, &next)).map(|(a, _)| *a) {
                Some(Action::Exclude) => continue,
                Some(Action::Stop) => {
                    st.stopped = true;
                    st.next = None;
                    return false;
                }
                None => {
                    st.next = Some(next);
                    return true;
                }
            }
        }
    }

    /// `isSyncedWith(name)`: whether an iterable is synced under `name`.
    pub fn is_synced_with(&self, name: &str) -> bool {
        self.st.lock().synced.iter().any(|(n, _)| n == name)
    }

    /// `get(name)`: the synced iterable's current value.
    pub fn get(&self, name: &str) -> JValue {
        self.st.lock().synced.iter().find(|(n, _)| n == name).map(|(_, s)| s.current.clone()).unwrap_or(JValue::Null)
    }

    /// `getCount()`: how many elements `next()` has returned.
    pub fn get_count(&self) -> i32 {
        self.st.lock().count
    }

    /// `getIndex()`: the current element's zero-based index, `-1` before the first.
    pub fn get_index(&self) -> i32 {
        self.get_count() - 1
    }

    /// `next()`: `NoSuchElementException` past the end; shifts the synced iterables.
    pub fn next(&self) -> Result<JValue, JError> {
        let need = self.st.lock().next.is_none();
        if need && !self.cache_next(true) {
            return Err(JError::new("java.util.NoSuchElementException", "There are no more valid elements in this iterator"));
        }
        let mut st = self.st.lock();
        st.first = Some(st.first.is_none());
        st.count += 1;
        let value = st.next.take().unwrap_or(JValue::Null);
        for (_, s) in st.synced.iter_mut() {
            s.current = if s.source.has_next() { s.source.next().unwrap_or(JValue::Null) } else { JValue::Null };
        }
        Ok(value)
    }

    /// `stop()`: no further elements.
    pub fn stop(&self) {
        let mut st = self.st.lock();
        st.stopped = true;
        st.next = None;
    }

    /// `exclude(compare)`: skip elements equal to it (`IllegalArgumentException` for `null`).
    pub fn exclude(&self, compare: &JValue) -> Result<Arc<ManagedIterator>, JError> {
        self.condition(Action::Exclude, compare)
    }

    /// `stop(compare)`: stop at an element equal to it.
    pub fn stop_at(&self, compare: &JValue) -> Result<Arc<ManagedIterator>, JError> {
        self.condition(Action::Stop, compare)
    }

    fn condition(&self, action: Action, compare: &JValue) -> Result<Arc<ManagedIterator>, JError> {
        if compare.is_null() {
            return Err(JError::new("java.lang.IllegalArgumentException", "Condition must have something to compare to"));
        }
        self.st.lock().conditions.push((action, compare.clone()));
        Ok(self.me.upgrade().expect("alive"))
    }

    /// `sync(iterable)`: under the name `synced`.
    pub fn sync(&self, iterable: &JValue) -> Option<Arc<ManagedIterator>> {
        self.sync_named(iterable, "synced")
    }

    /// `sync(iterable, name)`: `null` when it cannot be iterated.
    pub fn sync_named(&self, iterable: &JValue, name: &str) -> Option<Arc<ManagedIterator>> {
        let source = get_iterator(iterable)?;
        let mut st = self.st.lock();
        st.synced.retain(|(n, _)| n != name);
        st.synced.push((name.to_string(), Synced { source, current: JValue::Null }));
        drop(st);
        self.me.upgrade()
    }

    /// `getLastSyncedValues()`.
    pub fn get_last_synced_values(&self) -> Option<Vec<(String, JValue)>> {
        let st = self.st.lock();
        if st.synced.is_empty() {
            return None;
        }
        Some(st.synced.iter().map(|(n, s)| (n.clone(), s.current.clone())).collect())
    }

    fn java_string(&self) -> String {
        format!("ManagedIterator:{}", self.name)
    }
}

fn opt_managed(m: Option<Arc<ManagedIterator>>) -> super::Ret {
    val(m.map(|m| m.value()).unwrap_or(JValue::Null))
}

fn managed_def() -> Arc<ClassDef> {
    type M = ManagedIterator;
    let me = || c(MANAGED_ITERATOR);
    let b = || JType::Boolean;
    let def = ClassDef::new(MANAGED_ITERATOR)
        .implements(&["java.util.Iterator"])
        .method::<M>("getName", &[], Some(s()), |m, _, _| val(JValue::str(m.get_name())))
        .method::<M>("isFirst", &[], Some(b()), |m, _, _| val(JValue::Bool(m.is_first())))
        .method::<M>("isLast", &[], Some(b()), |m, _, _| val(JValue::Bool(m.is_last())))
        .method::<M>("getFirst", &[], Some(b()), |m, _, _| val(JValue::Bool(m.is_first())))
        .method::<M>("getLast", &[], Some(b()), |m, _, _| val(JValue::Bool(m.is_last())))
        .method::<M>("hasNext", &[], Some(b()), |m, _, _| val(JValue::Bool(m.has_next())))
        .method::<M>("getHasNext", &[], Some(b()), |m, _, _| val(JValue::Bool(m.has_next_popping(false))))
        .method::<M>("isSyncedWith", &[s()], Some(b()), |m, _, a| val(JValue::Bool(m.is_synced_with(&a[0].to_java_string()))))
        .method::<M>("get", &[s()], Some(o()), |m, _, a| val(m.get(&a[0].to_java_string())))
        .method::<M>("getCount", &[], Some(JType::Int), |m, _, _| val(JValue::Int(m.get_count())))
        .method::<M>("getIndex", &[], Some(JType::Int), |m, _, _| val(JValue::Int(m.get_index())))
        .method::<M>("next", &[], Some(o()), |m, _, _| val(m.next()?))
        .method::<M>("remove", &[], None, |_, _, _| Err(JError::new("java.lang.UnsupportedOperationException", "remove is not currently supported")))
        .method::<M>("stop", &[], None, |m, _, _| {
            m.stop();
            Ok(None)
        })
        .method::<M>("exclude", &[o()], Some(me()), |m, _, a| val(m.exclude(&a[0])?.value()))
        .method::<M>("stop", &[o()], Some(me()), |m, _, a| val(m.stop_at(&a[0])?.value()))
        .method::<M>("sync", &[o()], Some(me()), |m, _, a| opt_managed(m.sync(&a[0])))
        .method::<M>("sync", &[o(), s()], Some(me()), |m, _, a| opt_managed(m.sync_named(&a[0], &a[1].to_java_string())))
        .method::<M>("getLastSyncedValues", &[], Some(c("java.util.Map")), |m, _, _| {
            val(m.get_last_synced_values().map(|v| JValue::Map(JMap::from_entries(MapKind::Hash, v.into_iter().map(|(k, v)| (JValue::str(k), v))))).unwrap_or(JValue::Null))
        })
        .method::<M>("toString", &[], Some(s()), |m, _, _| val(JValue::str(m.java_string())));
    super::object_methods::<M>(def).register()
}

tool_object!(ManagedIterator, managed_def);

struct LoopState {
    iterators: Vec<Arc<ManagedIterator>>,
    last: Option<Arc<ManagedIterator>>,
    last_synced: Option<Vec<(String, JValue)>>,
}

/// Port of `org.apache.velocity.tools.generic.LoopTool` (`$loop`): a stack of watched `#foreach` loops, innermost last.
pub struct LoopTool {
    safe: SafeConfig,
    st: Mutex<LoopState>,
    me: Weak<LoopTool>,
}

impl LoopTool {
    /// `new LoopTool()`: no loops watched.
    pub fn new() -> Arc<LoopTool> {
        Arc::new_cyclic(|me| LoopTool { safe: SafeConfig::default(), st: Mutex::new(LoopState { iterators: Vec::new(), last: None, last_synced: None }), me: me.clone() })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: nothing once the configuration is locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    fn manage(&self, source: Source, name: Option<&str>) -> Arc<ManagedIterator> {
        let depth = self.get_depth();
        let managed = Arc::new_cyclic(|me| ManagedIterator {
            name: name.map(str::to_string).unwrap_or_else(|| format!("loop{depth}")),
            owner: self.me.clone(),
            st: Mutex::new(ManagedState { source, stopped: false, first: None, count: 0, next: None, conditions: Vec::new(), synced: Vec::new() }),
            me: me.clone(),
        });
        let mut st = self.st.lock();
        st.iterators.push(managed.clone());
        st.last = Some(managed.clone());
        managed
    }

    /// `watch(obj)`: `null` when it cannot be iterated.
    pub fn watch(&self, obj: &JValue) -> Option<Arc<ManagedIterator>> {
        let source = get_iterator(obj)?;
        Some(self.manage(source, None))
    }

    /// `watch(obj, name)`: `null` for a `null` name.
    pub fn watch_named(&self, obj: &JValue, name: Option<&str>) -> Option<Arc<ManagedIterator>> {
        let name = name?;
        let source = get_iterator(obj)?;
        Some(self.manage(source, Some(name)))
    }

    /// `sync(main, synced)`: `NullPointerException` when `main` cannot be watched.
    pub fn sync(&self, main: &JValue, synced: &JValue) -> Result<Option<Arc<ManagedIterator>>, JError> {
        Ok(self.watch(main).ok_or_else(super::npe)?.sync(synced))
    }

    fn iterators(&self) -> Vec<Arc<ManagedIterator>> {
        self.st.lock().iterators.clone()
    }

    /// `stop()`: the innermost loop.
    pub fn stop(&self) {
        if let Some(i) = self.iterators().last() {
            i.stop();
        }
    }

    /// `stop(name)`: the outermost loop with that name.
    pub fn stop_named(&self, name: Option<&str>) {
        if let Some(i) = self.iterators().into_iter().find(|i| Some(i.get_name()) == name) {
            i.stop();
        }
    }

    /// `stopTo(name)`: the innermost loop with that name and every loop inside it.
    pub fn stop_to(&self, name: Option<&str>) {
        let its = self.iterators();
        if let Some(pos) = its.iter().rposition(|i| Some(i.get_name()) == name) {
            for i in &its[pos..] {
                i.stop();
            }
        }
    }

    /// `stopAll()`: every watched loop.
    pub fn stop_all(&self) {
        for i in self.iterators() {
            i.stop();
        }
    }

    fn skip_in(number: i32, i: &ManagedIterator) {
        for _ in 0..number.max(0) {
            if i.has_next() {
                let _ = i.next();
            } else {
                break;
            }
        }
    }

    /// `skip(number)`: in the innermost loop.
    pub fn skip(&self, number: i32) {
        if let Some(i) = self.iterators().last() {
            LoopTool::skip_in(number, i);
        }
    }

    /// `skip(number, name)`: in the outermost loop with that name; nothing when none has it.
    pub fn skip_named(&self, number: i32, name: Option<&str>) {
        if let Some(i) = self.find_iterator(name) {
            LoopTool::skip_in(number, &i);
        }
    }

    fn last(&self) -> Option<Arc<ManagedIterator>> {
        self.st.lock().last.clone()
    }

    /// `isFirst()`: of the most recently watched loop, `None` when none was.
    pub fn is_first(&self) -> Option<bool> {
        self.last().map(|l| l.is_first())
    }

    /// `isFirst(name)`: of the outermost loop with that name, `None` when none has it.
    pub fn is_first_named(&self, name: Option<&str>) -> Option<bool> {
        self.find_iterator(name).map(|i| i.is_first())
    }

    /// `getFirst()`: as `is_first`.
    pub fn get_first(&self) -> Option<bool> {
        self.is_first()
    }

    /// `isLast()`: of the most recently watched loop, `None` when none was.
    pub fn is_last(&self) -> Option<bool> {
        self.last().map(|l| l.is_last())
    }

    /// `isLast(name)`: of the outermost loop with that name, `None` when none has it.
    pub fn is_last_named(&self, name: Option<&str>) -> Option<bool> {
        self.find_iterator(name).map(|i| i.is_last())
    }

    /// `getLast()`: as `is_last`.
    pub fn get_last(&self) -> Option<bool> {
        self.is_last()
    }

    /// `get(key)`: a synced value (innermost first, then the last popped loop's), else `first_…`, `last_…`,
    /// `count_…` or `index_…` of a named loop.
    pub fn get(&self, key: Option<&str>) -> JValue {
        let k = key.unwrap_or("null");
        for i in self.iterators().iter().rev() {
            if i.is_synced_with(k) {
                return i.get(k);
            }
        }
        if let Some(v) = self.st.lock().last_synced.as_ref().and_then(|l| l.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())).filter(|v| !v.is_null()) {
            return v;
        }
        let Some(key) = key.filter(|k| k.encode_utf16().count() >= 6) else { return JValue::Null };
        let b = |v: Option<bool>| v.map(JValue::Bool).unwrap_or(JValue::Null);
        let i = |v: Option<i32>| v.map(JValue::Int).unwrap_or(JValue::Null);
        if let Some(n) = key.strip_prefix("last_") {
            return b(self.is_last_named(Some(n)));
        }
        if let Some(n) = key.strip_prefix("count_") {
            return i(self.get_count_named(Some(n)));
        }
        if let Some(n) = key.strip_prefix("index_") {
            return i(self.get_index_named(Some(n)));
        }
        if let Some(n) = key.strip_prefix("first_") {
            return b(self.is_first_named(Some(n)));
        }
        JValue::Null
    }

    /// `get(name, synced)`.
    pub fn get_synced(&self, name: Option<&str>, synced: &str) -> JValue {
        self.find_iterator(name).map(|i| i.get(synced)).unwrap_or(JValue::Null)
    }

    /// `getIndex()`: of the most recently watched loop; `None` when none was or before its first element.
    pub fn get_index(&self) -> Option<i32> {
        self.get_count().filter(|c| *c != 0).map(|c| c - 1)
    }

    /// `getIndex(name)`: of the outermost loop with that name; `None` when none has it or before its first element.
    pub fn get_index_named(&self, name: Option<&str>) -> Option<i32> {
        self.get_count_named(name).filter(|c| *c != 0).map(|c| c - 1)
    }

    /// `getCount()`: of the most recently watched loop, `None` when none was.
    pub fn get_count(&self) -> Option<i32> {
        self.last().map(|l| l.get_count())
    }

    /// `getCount(name)`: of the outermost loop with that name, `None` when none has it.
    pub fn get_count_named(&self, name: Option<&str>) -> Option<i32> {
        self.find_iterator(name).map(|i| i.get_count())
    }

    /// `getThis()`: the most recently watched loop's iterator.
    pub fn get_this(&self) -> Option<Arc<ManagedIterator>> {
        self.last()
    }

    /// `getDepth()`: how many watched loops are still running.
    pub fn get_depth(&self) -> usize {
        self.st.lock().iterators.len()
    }

    fn find_iterator(&self, name: Option<&str>) -> Option<Arc<ManagedIterator>> {
        self.iterators().into_iter().find(|i| Some(i.get_name()) == name)
    }

    /// `pop()`: the innermost loop ends; its synced values stay readable.
    pub fn pop(&self) -> Option<Arc<ManagedIterator>> {
        let i = self.st.lock().iterators.pop()?;
        let synced = i.get_last_synced_values();
        self.st.lock().last_synced = synced;
        Some(i)
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for LoopTool {
    fn safe(&self) -> &SafeConfig {
        &self.safe
    }
}

fn opt_bool(v: Option<bool>) -> super::Ret {
    val(v.map(JValue::Bool).unwrap_or(JValue::Null))
}

fn opt_int(v: Option<i32>) -> super::Ret {
    val(v.map(JValue::Int).unwrap_or(JValue::Null))
}

fn def() -> Arc<ClassDef> {
    type L = LoopTool;
    let m = || c(MANAGED_ITERATOR);
    let bb = || c("java.lang.Boolean");
    let ii = || c("java.lang.Integer");
    let def = ClassDef::new(CLASS)
        .extends(&[safe::SAFE_CONFIG])
        .implements(&["java.io.Serializable"])
        .method::<L>("watch", &[o()], Some(m()), |t, _, a| opt_managed(t.watch(&a[0])))
        .method::<L>("watch", &[o(), s()], Some(m()), |t, _, a| opt_managed(t.watch_named(&a[0], a[1].as_str())))
        .method::<L>("sync", &[o(), o()], Some(m()), |t, _, a| opt_managed(t.sync(&a[0], &a[1])?))
        .method::<L>("stop", &[], None, |t, _, _| {
            t.stop();
            Ok(None)
        })
        .method::<L>("stop", &[s()], None, |t, _, a| {
            t.stop_named(a[0].as_str());
            Ok(None)
        })
        .method::<L>("stopTo", &[s()], None, |t, _, a| {
            t.stop_to(a[0].as_str());
            Ok(None)
        })
        .method::<L>("stopAll", &[], None, |t, _, _| {
            t.stop_all();
            Ok(None)
        })
        .method::<L>("skip", &[JType::Int], None, |t, _, a| {
            t.skip(super::num::int_value(&a[0]));
            Ok(None)
        })
        .method::<L>("skip", &[JType::Int, s()], None, |t, _, a| {
            t.skip_named(super::num::int_value(&a[0]), a[1].as_str());
            Ok(None)
        })
        .method::<L>("isFirst", &[], Some(bb()), |t, _, _| opt_bool(t.is_first()))
        .method::<L>("isFirst", &[s()], Some(bb()), |t, _, a| opt_bool(t.is_first_named(a[0].as_str())))
        .method::<L>("getFirst", &[], Some(bb()), |t, _, _| opt_bool(t.get_first()))
        .method::<L>("isLast", &[], Some(bb()), |t, _, _| opt_bool(t.is_last()))
        .method::<L>("isLast", &[s()], Some(bb()), |t, _, a| opt_bool(t.is_last_named(a[0].as_str())))
        .method::<L>("getLast", &[], Some(bb()), |t, _, _| opt_bool(t.get_last()))
        .method::<L>("get", &[s()], Some(o()), |t, _, a| val(t.get(a[0].as_str())))
        .method::<L>("get", &[s(), s()], Some(o()), |t, _, a| val(t.get_synced(a[0].as_str(), &a[1].to_java_string())))
        .method::<L>("getIndex", &[], Some(ii()), |t, _, _| opt_int(t.get_index()))
        .method::<L>("getIndex", &[s()], Some(ii()), |t, _, a| opt_int(t.get_index_named(a[0].as_str())))
        .method::<L>("getCount", &[], Some(ii()), |t, _, _| opt_int(t.get_count()))
        .method::<L>("getCount", &[s()], Some(ii()), |t, _, a| opt_int(t.get_count_named(a[0].as_str())))
        .method::<L>("getThis", &[], Some(m()), |t, _, _| opt_managed(t.get_this()))
        .method::<L>("getDepth", &[], Some(JType::Int), |t, _, _| val(JValue::Int(t.get_depth() as i32)));
    super::object_methods::<L>(safe::safe_config_methods::<L>(def)).register()
}

tool_object!(LoopTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || LoopTool::new().value()).annotated(Annotation::default_key("loop")).annotated(Annotation::valid_scope(&[super::scope::REQUEST]))
}
