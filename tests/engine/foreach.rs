//! `ForeachTestCase`.

use java_compat::lang::{JList, JValue, ListKind};

use crate::base::*;

#[test]
fn test_max_nbr_loops_constraint() {
    let mut b = Base::new();
    b.set_property("directive.foreach.max_loops", 3);
    b.assert_eval_equals("1 2 3 ", "#foreach ($item in [1..10])$item #end");
}

#[test]
fn test_collection_and_method_call() {
    let mut b = Base::new();
    let helper = class("org.apache.velocity.test.provider.ForeachMethodCallHelper")
        .method("getFoo", &[java_compat::reflect::JType::class("java.lang.Integer")], Some(string()), |_, a| ret(s(&format!("int {}", a[0].to_java_string()))))
        .method("getFoo", &[string()], Some(string()), |_, a| ret(s(&format!("str {}", a[0].to_java_string()))))
        .build();
    b.put("helper", helper.new_instance());
    b.put("col", JValue::List(JList::array_list(vec![JValue::Int(100), s("STRVALUE")])));
    b.assert_eval_equals("int 100 str STRVALUE ", "#foreach ( $item in $col )$helper.getFoo($item) #end");
}

#[test]
fn test_object_with_iterator_method() {
    let mut b = Base::new();
    let iterable = class("org.apache.velocity.test.ForeachTestCase$MyIterable")
        .method("iterator", &[], Some(java_compat::reflect::JType::class("java.util.Iterator")), |_, _| {
            ret(JValue::List(JList::new(ListKind::Iterator, vec![JValue::Int(1), JValue::Long(2), s("3")])))
        })
        .build();
    b.put("iterable", iterable.new_instance());
    b.assert_eval_equals("1 2 3 ", "#foreach ($i in $iterable)$i #end");
}

#[test]
fn test_not_really_iterable_iterator_method() {
    let mut b = Base::new();
    let nri = class("org.apache.velocity.test.ForeachTestCase$NotReallyIterable").method("iterator", &[], Some(object()), |_, _| ret(new_object())).build();
    b.put("nri", nri.new_instance());
    b.assert_eval_equals("", "#foreach ($i in $nri)$i #end");
}

#[test]
fn test_velocity_has_next_property() {
    let mut b = Base::new();
    b.put("list", JValue::string_list(["test1", "test2", "test3"]));
    b.assert_eval_equals("test1 SEPARATOR test2 SEPARATOR test3 ", "#foreach ($value in $list)$value #if( $foreach.hasNext )SEPARATOR #end#end");
}

#[test]
fn test_nested_velocity_has_next_property() {
    let mut b = Base::new();
    b.put("list", JValue::string_list(["test1", "test2", "test3", "test4"]));
    b.put("list2", JValue::string_list(["a1", "a2", "a3"]));
    b.assert_eval_equals(
        "test1 (a1;a2;a3)-test2 (a1;a2;a3)-test3 (a1;a2;a3)-test4 (a1;a2;a3)",
        "#foreach ($value in $list)$value (#foreach ($val in $list2)$val#if( $foreach.hasNext );#end#end)#if( $foreach.hasNext )-#end#end",
    );
}
