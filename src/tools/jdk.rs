//! The JDK classes the tools reflect on, as JDK 26's `java.base` declares them (members abridged to what
//! reflection users here look at: modifiers, public static fields, the no-argument constructor, `Object`'s
//! methods).

use java_compat::lang::JValue;
use java_compat::reflect::JType;

use super::classes::modifier::*;
use super::classes::{Annotation, AnnotationValue, Field, JavaClass, Member};

fn o() -> JType {
    JType::object()
}

fn s() -> JType {
    JType::string()
}

fn object() -> JavaClass {
    let class = || JType::class("java.lang.Class");
    JavaClass::new("java.lang.Object")
        .extends(None)
        .constructor(Member::new("java.lang.Object", PUBLIC, &[], None))
        .method(Member::new("getClass", PUBLIC | FINAL | NATIVE, &[], Some(class())))
        .method(Member::new("hashCode", PUBLIC | NATIVE, &[], Some(JType::Int)))
        .method(Member::new("equals", PUBLIC, &[o()], Some(JType::Boolean)))
        .method(Member::new("clone", PROTECTED | NATIVE, &[], Some(o())))
        .method(Member::new("toString", PUBLIC, &[], Some(s())))
        .method(Member::new("notify", PUBLIC | FINAL | NATIVE, &[], None))
        .method(Member::new("notifyAll", PUBLIC | FINAL | NATIVE, &[], None))
        .method(Member::new("wait", PUBLIC | FINAL, &[], None))
        .method(Member::new("wait", PUBLIC | FINAL, &[JType::Long], None))
        .method(Member::new("wait", PUBLIC | FINAL, &[JType::Long, JType::Int], None))
        .method(Member::new("wait0", PRIVATE | FINAL | NATIVE, &[JType::Long], None))
        .method(
            Member::new("finalize", PROTECTED, &[], None)
                .annotated(Annotation::new(super::classes::DEPRECATED).with("forRemoval", AnnotationValue::Bool(true)).with("since", AnnotationValue::Str("9".into()))),
        )
        .instantiable(|| Ok(JValue::Object(std::sync::Arc::new(PlainObject))))
}

/// A `new Object()`.
pub struct PlainObject;

impl java_compat::lang::JObject for PlainObject {
    fn class_name(&self) -> String {
        "java.lang.Object".into()
    }

    fn invoke(&self, name: &str, args: &[JValue]) -> Option<Result<Option<JValue>, java_compat::lang::JError>> {
        match (name, args) {
            ("toString", []) => Some(Ok(Some(JValue::str(self.to_java_string())))),
            _ => None,
        }
    }

    fn to_java_string(&self) -> String {
        java_compat::reflect::identity_string(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A boxed primitive class: final, `Number` (or `Object`), with its constants.
fn boxed(name: &str, superclass: &str, fields: Vec<Field>) -> JavaClass {
    let mut c = JavaClass::new(name).modifiers(PUBLIC | FINAL).extends(Some(superclass)).implements(&["java.lang.Comparable", "java.lang.constant.Constable", "java.lang.constant.ConstantDesc"]);
    for f in fields {
        c = c.field(f);
    }
    c
}

fn class_field(name: &str, primitive: &str) -> Field {
    let primitive = primitive.to_string();
    Field::public_static(name, true, JType::class("java.lang.Class"), move || super::classes::class_value(&primitive))
}

pub(crate) fn register() {
    object().register();
    for i in ["java.io.Serializable", "java.lang.Cloneable", "java.lang.Comparable", "java.lang.CharSequence", "java.lang.Iterable", "java.lang.AutoCloseable", "java.lang.constant.Constable", "java.lang.constant.ConstantDesc"] {
        JavaClass::interface(i).register();
    }
    JavaClass::interface("java.util.Collection").implements(&["java.lang.Iterable"]).register();
    JavaClass::interface("java.util.List").implements(&["java.util.SequencedCollection", "java.util.Collection"]).register();
    JavaClass::interface("java.util.SequencedCollection").implements(&["java.util.Collection"]).register();
    JavaClass::interface("java.util.Set").implements(&["java.util.Collection"]).register();
    JavaClass::interface("java.util.Map").register();
    JavaClass::interface("java.util.Iterator").register();
    JavaClass::interface("java.util.Enumeration").register();
    JavaClass::interface("java.util.Comparator").register();
    JavaClass::new("java.lang.Number").modifiers(PUBLIC | ABSTRACT).implements(&["java.io.Serializable"]).constructor(Member::new("java.lang.Number", PUBLIC, &[], None)).register();
    JavaClass::new("java.lang.String")
        .modifiers(PUBLIC | FINAL)
        .implements(&["java.io.Serializable", "java.lang.Comparable", "java.lang.CharSequence", "java.lang.constant.Constable", "java.lang.constant.ConstantDesc"])
        .constructor(Member::new("java.lang.String", PUBLIC, &[], None))
        .field(Field::public_static("CASE_INSENSITIVE_ORDER", true, JType::class("java.util.Comparator"), || JValue::Null))
        .instantiable(|| Ok(JValue::str("")))
        .register();
    let int = |name: &str, v: i32| Field::public_static(name, true, JType::Int, move || JValue::Int(v));
    boxed("java.lang.Integer", "java.lang.Number", vec![int("MIN_VALUE", i32::MIN), int("MAX_VALUE", i32::MAX), class_field("TYPE", "int"), int("SIZE", 32), int("BYTES", 4)]).register();
    let long = |name: &str, v: i64| Field::public_static(name, true, JType::Long, move || JValue::Long(v));
    boxed("java.lang.Long", "java.lang.Number", vec![long("MIN_VALUE", i64::MIN), long("MAX_VALUE", i64::MAX), class_field("TYPE", "long"), int("SIZE", 64), int("BYTES", 8)]).register();
    let short = |name: &str, v: i16| Field::public_static(name, true, JType::Short, move || JValue::Short(v));
    boxed("java.lang.Short", "java.lang.Number", vec![short("MIN_VALUE", i16::MIN), short("MAX_VALUE", i16::MAX), class_field("TYPE", "short"), int("SIZE", 16), int("BYTES", 2)]).register();
    let byte = |name: &str, v: i8| Field::public_static(name, true, JType::Byte, move || JValue::Byte(v));
    boxed("java.lang.Byte", "java.lang.Number", vec![byte("MIN_VALUE", i8::MIN), byte("MAX_VALUE", i8::MAX), class_field("TYPE", "byte"), int("SIZE", 8), int("BYTES", 1)]).register();
    let double = |name: &str, v: f64| Field::public_static(name, true, JType::Double, move || JValue::Double(v));
    boxed(
        "java.lang.Double",
        "java.lang.Number",
        vec![
            double("POSITIVE_INFINITY", f64::INFINITY),
            double("NEGATIVE_INFINITY", f64::NEG_INFINITY),
            double("NaN", f64::NAN),
            double("MAX_VALUE", f64::MAX),
            double("MIN_NORMAL", f64::MIN_POSITIVE),
            double("MIN_VALUE", f64::from_bits(1)),
            int("MAX_EXPONENT", 1023),
            int("MIN_EXPONENT", -1022),
            int("SIZE", 64),
            int("PRECISION", 53),
            int("BYTES", 8),
            class_field("TYPE", "double"),
        ],
    )
    .register();
    let float = |name: &str, v: f32| Field::public_static(name, true, JType::Float, move || JValue::Float(v));
    boxed(
        "java.lang.Float",
        "java.lang.Number",
        vec![
            float("POSITIVE_INFINITY", f32::INFINITY),
            float("NEGATIVE_INFINITY", f32::NEG_INFINITY),
            float("NaN", f32::NAN),
            float("MAX_VALUE", f32::MAX),
            float("MIN_NORMAL", f32::MIN_POSITIVE),
            float("MIN_VALUE", f32::from_bits(1)),
            int("SIZE", 32),
            int("BYTES", 4),
            class_field("TYPE", "float"),
        ],
    )
    .register();
    let boolean = |name: &str, v: bool| Field::public_static(name, true, JType::class("java.lang.Boolean"), move || JValue::Bool(v));
    JavaClass::new("java.lang.Boolean")
        .modifiers(PUBLIC | FINAL)
        .implements(&["java.io.Serializable", "java.lang.Comparable", "java.lang.constant.Constable"])
        .field(boolean("TRUE", true))
        .field(boolean("FALSE", false))
        .field(class_field("TYPE", "boolean"))
        .register();
    let ch = |name: &str, v: char| Field::public_static(name, true, JType::Char, move || JValue::Char(v));
    JavaClass::new("java.lang.Character")
        .modifiers(PUBLIC | FINAL)
        .implements(&["java.io.Serializable", "java.lang.Comparable", "java.lang.constant.Constable"])
        .field(ch("MIN_VALUE", '\0'))
        .field(ch("MAX_VALUE", '\u{ffff}'))
        .field(class_field("TYPE", "char"))
        .register();
    JavaClass::new("java.math.BigDecimal").extends(Some("java.lang.Number")).implements(&["java.lang.Comparable"]).register();
    JavaClass::new("java.math.BigInteger").extends(Some("java.lang.Number")).implements(&["java.lang.Comparable"]).register();
    JavaClass::new("java.util.Locale").modifiers(PUBLIC | FINAL).implements(&["java.lang.Cloneable", "java.io.Serializable"]).register();
    JavaClass::new("java.util.Date").implements(&["java.io.Serializable", "java.lang.Cloneable", "java.lang.Comparable"]).instantiable(|| Ok(super::conversion::date_value(super::now_millis()))).register();
    JavaClass::new("java.util.Calendar").modifiers(PUBLIC | ABSTRACT).implements(&["java.io.Serializable", "java.lang.Cloneable", "java.lang.Comparable"]).register();
    JavaClass::new("java.util.GregorianCalendar").extends(Some("java.util.Calendar")).register();
    JavaClass::new("java.util.ArrayList").extends(Some("java.util.AbstractList")).implements(&["java.util.List", "java.util.RandomAccess", "java.lang.Cloneable", "java.io.Serializable"]).instantiable(|| Ok(JValue::List(java_compat::lang::JList::array_list(Vec::new())))).register();
    JavaClass::new("java.util.AbstractList").modifiers(PUBLIC | ABSTRACT).extends(Some("java.util.AbstractCollection")).implements(&["java.util.List"]).register();
    JavaClass::new("java.util.AbstractCollection").modifiers(PUBLIC | ABSTRACT).implements(&["java.util.Collection"]).register();
    JavaClass::new("java.util.HashMap").extends(Some("java.util.AbstractMap")).implements(&["java.util.Map", "java.lang.Cloneable", "java.io.Serializable"]).instantiable(|| Ok(JValue::Map(java_compat::lang::JMap::new(java_compat::lang::MapKind::Hash)))).register();
    JavaClass::new("java.util.AbstractMap").modifiers(PUBLIC | ABSTRACT).implements(&["java.util.Map"]).register();
    JavaClass::new("java.lang.Math").modifiers(PUBLIC | FINAL).field(double("E", std::f64::consts::E)).field(double("PI", std::f64::consts::PI)).field(double("TAU", std::f64::consts::TAU)).register();
    JavaClass::new("java.lang.Class").modifiers(PUBLIC | FINAL).implements(&["java.io.Serializable"]).register();
}
