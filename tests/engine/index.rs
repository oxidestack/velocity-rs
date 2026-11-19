//! `IndexTestCase`: `$a[i]` on arrays, lists, maps and `get(…)` objects, and `#set` through an index.

use java_compat::lang::{JArray, JError, JList, JValue};
use java_compat::reflect::JType;

use crate::base::*;

fn base() -> Base {
    let mut b = Base::new();
    b.set_property("runtime.strict_mode.enable", true);
    b.put("NULL", JValue::Null);
    b.put("red", s("blue"));
    let a = JValue::Array(std::sync::Arc::new(JArray::new(JType::Int, vec![JValue::Int(1), JValue::Int(2), JValue::Int(3)])));
    b.put("a", a.clone());
    b.put("str", JArray::strings(["a", "ab", "abc"]));
    let alist = JValue::List(JList::array_list(vec![JValue::Int(1), JValue::Int(2), JValue::Int(3), a, JValue::Null]));
    b.put("alist", alist.clone());
    let foo_class = class("org.apache.velocity.test.IndexTestCase$Foo")
        .method("getBar", &[], Some(object()), |f, _| ret(f.field("bar")))
        .method("get", &[JType::class("java.lang.Boolean")], Some(string()), |_, a| ret(s(if matches!(a[0], JValue::Bool(true)) { "BIG TRUE" } else { "BIG FALSE" })))
        .method("get", &[string()], Some(string()), |_, a| ret(s(&format!("{} foobar ", a[0].to_java_string()))))
        .build();
    b.put("foo", foo_class.with_fields(&[("bar", alist)]));
    let boo = class("org.apache.velocity.test.IndexTestCase$Boo")
        .method("get", &[object()], Some(object()), |_, a| match &a[0] {
            JValue::Null => ret(s("GOT NULL")),
            JValue::Str(x) if &**x == "throwex" => Err(JError::new("java.lang.RuntimeException", "Generated Exception")),
            other => ret(other.clone()),
        })
        .build();
    b.put("boo", boo.new_instance());
    b
}

#[test]
fn test_calling_index() {
    let mut b = base();
    b.assert_eval_equals("1 -3-", "$a[0] -$a[ 2 ]-");
    b.assert_eval_equals("x21", "#set($i = 1)x$a[$i]1");
    b.assert_eval_equals("3", "$a[ $a[ $a[0] ] ]");
    b.assert_eval_equals("ab", "$str[1]");
    b.assert_eval_equals("123", "$alist[0]$alist[1]$alist[2]");
    b.assert_eval_equals("1][2-[3]", "$alist[0]][$alist[$a[0]]-[$alist[2]]");
    b.assert_eval_equals("2", "$alist[3][1]");
    b.assert_eval_equals("3 [1]", "$alist[2] [1]");
    b.assert_eval_equals("4", "#set($bar = [3, 4, 5])$bar[1]");
    b.assert_eval_equals("21", "#set($i = 1)#set($bar = [3, $a[$i], $a[0]])$bar[1]$bar[2]");
    b.assert_eval_equals("2", "$foo.bar[1]");
    b.assert_eval_equals("2", "$foo.getBar()[1]");
    b.assert_eval_equals("2", "$foo.getBar()[3][1]");
    b.assert_eval_equals(" a  ab  abc ", "#foreach($i in $foo.bar[3]) $str[$foreach.index] #end");
    b.assert_eval_equals("apple", "#set($hash = {'a':'apple', 'b':'orange'})$hash['a']");
    b.assert_eval_equals("xx ", "#if($alist[4] == $NULL)xx#end #if($alist[4])yy#end");
    b.assert_eval_equals("BIG TRUEaBIG FALSE", "$foo[true]a$foo[false]");
    b.assert_eval_equals("junk foobar ", "$foo[\"junk\"]");
    b.assert_eval_equals("GOT NULL", "#set($i=$NULL)$boo[$i]");
    b.assert_eval_equals("321", "$a[-1]$a[ -2]$a[-3 ]");
    b.assert_eval_equals("67xx", "#set($hash={1:11, 5:67, 23:2})$hash[5]$!hash[6]#if(!$hash[1000])xx#end");
    for t in ["[]", "$[]", "$.[]", "$[1]", "$[1]1", "$1[1]1"] {
        b.assert_eval_equals(t, t);
    }
    b.assert_eval_equals("blue.[]", "$red.[]");
    b.assert_eval_equals("blue[]", "${red}[]");
    b.assert_eval_equals("blue][", "$red][");
    b.assert_eval_equals("1[]", "${a[0]}[]");
    b.assert_eval_equals("1a$[]", "$a[0]a$[]");
    b.assert_eval_equals("$![]", "$![]");
    b.assert_eval_equals("$\\![]", "$\\![]");
}

#[test]
fn test_index_setting() {
    let mut b = base();
    b.assert_eval_equals("foo", "#set($str[1] = \"foo\")$str[1]");
    b.assert_eval_equals("5150", "#set($alist[1] = 5150)$alist[1]");
    b.assert_eval_equals("15", "$alist[3][0]#set($alist[3][0] = 5)$alist[3][0]");
    b.assert_eval_equals("orange", "#set($blaa = {\"apple\":\"grape\"})#set($blaa[\"apple\"] = \"orange\")$blaa[\"apple\"]");
    b.assert_eval_equals("null", "#set($str[0] = $NULL)#if($str[0] == $NULL)null#end");
    b.assert_eval_equals("null", "#set($blaa = {\"apple\":\"grape\"})#set($blaa[\"apple\"] = $NULL)#if($blaa[\"apple\"] == $NULL)null#end");
    b.assert_eval_equals("2112", "#set($a[-1] = 2112)$a[2]");
    b.assert_eval_equals("3344", "#set($hash = {1:11, 2:22, 5:66})#set($hash[2]=33)#set($hash[3]=44)$hash[2]$hash[3]");
}

#[test]
fn test_error_handling() {
    let mut b = base();
    b.assert_eval_exception_at("$boo['throwex']", "", 1, 5);
    b.assert_eval_exception_at("$boo[]", "", 1, 6);
    b.assert_eval_exception_at("$boo[blaa]", "", 1, 6);
    b.assert_eval_exception_at("#set($foo[1] = 3)", "", 1, 10);
    b.assert_eval_exception_at("$a[500]", "", 1, 3);
}
