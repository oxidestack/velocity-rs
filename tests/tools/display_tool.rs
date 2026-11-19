//! `org.apache.velocity.tools.generic.DisplayToolTests`.

use std::sync::Arc;

use java_compat::lang::{JObject, JValue};
use java_compat::reflect::{ClassDef, JType, LazyClass};
use velocity::tools::display::*;

use crate::base::{i, ints, list, map, s};

fn some(v: &str) -> Option<String> {
    Some(v.to_string())
}

#[test]
fn method_alt_object() {
    let display = DisplayTool::new();
    assert_eq!(display.alt(&JValue::Null).as_str().map(str::to_string), display.get_default_alternate());
    let notnull = JValue::Object(Arc::new(velocity::tools::jdk_object()));
    assert!(java_compat::lang::java_equals(&display.alt(&notnull), &notnull));
}

#[test]
fn method_alt_object_object() {
    let display = DisplayTool::new();
    let notnull = JValue::Object(Arc::new(velocity::tools::jdk_object()));
    assert!(java_compat::lang::java_equals(&display.alt_or(&JValue::Null, &notnull), &notnull));
    assert!(java_compat::lang::java_equals(&display.alt_or(&notnull, &s("foo")), &notnull));
}

#[test]
fn method_capitalize_object() {
    let display = DisplayTool::new();
    assert_eq!(display.capitalize(&s("fOO")), some("FOO"));
    assert_eq!(display.capitalize(&s("Foo")), some("Foo"));
    assert_eq!(display.capitalize(&s("foo")), some("Foo"));
    assert_eq!(display.capitalize(&s("f")), some("F"));
    assert_eq!(display.capitalize(&s("")), some(""));
    assert_eq!(display.capitalize(&JValue::Null), None);
}

#[test]
fn method_cell_object() {
    let display = DisplayTool::new();
    display.set_cell_length(4);
    assert_eq!(display.cell(&JValue::Null), None);
    assert_eq!(display.cell(&s("foo")), some("foo "));
    assert_eq!(display.cell(&s("foobar")), some("f..."));
    assert_eq!(display.cell(&s("foob")), some("foob"));
}

#[test]
fn method_cell_object_string() {
    let display = DisplayTool::new();
    display.set_cell_length(5);
    assert_eq!(display.cell_suffixed(&s("testing"), Some(">")), some("test>"));
}

#[test]
fn method_cell_objectint() {
    let display = DisplayTool::new();
    assert_eq!(display.cell_sized(&s("testing"), 7), some("testing"));
    assert_eq!(display.cell_sized(&s("testing"), 8), some("testing "));
    assert_eq!(display.cell_sized(&s("testing"), 6), some("tes..."));
}

#[test]
fn method_cell_objectint_string() {
    let display = DisplayTool::new();
    assert_eq!(display.cell_with(&s("foo"), 1, None), some("f"));
    assert_eq!(display.cell_with(&s("foo"), 1, Some("bar")), some("f"));
    assert_eq!(display.cell_with(&s("foobar"), 4, Some("bar")), some("fbar"));
    assert_eq!(display.cell_with(&s("foo"), 0, Some("bar")), None);
    assert_eq!(display.cell_with(&s("foo"), -1, Some("bar")), None);
}

#[test]
fn method_configure_map() {
    let display = DisplayTool::new();
    let conf = map(&[
        (LIST_DELIM_KEY, s(";")),
        (LIST_FINAL_DELIM_KEY, s(" und ")),
        (TRUNCATE_LENGTH_KEY, s("5")),
        (TRUNCATE_SUFFIX_KEY, s(">")),
        (TRUNCATE_AT_WORD_KEY, s("true")),
        (CELL_LENGTH_KEY, s("4")),
        (CELL_SUFFIX_KEY, s("~")),
        (DEFAULT_ALTERNATE_KEY, s("n/a")),
        (ALLOWED_TAGS_KEY, s("img,br")),
    ]);
    display.configure(&conf);
    assert_eq!(display.get_list_delimiter(), some(";"));
    assert_eq!(display.get_list_final_delimiter(), some(" und "));
    assert_eq!(display.get_truncate_length(), 5);
    assert_eq!(display.get_truncate_suffix(), some(">"));
    assert!(display.get_truncate_at_word());
    assert_eq!(display.get_cell_suffix(), some("~"));
    assert_eq!(display.get_cell_length(), 4);
    assert_eq!(display.get_default_alternate(), some("n/a"));
    let tags = display.get_allowed_tags().expect("tags");
    assert_eq!(tags[0], some("img"));
    assert_eq!(tags[1], some("br"));
    // configure is locked now
    conf.put_str(LIST_DELIM_KEY, s(" & "));
    display.configure(&conf);
    assert_eq!(display.get_list_delimiter(), some(";"));
}

#[test]
fn method_measure_string() {
    let display = DisplayTool::new();
    assert!(display.measure(&JValue::Null).is_none());
    let dims = display.measure(&s("")).unwrap();
    assert_eq!(dims.get_height(), 1);
    assert_eq!(dims.get_width(), 0);
    let dims = display.measure(&s("twelve chars")).unwrap();
    assert_eq!(dims.get_width(), 12);
    assert_eq!(dims.get_height(), 1);
    let dims = display.measure(&s("one\ntwo\nthree")).unwrap();
    assert_eq!(dims.get_width(), 5);
    assert_eq!(dims.get_height(), 3);
}

#[test]
fn method_message_string_object_var_args() {
    let display = DisplayTool::new();
    assert_eq!(display.message(None, Some(&[])).unwrap(), None);
    assert_eq!(display.message(Some("foo"), Some(&[])).unwrap(), some("foo"));
    assert_eq!(display.message(Some("foo"), None).unwrap(), some("foo"));
    assert_eq!(display.message(Some("foo"), Some(&[])).unwrap(), some("foo"));
    assert_eq!(display.message(Some("foo"), Some(&[list(&[])])).unwrap(), some("foo"));
    assert_eq!(display.message(Some("foo"), Some(&[i(1)])).unwrap(), some("foo"));
    assert_eq!(display.message(Some("foo {0}"), Some(&[s("bar")])).unwrap(), some("foo bar"));
    assert_eq!(display.message(Some("foo {1} {0}"), Some(&[s("bar"), i(2)])).unwrap(), some("foo 2 bar"));
}

#[test]
fn method_printf_string_object_var_args() {
    let display = DisplayTool::new();
    assert_eq!(display.printf(None, Some(&[])).unwrap(), None);
    assert_eq!(display.printf(Some("foo"), Some(&[])).unwrap(), some("foo"));
    assert_eq!(display.printf(Some("foo"), None).unwrap(), some("foo"));
    assert_eq!(display.printf(Some("foo"), Some(&[])).unwrap(), some("foo"));
    assert_eq!(display.printf(Some("foo"), Some(&[list(&[])])).unwrap(), some("foo"));
    assert_eq!(display.printf(Some("foo"), Some(&[i(1)])).unwrap(), some("foo"));
    assert_eq!(display.printf(Some("foo %s"), Some(&[s("bar")])).unwrap(), some("foo bar"));
    assert_eq!(display.printf(Some("foo %2$d %1$s"), Some(&[s("bar"), i(2)])).unwrap(), some("foo 2 bar"));
}

#[test]
fn method_list_object() {
    let display = DisplayTool::new();
    let nums = ints(&[1, 2, 3]);
    assert_eq!(display.list(&nums).unwrap(), some("1, 2 and 3"));
    display.set_list_delimiter(Some(" & "));
    assert_eq!(display.list(&nums).unwrap(), some("1 & 2 and 3"));
    display.set_list_final_delimiter(Some(" & "));
    assert_eq!(display.list(&nums).unwrap(), some("1 & 2 & 3"));
}

#[test]
fn method_list_object_string() {
    let display = DisplayTool::new();
    let nums = list(&[i(1), i(2), i(3)]);
    assert_eq!(display.list_delimited(&JValue::Null, None).unwrap(), None);
    assert_eq!(display.list_delimited(&nums, None).unwrap(), some("1null2null3"));
    assert_eq!(display.list_delimited(&nums, Some(", ")).unwrap(), some("1, 2, 3"));
}

#[test]
fn method_list_object_string_string() {
    let display = DisplayTool::new();
    let nums = ints(&[1, 2, 3]);
    assert_eq!(display.list_with(&JValue::Null, None, None).unwrap(), None);
    assert_eq!(display.list_with(&nums, None, None).unwrap(), some("1null2null3"));
    assert_eq!(display.list_with(&nums, Some(" & "), None).unwrap(), some("1 & 2null3"));
    assert_eq!(display.list_with(&nums, None, Some(" & ")).unwrap(), some("1null2 & 3"));
    assert_eq!(display.list_with(&nums, Some(""), Some("")).unwrap(), some("123"));
    assert_eq!(display.list_with(&nums, Some("; "), Some(" und ")).unwrap(), some("1; 2 und 3"));
}

/// `DisplayToolTests.TestBean`.
struct TestBean {
    num: i32,
    str: &'static str,
}

fn test_bean_def() -> Arc<ClassDef> {
    ClassDef::new("org.apache.velocity.tools.generic.DisplayToolTests$TestBean")
        .method::<TestBean>("getNum", &[], Some(JType::Int), |b, _, _| Ok(Some(JValue::Int(b.num))))
        .method::<TestBean>("getStr", &[], Some(JType::string()), |b, _, _| Ok(Some(s(b.str))))
        .register()
}

impl JObject for TestBean {
    fn class_name(&self) -> String {
        "org.apache.velocity.tools.generic.DisplayToolTests$TestBean".into()
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<Result<Option<JValue>, java_compat::lang::JError>> {
        None
    }

    fn to_java_string(&self) -> String {
        java_compat::reflect::identity_string(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<ClassDef>> {
        static DEF: LazyClass = LazyClass::new(test_bean_def);
        Some(DEF.get())
    }
}

#[test]
fn method_list_object_string_string_string() {
    let bean = |num, str| JValue::Object(Arc::new(TestBean { num, str }));
    let beans = [bean(1, "one"), bean(2, "two"), bean(3, "three")];
    let bean_array = JValue::Array(Arc::new(java_compat::lang::JArray::new(JType::class("org.apache.velocity.tools.generic.DisplayToolTests$TestBean"), beans.to_vec())));
    let bean_list = list(&beans);
    let display = DisplayTool::new();
    assert_eq!(display.list_property(&JValue::Null, None, None, None).unwrap(), None);
    assert_eq!(display.list_property(&bean_array, None, None, Some("num")).unwrap(), some("1null2null3"));
    assert_eq!(display.list_property(&bean_list, Some(""), Some(""), Some("num")).unwrap(), some("123"));
    assert_eq!(display.list_property(&bean_list, Some(", "), Some(" or "), Some("str")).unwrap(), some("one, two or three"));
}

#[test]
fn method_set_allowed_tags_string_array() {
    let display = DisplayTool::new();
    assert!(display.get_allowed_tags().is_none());
    let tags = vec![some("img")];
    display.set_allowed_tags(Some(tags.clone()));
    assert_eq!(display.get_allowed_tags(), Some(tags));
}

#[test]
fn method_set_cell_length_int() {
    let display = DisplayTool::new();
    display.set_cell_length(10);
    assert_eq!(display.get_cell_length(), 10);
}

#[test]
fn method_set_cell_suffix_string() {
    let display = DisplayTool::new();
    display.set_cell_suffix(Some("foo"));
    assert_eq!(display.get_cell_suffix(), some("foo"));
}

#[test]
fn method_set_default_alternate_string() {
    let display = DisplayTool::new();
    display.set_default_alternate(Some("foo"));
    assert_eq!(display.get_default_alternate(), some("foo"));
}

#[test]
fn method_set_list_delimiter_string() {
    let display = DisplayTool::new();
    display.set_list_delimiter(Some("foo"));
    assert_eq!(display.get_list_delimiter(), some("foo"));
}

#[test]
fn method_set_list_final_delimiter_string() {
    let display = DisplayTool::new();
    display.set_list_final_delimiter(Some("foo"));
    assert_eq!(display.get_list_final_delimiter(), some("foo"));
}

#[test]
fn method_set_truncate_length_int() {
    let display = DisplayTool::new();
    display.set_truncate_length(5);
    assert_eq!(display.get_truncate_length(), 5);
}

#[test]
fn method_set_truncate_suffix_string() {
    let display = DisplayTool::new();
    display.set_truncate_suffix(Some("foo"));
    assert_eq!(display.get_truncate_suffix(), some("foo"));
}

#[test]
fn method_set_truncate_at_word_string() {
    let display = DisplayTool::new();
    assert!(!display.get_truncate_at_word());
    display.set_truncate_at_word(true);
    assert!(display.get_truncate_at_word());
}

#[test]
fn method_space_int() {
    let display = DisplayTool::new();
    assert_eq!(display.space(-1), None);
    assert_eq!(display.space(0), some(""));
    assert_eq!(display.space(1), some(" "));
    assert_eq!(display.space(5), some("     "));
}

#[test]
fn method_truncate_object() {
    let display = DisplayTool::new();
    display.set_truncate_length(4);
    assert_eq!(display.truncate(&JValue::Null), None);
    assert_eq!(display.truncate(&s("foobar")), some("f..."));
    assert_eq!(display.truncate(&s("foob")), some("foob"));
    assert_eq!(display.truncate(&s("foo")), some("foo"));
}

#[test]
fn method_truncate_object_string() {
    let display = DisplayTool::new();
    display.set_truncate_length(4);
    assert_eq!(display.truncate_suffixed(&JValue::Null, Some(">")), None);
    assert_eq!(display.truncate_suffixed(&s("foobar"), Some(">")), some("foo>"));
    assert_eq!(display.truncate_suffixed(&s("foobar"), None), some("foob"));
    assert_eq!(display.truncate_suffixed(&s("foobar"), Some("woogie")), some("foob"));
    assert_eq!(display.truncate_suffixed(&s("foo"), Some(">")), some("foo"));
}

#[test]
fn method_truncate_objectint() {
    let display = DisplayTool::new();
    assert_eq!(display.truncate_to(&JValue::Null, 1), None);
    assert_eq!(display.truncate_to(&s("foobar"), -1), None);
    assert_eq!(display.truncate_to(&s("foobar"), 0), None);
    assert_eq!(display.truncate_to(&s("foobar"), 1), some("f"));
    assert_eq!(display.truncate_to(&s("foobar"), 2), some("fo"));
    assert_eq!(display.truncate_to(&s("foobar"), 3), some("foo"));
    assert_eq!(display.truncate_to(&s("foobar"), 4), some("f..."));
    assert_eq!(display.truncate_to(&s("foobar"), 5), some("fo..."));
    assert_eq!(display.truncate_to(&s("foobar"), 6), some("foobar"));
    assert_eq!(display.truncate_to(&s("foobar"), 7), some("foobar"));
}

#[test]
fn method_truncate_objectint_string() {
    let display = DisplayTool::new();
    assert_eq!(display.truncate_with(&JValue::Null, 0, None), None);
    assert_eq!(display.truncate_with(&s("foo"), 0, None), None);
    assert_eq!(display.truncate_with(&s("foo"), 1, None), some("f"));
    assert_eq!(display.truncate_with(&s("foobar"), 4, None), some("foob"));
    assert_eq!(display.truncate_with(&s("foobar"), 4, Some(">")), some("foo>"));
}

#[test]
fn method_truncate_objectint_stringboolean() {
    let display = DisplayTool::new();
    assert_eq!(display.truncate_full(&JValue::Null, 0, None, true), None);
    assert_eq!(display.truncate_full(&s("foo"), 0, None, false), None);
    assert_eq!(display.truncate_full(&s("foo"), 1, None, true), some("f"));
    assert_eq!(display.truncate_full(&s("long string"), 10, Some(">"), false), some("long stri>"));
    assert_eq!(display.truncate_full(&s("long string"), 10, Some(">"), true), some("long>"));
}

#[test]
fn method_uncapitalize_object() {
    let display = DisplayTool::new();
    assert_eq!(display.uncapitalize(&JValue::Null), None);
    assert_eq!(display.uncapitalize(&s("")), some(""));
    assert_eq!(display.uncapitalize(&s("test")), some("test"));
    assert_eq!(display.uncapitalize(&s("Test")), some("test"));
    assert_eq!(display.uncapitalize(&s("TEST")), some("tEST"));
}

#[test]
fn method_br_object() {
    let display = DisplayTool::new();
    assert_eq!(display.br(&JValue::Null), None);
    assert_eq!(display.br(&s("")), some(""));
    assert_eq!(display.br(&s("\n")), some("<br />\n"));
    assert_eq!(display.br(&s("line1 \n LINE2")), some("line1 <br />\n LINE2"));
}

const HTML: &str = "<p>paragraph <a href=\"url\" target='t'>link</a></p> <h1>header1</h1> <h2>header2</h2> <br><br/><br  /><b>bold</b>";

#[test]
fn method_strip_tags_object() {
    let display = DisplayTool::new();
    assert_eq!(display.strip_tags(&JValue::Null).unwrap(), None);
    assert_eq!(display.strip_tags(&s("")).unwrap(), some(""));
    assert_eq!(display.strip_tags(&s(HTML)).unwrap(), some("paragraph link header1 header2 bold"));
}

#[test]
fn method_strip_tags_object_string_var_args() {
    let display = DisplayTool::new();
    assert_eq!(display.strip_tags_allowing(&JValue::Null, None).unwrap(), None);
    assert_eq!(display.strip_tags_allowing(&s(""), Some(&[some(""), some("")])).unwrap(), some(""));
    assert_eq!(display.strip_tags_allowing(&s(HTML), Some(&[some("h1"), some("h2")])).unwrap(), some("paragraph link <h1>header1</h1> <h2>header2</h2> bold"));
    assert_eq!(display.strip_tags_allowing(&s(HTML), Some(&[some("a")])).unwrap(), some("paragraph <a href=\"url\" target='t'>link</a> header1 header2 bold"));
    assert_eq!(display.strip_tags_allowing(&s(HTML), Some(&[some("b"), some(""), None, some("br")])).unwrap(), some("paragraph link header1 header2 <br><br/><br  /><b>bold</b>"));
}

#[test]
fn method_plural_int_string() {
    let display = DisplayTool::new();
    assert_eq!(display.plural(1, None), None);
    assert_eq!(display.plural(2, Some("")), some(""));
    assert_eq!(display.plural(0, Some("item")), some("items"));
    assert_eq!(display.plural(-1, Some("item")), some("item"));
    assert_eq!(display.plural(2, Some("555")), some("555s"));
    assert_eq!(display.plural(2, Some("TOY")), some("TOYS"));
    assert_eq!(display.plural(2, Some("lady")), some("ladies"));
    assert_eq!(display.plural(2, Some("fox")), some("foxes"));
    assert_eq!(display.plural(2, Some("church")), some("churches"));
}

#[test]
fn method_plural_int_string_string() {
    let display = DisplayTool::new();
    assert_eq!(display.plural_with(1, None, None), None);
    assert_eq!(display.plural_with(2, Some("empty"), Some("")), some(""));
    assert_eq!(display.plural_with(0, Some("man"), Some("men")), some("men"));
    assert_eq!(display.plural_with(-1, Some("mouse"), Some("mice")), some("mouse"));
}
