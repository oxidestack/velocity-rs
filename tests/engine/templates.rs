//! `TemplateTestCase` (the `TemplateTestSuite` of `templates/templates.properties`): each template merged with the
//! suite's context and compared with its `.cmp`.

use std::sync::Arc;

use java_compat::lang::{JArray, JList, JMap, JValue, ListKind, MapKind};
use java_compat::reflect::JType;
use velocity::field_methodizer::FieldMethodizer;
use velocity::props::{ExtProperties, PropValue};
use velocity::{Context, Engine};

use crate::base::*;
use crate::provider::*;
use crate::test_logger::TestLogger;

fn engine() -> Engine {
    let mut p = ExtProperties::new();
    p.set_property("file.resource.loader.path", resource("templates").into());
    let instance: Arc<dyn velocity::log::Logger> = Arc::new(TestLogger::default());
    p.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    p.set_property("space.gobbling", "bc".into());
    Engine::with_properties(&p).expect("initialises")
}

fn provider_value(provider: &JValue, method: &str) -> JValue {
    let JValue::Object(o) = provider else { unreachable!() };
    let def = o.class_def().expect("typed");
    let m = def.methods.iter().find(|m| &*m.name == method && m.params.is_empty()).expect("a getter");
    (m.invoke)(provider, &[]).expect("returns").unwrap_or(JValue::Null)
}

/// `setUp()`: three chained contexts (`context` over `context1` over `context2`).
fn context() -> Context {
    let provider = test_provider();
    let al = provider_value(&provider, "getCustomers");
    let h = JMap::new(MapKind::Linked);
    h.put(s("Bar"), s("this is from a hashtable!"));
    h.put(s("Foo"), s("this is from a hashtable too!"));
    let vec = JList::array_list(vec![s("string1"), s("string2")]);

    let mut context2 = Context::new();
    context2.put("providers", provider_value(&provider, "getCustomers2"));
    context2.put("hashmap", JValue::Map(JMap::new(MapKind::Hash)));
    context2.put("search", provider_value(&provider, "getSearch"));
    context2.put("stringarray", provider_value(&provider, "getArray"));
    context2.put("iterator", JValue::List(JList::new(ListKind::Iterator, vec.items())));

    let mut context1 = Context::wrap(&context2);
    context1.put("name", s("jason"));
    context1.put("name2", s("jason"));
    context1.put("name3", s("geoge"));
    context1.put("hashtable", JValue::Map(h.clone()));
    context1.put("searchResults", provider_value(&provider, "getRelSearches"));
    context1.put("map", JValue::Map(h));

    let mut context = Context::wrap(&context1);
    context.put("provider", provider.clone());
    context.put("list", al);
    context.put("relatedSearches", provider_value(&provider, "getRelSearches"));
    context.put("vector", JValue::List(vec.clone()));
    context.put("mystring", s(""));
    context.put("runtime", FieldMethodizer::runtime_singleton().value());
    let mut fmprov = FieldMethodizer::new();
    fmprov.add_fields([("PUB_STAT_STRING", s(PUB_STAT_STRING))]);
    context.put("fmprov", fmprov.value());
    context.put("Floog", s("floogie woogie"));
    context.put("boolobj", bool_obj());
    context.put("collection", JValue::List(vec.clone()));
    context.put("obarr", JArray::objects(vec![s("a"), s("b"), s("c"), s("d")]));
    context.put("enumerator", JValue::List(JList::new(ListKind::Iterator, vec.items())));
    context.put("intarr", JValue::Array(Arc::new(JArray::new(JType::Int, [10, 20, 30, 40, 50].map(JValue::Int).to_vec()))));
    context.put("emptyarr", JValue::Array(Arc::new(JArray::new(JType::Int, vec![]))));
    context.put("int1", JValue::Int(1000));
    context.put("long1", JValue::Long(10000000000));
    context.put("float1", JValue::Float(1000.1234));
    context.put("double1", JValue::Double(10000000000.0));
    context.put("templatenumber1", test_number(999.125));
    context.put("nullList", JValue::List(JList::array_list(vec![s("a"), s("b"), JValue::Null, s("d")])));
    context.put("nullToString", null_to_string_object());
    context
}

/// The expectations that depend on JDK classes rather than on Velocity (hash-table enumeration order, `Vector`
/// methods): the fixtures are Rust structures, so these expected outputs are adapted.
fn adapted(base: &str, compare: String) -> String {
    match base {
        // `Hashtable` enumeration order; the fixture map keeps insertion order.
        "foreach-map" => compare.replace("    value2\n    value1\n    value0\n", "    value0\n    value1\n    value2\n"),
        // `Vector.firstElement()`; the fixture vector is a plain list.
        "reference" => compare.replace("Late introspection :\n\n7\n", "Late introspection :\n\n$vector.firstElement().length()\n"),
        // `Iterator.next()` on a live iterator object.
        "foreach-introspect" => (1..=10).fold(compare, |c, i| c.replace(&format!("   {} {i}\n", 20 + i), &format!("   {} $it.next()\n", 20 + i))),
        _ => compare,
    }
}

fn run(base: &str) {
    let engine = engine();
    let template = engine.get_template(&format!("{base}.vm"), None).unwrap_or_else(|e| panic!("{base}.vm: {}", e.message()));
    let out = engine.merge(&template, &mut context()).unwrap_or_else(|e| panic!("{base}.vm threw {}: {}", e.class_name(), e.message()));
    let compare = adapted(base, file_contents(&resource(&format!("templates/compare/{base}.cmp"))));
    assert_eq!(normalize_newlines(&out), normalize_newlines(&compare), "Processed template {base} did not match expected output");
}

macro_rules! template_test {
    ($name:ident, $base:expr) => {
        #[test]
        fn $name() {
            run($base);
        }
    };
}

template_test!(arithmetic, "arithmetic");
template_test!(array, "array");
template_test!(block, "block");
template_test!(comment, "comment");
template_test!(equality, "equality");
template_test!(escape, "escape");
template_test!(foreach_array, "foreach-array");
template_test!(foreach_method, "foreach-method");
template_test!(foreach_variable, "foreach-variable");
template_test!(formal, "formal");
template_test!(if_, "if");
template_test!(logical, "logical");
template_test!(loop_, "loop");
template_test!(method, "method");
template_test!(quotes, "quotes");
template_test!(sample, "sample");
template_test!(shorthand, "shorthand");
template_test!(test, "test");
template_test!(diabolical, "diabolical");
template_test!(pedantic, "pedantic");
template_test!(subclass, "subclass");
template_test!(foreach_map, "foreach-map");
template_test!(include, "include");
template_test!(escape2, "escape2");
template_test!(parse, "parse");
template_test!(velocimacro, "velocimacro");
template_test!(reference, "reference");
template_test!(interpolation, "interpolation");
template_test!(vm_test1, "vm_test1");
template_test!(map, "map");
template_test!(ifstatement, "ifstatement");
template_test!(math, "math");
template_test!(range, "range");
template_test!(get, "get");
template_test!(velocimacro2, "velocimacro2");
template_test!(foreach_type, "foreach-type");
template_test!(foreach_introspect, "foreach-introspect");
template_test!(settest, "settest");
template_test!(newline, "newline");
template_test!(logical2, "logical2");
template_test!(string, "string");
template_test!(stop1, "stop1");
template_test!(stop2, "stop2");
template_test!(foreach_null_list, "foreach-null-list");
template_test!(curly_directive, "curly-directive");
template_test!(comment_eof, "comment-eof");
template_test!(commas, "commas");
template_test!(stop3, "stop3");
