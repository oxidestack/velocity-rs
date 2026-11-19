//! `org.apache.velocity.test.provider`: the fixture classes the template tests put in their contexts.

#![allow(dead_code)]

use std::sync::{Arc, LazyLock};

use java_compat::lang::{JArray, JError, JList, JMap, JValue, ListKind, MapKind};
use java_compat::reflect::JType;

use crate::base::*;

const PACKAGE: &str = "org.apache.velocity.test.provider.";

fn c(name: &str) -> JType {
    JType::class(name)
}

fn provider_type() -> JType {
    c("org.apache.velocity.test.provider.TestProvider")
}

fn hashtable(entries: &[(&str, JValue)]) -> JMap {
    let m = JMap::new(MapKind::Linked);
    for (k, v) in entries {
        m.put(s(k), v.clone());
    }
    m
}

/// `getSearch()`: a `Hashtable` whose `RelatedLinks` is a list holding the table itself.
fn search() -> JValue {
    let h = hashtable(&[
        ("Text", s("this is some text")),
        ("EscText", s("this is escaped text")),
        ("Title", s("this is the title")),
        ("Index", s("this is the index")),
        ("URL", s("http://periapt.com")),
    ]);
    let al = JList::array_list(vec![JValue::Map(h.clone())]);
    h.put(s("RelatedLinks"), JValue::List(al));
    JValue::Map(h)
}

fn strings(items: &[&str], kind: ListKind) -> JValue {
    JValue::List(JList::new(kind, items.iter().map(|v| s(v)).collect()))
}

/// `concat` / `objConcat`: each element, then a space.
fn joined(items: &[JValue]) -> String {
    items.iter().map(|v| format!("{} ", v.to_java_string())).collect()
}

fn list_items(v: &JValue) -> Vec<JValue> {
    v.iterate().unwrap_or_default()
}

pub static PERSON: LazyLock<BeanFactory> = LazyLock::new(|| class(&format!("{PACKAGE}Person")).method("getName", &[], Some(string()), |_, _| ret(s("Person"))).build());

pub static CHILD: LazyLock<BeanFactory> =
    LazyLock::new(|| class(&format!("{PACKAGE}Child")).extends(&[&format!("{PACKAGE}Person")]).method("getName", &[], Some(string()), |_, _| ret(s("Child"))).build());

pub static TEST_PROVIDER: LazyLock<BeanFactory> = LazyLock::new(|| {
    let object_array = || JType::array(JType::object());
    class(&format!("{PACKAGE}TestProvider"))
        .method("getName", &[], Some(string()), |_, _| ret(s("jason")))
        .method("getStack", &[], Some(c("java.util.Stack")), |_, _| ret(strings(&["stack element 1", "stack element 2", "stack element 3"], ListKind::ArrayList)))
        .method("getEmptyList", &[], Some(c("java.util.List")), |_, _| ret(JValue::List(JList::array_list(vec![]))))
        .method("getList", &[], Some(c("java.util.List")), |_, _| ret(strings(&["list element 1", "list element 2", "list element 3"], ListKind::ArrayList)))
        .method("getSearch", &[], Some(c("java.util.Hashtable")), |_, _| ret(search()))
        .method("getHashtable", &[], Some(c("java.util.Hashtable")), |_, _| {
            ret(JValue::Map(hashtable(&[("key0", s("value0")), ("key1", s("value1")), ("key2", s("value2"))])))
        })
        .method("getRelSearches", &[], Some(c("java.util.ArrayList")), |_, _| ret(JValue::List(JList::array_list(vec![search()]))))
        .method("getTitle", &[], Some(string()), |b, _| ret(b.field("title")))
        .method("setTitle", &[string()], None, |b, a| {
            b.set_field("title", a[0].clone());
            Ok(None)
        })
        .method("getMenu", &[], Some(object_array()), |_, _| {
            let items = (1..=3)
                .map(|i| JValue::Map(hashtable(&[("id", s(&format!("item{i}"))), ("name", s(&format!("name{i}"))), ("label", s(&format!("label{i}")))])))
                .collect();
            ret(JArray::objects(items))
        })
        .method("getCustomers", &[], Some(c("java.util.ArrayList")), |_, _| {
            ret(strings(&["ArrayList element 1", "ArrayList element 2", "ArrayList element 3", "ArrayList element 4"], ListKind::ArrayList))
        })
        .method("getCustomers2", &[], Some(c("java.util.ArrayList")), |_, _| ret(JValue::List(JList::array_list((0..4).map(|_| test_provider()).collect()))))
        .method("me", &[], Some(object()), |b, _| ret(b.field("this")))
        .method("toString", &[], Some(string()), |_, _| ret(s("test provider")))
        .method("getVector", &[], Some(c("java.util.Vector")), |_, _| ret(strings(&["vector element 1", "vector element 2"], ListKind::ArrayList)))
        .method("getArray", &[], Some(JType::array(string())), |_, _| {
            ret(JValue::Array(Arc::new(JArray::new(string(), vec![s("first element"), s("second element")]))))
        })
        .method("theAPLRules", &[], Some(JType::Boolean), |_, _| ret(JValue::Bool(true)))
        .method("getStateTrue", &[], Some(JType::Boolean), |_, _| ret(JValue::Bool(true)))
        .method("getStateFalse", &[], Some(JType::Boolean), |_, _| ret(JValue::Bool(false)))
        .method("objectArrayMethod", &[object_array()], Some(string()), |_, _| ret(s("result of objectArrayMethod")))
        .method("concat", &[object_array()], Some(string()), |_, a| ret(s(&joined(&list_items(&a[0])))))
        .method("concat", &[c("java.util.List")], Some(string()), |_, a| ret(s(&joined(&list_items(&a[0])))))
        .method("objConcat", &[c("java.util.List")], Some(string()), |_, a| ret(s(&joined(&list_items(&a[0])))))
        .method("parse", &[string(), object(), string(), string()], Some(string()), |_, a| {
            if a[1].is_null() {
                return Err(JError::without_message("java.lang.NullPointerException"));
            }
            ret(s(&format!("{}{}{}{}", a[0].to_java_string(), a[1].to_java_string(), a[2].to_java_string(), a[3].to_java_string())))
        })
        .method("concat", &[string(), string()], Some(string()), |_, a| ret(s(&format!("{}{}", a[0].to_java_string(), a[1].to_java_string()))))
        .method("getPerson", &[], Some(c(&format!("{PACKAGE}Person"))), |_, _| ret(PERSON.new_instance()))
        .method("getChild", &[], Some(c(&format!("{PACKAGE}Child"))), |_, _| ret(CHILD.new_instance()))
        .method("showPerson", &[c(&format!("{PACKAGE}Person"))], Some(string()), |_, a| {
            match &a[0] {
                JValue::Object(o) => ret(s(if o.class_name().ends_with("Child") { "Child" } else { "Person" })),
                _ => Err(JError::without_message("java.lang.NullPointerException")),
            }
        })
        .method("chop", &[string(), JType::Int], Some(string()), |_, a| {
            let text: Vec<u16> = a[0].to_java_string().encode_utf16().collect();
            let i = match a[1] {
                JValue::Int(i) => i as i64,
                _ => 0,
            };
            let end = text.len() as i64 - i;
            if end < 0 || end > text.len() as i64 {
                return Err(JError::new("java.lang.StringIndexOutOfBoundsException", format!("begin 0, end {end}, length {}", text.len())));
            }
            ret(s(&String::from_utf16_lossy(&text[..end as usize])))
        })
        .method("allEmpty", &[object_array()], Some(JType::Boolean), |_, a| ret(JValue::Bool(list_items(&a[0]).iter().all(|v| v.to_java_string().is_empty()))))
        .method("setState", &[c("java.lang.Boolean")], None, |_, _| Ok(None))
        .method("setBangStart", &[c("java.lang.Integer")], None, |b, a| {
            println!("SetBangStart() : called with val = {}", a[0].to_java_string());
            b.set_field("stateint", a[0].clone());
            Ok(None)
        })
        .method("bang", &[], Some(c("java.lang.Integer")), |b, _| {
            let current = match b.field("stateint") {
                JValue::Int(i) => i,
                _ => 0,
            };
            println!("Bang! : {current}");
            b.set_field("stateint", JValue::Int(current + 1));
            ret(JValue::Int(current))
        })
        .method("get", &[string()], Some(string()), |_, a| ret(a[0].clone()))
        .method("put", &[string(), object()], Some(string()), |b, a| {
            b.set_field("ob", a[1].clone());
            ret(a[0].clone())
        })
        .method("getFoo", &[], Some(string()), |_, _| Err(JError::new("java.lang.Exception", "From getFoo()")))
        .method("getThrow", &[], Some(string()), |_, _| Err(JError::new("java.lang.Exception", "From getThrow()")))
        .build()
});

/// `TestProvider.PUB_STAT_STRING`.
pub const PUB_STAT_STRING: &str = "Public Static String";

/// `new TestProvider()`.
pub fn test_provider() -> JValue {
    let v = TEST_PROVIDER.with_fields(&[("title", s("lunatic")), ("state", JValue::Bool(false)), ("ob", JValue::Null), ("stateint", JValue::Int(0))]);
    // `me()` returns the instance itself.
    if let JValue::Object(o) = &v {
        if let Some(b) = o.as_any().downcast_ref::<Bean>() {
            b.set_field("this", JValue::Object(Arc::downgrade(o).upgrade().expect("alive")));
        }
    }
    v
}

pub fn bool_obj() -> JValue {
    static BOOL_OBJ: LazyLock<BeanFactory> = LazyLock::new(|| {
        class(&format!("{PACKAGE}BoolObj"))
            .method("isBoolean", &[], Some(JType::Boolean), |_, _| ret(JValue::Bool(true)))
            .method("isNotboolean", &[], Some(string()), |_, _| ret(s("hello")))
            .build()
    });
    BOOL_OBJ.new_instance()
}

pub fn null_to_string_object() -> JValue {
    static NULL_TO_STRING: LazyLock<BeanFactory> =
        LazyLock::new(|| class(&format!("{PACKAGE}NullToStringObject")).method("toString", &[], Some(string()), |_, _| ret(JValue::Null)).build());
    NULL_TO_STRING.new_instance()
}

pub fn test_number(v: f64) -> JValue {
    static TEST_NUMBER: LazyLock<BeanFactory> = LazyLock::new(|| {
        class(&format!("{PACKAGE}TestNumber"))
            .implements(&["org.apache.velocity.util.TemplateNumber"])
            .method("getAsNumber", &[], Some(c("java.lang.Number")), |b, _| ret(b.field("n")))
            .build()
    });
    TEST_NUMBER.with_fields(&[("n", JValue::Double(v))])
}

pub fn number_methods() -> JValue {
    static NUMBER_METHODS: LazyLock<BeanFactory> = LazyLock::new(|| {
        let m = |label: &'static str| move |_: &Bean, a: &[JValue]| ret(s(&format!("{label} ({})", a[0].to_java_string())));
        class(&format!("{PACKAGE}NumberMethods"))
            .method("numMethod", &[JType::Byte], Some(string()), m("byte"))
            .method("numMethod", &[JType::Short], Some(string()), m("short"))
            .method("numMethod", &[JType::Int], Some(string()), m("int"))
            .method("numMethod", &[JType::Double], Some(string()), m("double"))
            .method("numMethod", &[JType::Long], Some(string()), m("long"))
            .method("numMethod", &[c("java.math.BigInteger")], Some(string()), m("BigInteger"))
            .method("numMethod", &[c("java.math.BigDecimal")], Some(string()), m("BigDecimal"))
            .build()
    });
    NUMBER_METHODS.new_instance()
}

