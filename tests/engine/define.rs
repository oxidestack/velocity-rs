//! `DefineTestCase`.

use java_compat::lang::JValue;

use crate::base::*;

fn def_and_eval(b: &mut Base, key: &str, block: &str) -> String {
    b.evaluate(&format!("#define( ${key} ){block}#end${key}")).unwrap()
}

#[test]
fn test_simple() {
    let mut b = Base::new();
    assert_eq!(def_and_eval(&mut b, "def", "abc"), "abc");
    b.assert_eval_equals("abc abc abc", "#define( $a )abc#end$a $a $a");
}

#[test]
fn test_not_simple() {
    let mut b = Base::new();
    assert_eq!(def_and_eval(&mut b, "def", "#if( $def )true#end"), "true");
    assert_eq!(def_and_eval(&mut b, "def", "#foreach( $i in [1..3] )$i#end"), "123");
    assert_eq!(def_and_eval(&mut b, "def", "#macro( test )hello world#end#test()"), "hello world");
}

#[test]
fn test_overriding_definition_internally() {
    Base::new().assert_eval_equals("truefalse", "#define( $or )true#set( $or = false )#end$or$or");
}

#[test]
fn test_late_binding() {
    let mut b = Base::new();
    b.put("baz", s("foo"));
    b.assert_eval_equals("foobar", "#define( $lb )$baz#end${lb}#set( $baz = 'bar' )${lb}");
}

#[test]
fn test_rerendering() {
    let mut b = Base::new();
    let inc = class("org.apache.velocity.test.DefineTestCase$Inc")
        .method("toString", &[], Some(string()), |b, _| {
            let v = match b.field("foo") {
                JValue::Int(i) => i,
                _ => 1,
            };
            b.set_field("foo", JValue::Int(v + 1));
            ret(s(&v.to_string()))
        })
        .build();
    b.put("inc", inc.with_fields(&[("foo", JValue::Int(1))]));
    b.assert_eval_equals("1 2 3", "#define( $i )$inc#end$i $i $i");
}

#[test]
fn test_assignation() {
    Base::new().assert_eval_equals("[][hello]", "#define( $orig )hello#end[#set( $assig = $orig )][$assig]");
}

#[test]
fn test_non_rendering_usage() {
    let template = "#define($foo)\n foo_contents\n#end\n#if ($foo)\n found foo\n#end";
    Base::new().assert_eval_equals(" found foo\n", template);
}

#[test]
fn test_recursion_limit() {
    Base::new().assert_eval_equals("$r", "#define( $r )$r#end$r");
}

#[test]
fn test_things_of_questionable_morality() {
    let mut b = Base::new();
    assert_eq!(def_and_eval(&mut b, "foo", "foo#define( $foo )bar#end$foo"), "foobar");
}
