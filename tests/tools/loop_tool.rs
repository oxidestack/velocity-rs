//! `org.apache.velocity.tools.generic.LoopToolTests`.

use std::sync::Arc;

use java_compat::lang::{JList, JMap, JObject, JValue, ListKind, MapKind};
use java_compat::reflect::{ClassDef, JType, LazyClass};
use velocity::tools::loop_tool::LoopTool;

use crate::base::{ints, s, strings};

const ARRAY: [&str; 3] = ["foo", "bar", "woogie"];

fn array() -> JValue {
    strings(&ARRAY)
}

/// `LIST = Arrays.asList(ARRAY)`.
fn list() -> JValue {
    JValue::List(JList::new(ListKind::Fixed, ARRAY.iter().map(|a| s(a)).collect()))
}

fn as_str(v: JValue) -> String {
    v.as_str().expect("a string").to_string()
}

/// `HasIteratorMethod`: a public `iterator()` returning `LIST.iterator()`; `MyIterable` also implements
/// `Iterable`.
struct HasIteratorMethod {
    iterable: bool,
}

fn has_iterator_def() -> Arc<ClassDef> {
    ClassDef::new("org.apache.velocity.tools.generic.LoopToolTests$HasIteratorMethod")
        .method::<HasIteratorMethod>("iterator", &[], Some(JType::class("java.util.Iterator")), |_, _, _| {
            Ok(Some(velocity::tools::iter::JIterator::value("java.util.AbstractList$Itr", list().iterate().unwrap())))
        })
        .register()
}

fn my_iterable_def() -> Arc<ClassDef> {
    ClassDef::new("org.apache.velocity.tools.generic.LoopToolTests$MyIterable")
        .extends(&["org.apache.velocity.tools.generic.LoopToolTests$HasIteratorMethod"])
        .implements(&["java.lang.Iterable"])
        .method::<HasIteratorMethod>("iterator", &[], Some(JType::class("java.util.Iterator")), |_, _, _| {
            Ok(Some(velocity::tools::iter::JIterator::value("java.util.AbstractList$Itr", list().iterate().unwrap())))
        })
        .register()
}

impl JObject for HasIteratorMethod {
    fn class_name(&self) -> String {
        self.class_def().unwrap().name.to_string()
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<Result<Option<JValue>, java_compat::lang::JError>> {
        None
    }

    fn to_java_string(&self) -> String {
        java_compat::reflect::identity_string(self)
    }

    fn iterate(&self) -> Option<Vec<JValue>> {
        self.iterable.then(|| list().iterate().unwrap())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<ClassDef>> {
        static PLAIN: LazyClass = LazyClass::new(has_iterator_def);
        static ITERABLE: LazyClass = LazyClass::new(my_iterable_def);
        Some(if self.iterable { ITERABLE.get() } else { PLAIN.get() })
    }
}

#[test]
fn ctor_loop_tool() {
    LoopTool::new();
}

#[test]
fn method_skip_int() {
    let lp = LoopTool::new();
    let i = lp.watch(&array()).unwrap();
    lp.skip(0);
    assert_eq!(as_str(i.next().unwrap()), ARRAY[0]);
    lp.skip(1);
    assert_eq!(as_str(i.next().unwrap()), ARRAY[2]);
    lp.pop();
    let i = lp.watch(&array()).unwrap();
    lp.skip(2);
    assert_eq!(as_str(i.next().unwrap()), ARRAY[2]);
}

#[test]
fn method_skip_int_string() {
    let lp = LoopTool::new();
    let i = lp.watch_named(&array(), Some("i")).unwrap();
    let j = lp.watch_named(&array(), Some("j")).unwrap();
    let k = lp.watch_named(&array(), Some("k")).unwrap();
    lp.skip_named(1, None);
    lp.skip_named(1, Some(""));
    lp.skip_named(1, Some("test"));
    lp.skip_named(1, Some("i"));
    lp.skip_named(1, Some("j"));
    lp.skip_named(1, Some("k"));
    assert_eq!(as_str(i.next().unwrap()), ARRAY[1]);
    assert_eq!(as_str(j.next().unwrap()), ARRAY[1]);
    assert_eq!(as_str(k.next().unwrap()), ARRAY[1]);
}

#[test]
fn method_stop() {
    let lp = LoopTool::new();
    let i = lp.watch(&array()).unwrap();
    assert!(i.has_next());
    lp.stop();
    assert!(!i.has_next());
}

#[test]
fn method_stop_all() {
    let lp = LoopTool::new();
    let i = lp.watch(&array()).unwrap();
    let j = lp.watch(&array()).unwrap();
    let k = lp.watch(&j.value()).unwrap();
    assert!(i.has_next() && j.has_next() && k.has_next());
    lp.stop_all();
    assert!(!(i.has_next() || j.has_next() || k.has_next()));
}

#[test]
fn method_stop_to_string() {
    let lp = LoopTool::new();
    let i = lp.watch_named(&array(), Some("i")).unwrap();
    let j = lp.watch_named(&array(), Some("j")).unwrap();
    let k = lp.watch_named(&array(), Some("k")).unwrap();
    assert!(i.has_next() && j.has_next() && k.has_next());
    lp.stop_to(None);
    assert!(i.has_next() && j.has_next() && k.has_next());
    lp.stop_to(Some(""));
    assert!(i.has_next() && j.has_next() && k.has_next());
    lp.stop_to(Some("test"));
    assert!(i.has_next() && j.has_next() && k.has_next());
    lp.stop_to(Some("j"));
    assert!(i.has_next());
    assert!(!(j.has_next() || k.has_next()));
}

#[test]
fn method_stop_string() {
    let lp = LoopTool::new();
    let i = lp.watch_named(&array(), Some("i")).unwrap();
    let j = lp.watch_named(&array(), Some("j")).unwrap();
    let k = lp.watch_named(&array(), Some("k")).unwrap();
    assert!(i.has_next() && j.has_next() && k.has_next());
    lp.stop_named(None);
    assert!(i.has_next() && j.has_next() && k.has_next());
    lp.stop_named(Some(""));
    assert!(i.has_next() && j.has_next() && k.has_next());
    lp.stop_named(Some("test"));
    assert!(i.has_next() && j.has_next() && k.has_next());
    lp.stop_named(Some("j"));
    assert!(i.has_next() && k.has_next());
    assert!(!j.has_next());
}

#[test]
fn method_watch_object() {
    let lp = LoopTool::new();
    assert!(lp.watch(&JValue::Null).is_none());
    assert!(lp.watch(&JValue::Object(Arc::new(velocity::tools::jdk_object()))).is_none());
    assert!(lp.watch(&array()).is_some());
    let inner = lp.watch(&array()).unwrap().value();
    assert!(lp.watch(&inner).is_some());
    assert!(lp.watch(&list()).is_some());
    assert!(lp.watch(&JValue::Map(JMap::new(MapKind::Hash))).is_some());
    assert!(lp.watch(&JValue::Object(Arc::new(HasIteratorMethod { iterable: true }))).is_some());
    assert!(lp.watch(&JValue::Object(Arc::new(HasIteratorMethod { iterable: false }))).is_some());
}

#[test]
fn method_watch_object_string() {
    let lp = LoopTool::new();
    assert!(lp.watch_named(&array(), None).is_none());
    assert!(lp.watch_named(&array(), Some("")).is_some());
    assert!(lp.watch_named(&array(), Some("name")).is_some());
}

#[test]
fn method_is_first() {
    let lp = LoopTool::new();
    let i = lp.watch(&array()).unwrap();
    assert_eq!(lp.is_first(), Some(true));
    i.next().unwrap();
    assert_eq!(lp.is_first(), Some(true));
    i.next().unwrap();
    assert_eq!(lp.is_first(), Some(false));
}

fn b(v: JValue) -> bool {
    matches!(v, JValue::Bool(true))
}

#[test]
fn method_is_first_string() {
    let lp = LoopTool::new();
    let i = lp.watch_named(&array(), Some("i")).unwrap();
    assert_eq!(lp.is_first(), Some(true));
    i.next().unwrap();
    let j = lp.watch_named(&array(), Some("j")).unwrap();
    assert_eq!(lp.is_first(), Some(true));
    assert_eq!(lp.is_first_named(Some("i")), Some(true));
    assert_eq!(lp.is_first_named(Some("j")), Some(true));
    assert!(b(lp.get(Some("first_i"))));
    assert!(b(lp.get(Some("first_j"))));
    i.next().unwrap();
    assert_eq!(lp.is_first_named(Some("i")), Some(false));
    assert!(!b(lp.get(Some("first_i"))));
    j.next().unwrap();
    assert_eq!(lp.is_first(), Some(true));
    assert_eq!(lp.is_first_named(Some("j")), Some(true));
    assert!(b(lp.get(Some("first_j"))));
    j.next().unwrap();
    assert_eq!(lp.is_first_named(Some("j")), Some(false));
    assert!(!b(lp.get(Some("first_j"))));
}

#[test]
fn method_is_last() {
    let lp = LoopTool::new();
    let i = lp.watch(&array()).unwrap();
    assert_eq!(lp.is_last(), Some(false));
    i.next().unwrap();
    assert_eq!(lp.is_last(), Some(false));
    i.next().unwrap();
    assert_eq!(lp.is_last(), Some(false));
    i.next().unwrap();
    assert_eq!(lp.is_last(), Some(true));
}

#[test]
fn method_is_last_string() {
    let lp = LoopTool::new();
    let i = lp.watch_named(&array(), Some("i")).unwrap();
    assert_eq!(lp.is_last(), Some(false));
    i.next().unwrap();
    i.next().unwrap();
    i.next().unwrap();
    assert_eq!(lp.is_last(), Some(true));
    let j = lp.watch_named(&array(), Some("j")).unwrap();
    assert_eq!(lp.is_last(), Some(false));
    assert_eq!(lp.is_last_named(Some("i")), Some(true));
    assert_eq!(lp.is_last_named(Some("j")), Some(false));
    assert!(b(lp.get(Some("last_i"))));
    assert!(!b(lp.get(Some("last_j"))));
    j.next().unwrap();
    j.next().unwrap();
    j.next().unwrap();
    assert_eq!(lp.is_last(), Some(true));
    assert_eq!(lp.is_last_named(Some("j")), Some(true));
    assert!(b(lp.get(Some("last_j"))));
}

#[test]
fn method_get() {
    let lp = LoopTool::new();
    let i = lp.watch(&array()).unwrap().sync_named(&array(), "twin").unwrap();
    while i.has_next() {
        let v = i.next().unwrap();
        assert_eq!(as_str(v), as_str(lp.get(Some("twin"))));
    }
    let little = ints(&[10, 20, 30]);
    let big = JValue::Array(Arc::new(java_compat::lang::JArray::new(JType::class("java.lang.Integer"), (1..=5).map(JValue::Int).collect())));
    let i = lp.watch(&big).unwrap().sync_named(&little, "little").unwrap();
    while i.has_next() {
        let JValue::Int(v) = i.next().unwrap() else { panic!("an Integer") };
        if v < 4 {
            assert!(matches!(lp.get(Some("little")), JValue::Int(x) if x == v * 10));
        } else {
            assert!(lp.get(Some("little")).is_null());
        }
    }
}

#[test]
fn method_get_string_string() {
    let lp = LoopTool::new();
    let other = ints(&[1, 2, 3]);
    let i = lp.watch(&array()).unwrap().sync(&other).unwrap();
    let j = lp.watch(&other).unwrap().sync(&array()).unwrap();
    while i.has_next() && j.has_next() {
        let ival = i.next().unwrap();
        let jval = j.next().unwrap();
        assert!(java_compat::lang::java_equals(&ival, &lp.get(Some("synced"))));
        assert!(java_compat::lang::java_equals(&jval, &lp.get_synced(Some("loop0"), "synced")));
    }
}

#[test]
fn method_get_count_or_get_index() {
    let lp = LoopTool::new();
    let i = lp.watch(&array()).unwrap();
    assert_eq!(lp.get_count(), Some(0));
    assert_eq!(lp.get_index(), None);
    i.next().unwrap();
    assert_eq!(lp.get_count(), Some(1));
    assert_eq!(lp.get_index(), Some(0));
    i.next().unwrap();
    assert_eq!(lp.get_count(), Some(2));
    assert_eq!(lp.get_index(), Some(1));
    i.next().unwrap();
    assert_eq!(lp.get_count(), Some(3));
    assert_eq!(lp.get_index(), Some(2));
    lp.pop();
    lp.watch(&array()).unwrap();
    lp.skip(2);
    assert_eq!(lp.get_count(), Some(2));
    assert_eq!(lp.get_index(), Some(1));
}

#[test]
fn method_get_count_or_get_index_string() {
    let lp = LoopTool::new();
    let i = lp.watch_named(&array(), Some("i")).unwrap();
    assert_eq!(lp.get_count_named(Some("i")), Some(0));
    assert_eq!(lp.get_index_named(Some("i")), None);
    i.next().unwrap();
    assert_eq!(lp.get_count_named(Some("i")), Some(1));
    assert_eq!(lp.get_index_named(Some("i")), Some(0));
    lp.watch_named(&array(), Some("j")).unwrap();
    lp.skip(2);
    assert_eq!(lp.get_count_named(Some("j")), Some(2));
    assert_eq!(lp.get_index_named(Some("j")), Some(1));
    assert_eq!(lp.get_count_named(Some("i")), Some(1));
    assert_eq!(lp.get_index_named(Some("i")), Some(0));
    assert!(matches!(lp.get(Some("count_j")), JValue::Int(2)));
    assert!(matches!(lp.get(Some("index_j")), JValue::Int(1)));
    assert!(matches!(lp.get(Some("count_i")), JValue::Int(1)));
    assert!(matches!(lp.get(Some("index_i")), JValue::Int(0)));
}

#[test]
fn alias_methods() {
    let lp = LoopTool::new();
    let i = lp.watch(&array()).unwrap();
    for _ in 0..3 {
        assert_eq!(lp.is_first(), lp.get_first());
        assert_eq!(lp.is_last(), lp.get_last());
        i.next().unwrap();
    }
    assert_eq!(lp.is_first(), lp.get_first());
    assert_eq!(lp.is_last(), lp.get_last());
}

#[test]
fn watch_and_exclude() {
    let lp = LoopTool::new();
    let i = lp.watch(&array()).unwrap().exclude(&s("bar")).unwrap();
    assert_eq!(as_str(i.next().unwrap()), "foo");
    assert_eq!(as_str(i.next().unwrap()), "woogie");
    assert_eq!(lp.is_last(), Some(true));
    assert!(!i.has_next());
}

#[test]
fn watch_and_stop() {
    let lp = LoopTool::new();
    let i = lp.watch(&array()).unwrap().stop_at(&s("bar")).unwrap();
    assert_eq!(as_str(i.next().unwrap()), "foo");
    assert_eq!(lp.is_last(), Some(true));
    assert!(!i.has_next());
}

#[test]
fn method_get_depth() {
    let lp = LoopTool::new();
    assert_eq!(lp.get_depth(), 0);
    lp.watch(&array());
    assert_eq!(lp.get_depth(), 1);
    lp.watch(&array());
    assert_eq!(lp.get_depth(), 2);
    lp.pop();
    assert_eq!(lp.get_depth(), 1);
    lp.pop();
    assert_eq!(lp.get_depth(), 0);
}
