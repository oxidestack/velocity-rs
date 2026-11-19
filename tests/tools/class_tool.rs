//! `org.apache.velocity.tools.generic.ClassToolTests`, with the test's nested classes as registered classes.

use std::sync::Arc;

use java_compat::reflect::JType;
use velocity::tools::class_tool::{ClassTool, INSPECT_KEY};
use velocity::tools::classes::{self, Annotation, JavaClass, Member, modifier::*};

use crate::base::{map, s};

const TESTS: &str = "org.apache.velocity.tools.generic.ClassToolTests";
const MY_ABSTRACT: &str = "org.apache.velocity.tools.generic.ClassToolTests$MyAbstract";
const MY_DEPRECATED: &str = "org.apache.velocity.tools.generic.ClassToolTests$MyDeprecated";
const PRIVATE_STRICT_STATIC: &str = "org.apache.velocity.tools.generic.ClassToolTests$PrivateStrictStatic";
const PROTECTED_NO_DEFAULT_CTOR: &str = "org.apache.velocity.tools.generic.ClassToolTests$ProtectedNoDefaultCtor";

/// The test class and its nested classes, as `javac` declares them.
fn fixtures() {
    JavaClass::new(TESTS).constructor(Member::new(TESTS, PUBLIC, &[], None)).register();
    JavaClass::new(MY_ABSTRACT).modifiers(PROTECTED | STATIC | ABSTRACT).constructor(Member::new(MY_ABSTRACT, PROTECTED, &[], None)).register();
    JavaClass::new(MY_DEPRECATED).modifiers(PROTECTED | STATIC).annotated(Annotation::deprecated()).constructor(Member::new(MY_DEPRECATED, PROTECTED, &[], None)).register();
    // `strictfp` is not a class modifier at run time.
    JavaClass::new(PRIVATE_STRICT_STATIC).modifiers(PRIVATE | STATIC).constructor(Member::new(PRIVATE_STRICT_STATIC, PRIVATE, &[], None)).register();
    JavaClass::new(PROTECTED_NO_DEFAULT_CTOR).modifiers(PROTECTED | STATIC).constructor(Member::new(PROTECTED_NO_DEFAULT_CTOR, PUBLIC, &[JType::string()], None)).register();
}

fn class(name: &str) -> Arc<JavaClass> {
    fixtures();
    classes::for_name(name).unwrap_or_else(|| panic!("{name} is registered"))
}

#[test]
fn ctor_class_tool() {
    ClassTool::new();
}

#[test]
fn ctor_class_tool_class_tool_class() {
    assert!(ClassTool::child(None, Some(class(velocity::tools::class_tool::CLASS))).is_err(), "Constructor 'ClassTool(null, Class)' worked but shouldn't have.");
    let parent = ClassTool::new();
    assert!(ClassTool::child(Some(&parent), None).is_err(), "Constructor 'ClassTool(ClassTool, null)' worked but shouldn't have.");
    assert!(ClassTool::child(Some(&parent), Some(class(TESTS))).is_ok());
}

#[test]
fn method_configure_map() {
    let class_tool = ClassTool::new();
    assert_eq!(class_tool.get_type().name, "java.lang.Object");
    class_tool.configure(&map(&[(INSPECT_KEY, s("java.util.Map"))])).unwrap();
    assert_eq!(class_tool.get_type().name, "java.util.Map");
}

#[test]
fn method_get_annotations() {
    let class_tool = ClassTool::new();
    assert!(class_tool.get_annotations().is_empty());
    class_tool.set_type(Some(class(MY_DEPRECATED))).unwrap();
    assert_eq!(class_tool.get_annotations().len(), 1);
    class_tool.set_type(Some(class(velocity::tools::values::CLASS))).unwrap();
    let mut annotations = class_tool.get_annotations();
    assert_eq!(annotations.len(), 3);
    annotations.sort_by_key(|a| a.java_string());
    assert_eq!(annotations[0].type_name, classes::DEFAULT_KEY);
    assert_eq!(annotations[1].type_name, classes::INVALID_SCOPE);
    assert_eq!(annotations[2].type_name, classes::SKIP_SETTERS);
}

#[test]
fn method_get_constructors() {
    assert!(!ClassTool::new().get_constructors().is_empty());
}

#[test]
fn method_get_fields() {
    assert!(ClassTool::new().get_fields().is_empty());
}

#[test]
fn method_get_methods() {
    assert!(!ClassTool::new().get_methods().is_empty());
}

#[test]
fn method_get_types() {
    assert!(!ClassTool::new().get_types().is_empty());
}

#[test]
fn method_get_full_name() {
    assert_eq!(ClassTool::new().get_full_name(), "java.lang.Object");
}

#[test]
fn method_get_name() {
    assert_eq!(ClassTool::new().get_name(), "Object");
}

#[test]
fn method_get_package() {
    assert_eq!(ClassTool::new().get_package(), "java.lang");
}

#[test]
fn method_get_type() {
    let class_tool = ClassTool::new();
    assert_eq!(class_tool.get_type().name, "java.lang.Object");
    class_tool.set_type(Some(class(velocity::tools::class_tool::CLASS))).unwrap();
    assert_eq!(class_tool.get_type().name, velocity::tools::class_tool::CLASS);
}

#[test]
fn method_get_super() {
    let class_tool = ClassTool::new();
    assert!(class_tool.get_super().is_none());
    class_tool.set_type(Some(class(velocity::tools::class_tool::CLASS))).unwrap();
    assert_eq!(class_tool.get_super().unwrap().get_type().name, velocity::tools::SAFE_CONFIG);
}

#[test]
fn method_inspect_class() {
    let class_tool = ClassTool::new();
    let result = class_tool.inspect_class(Some(class(velocity::tools::class_tool::CLASS))).unwrap();
    assert_eq!(result.get_type().name, velocity::tools::class_tool::CLASS);
}

#[test]
fn method_inspect_object() {
    let class_tool = ClassTool::new();
    let result = class_tool.inspect_object(&class_tool.value()).unwrap();
    assert_eq!(result.get_type().name, velocity::tools::class_tool::CLASS);
}

#[test]
fn method_inspect_string() {
    let class_tool = ClassTool::new();
    assert!(class_tool.inspect_named(None).unwrap().is_none());
    assert!(class_tool.inspect_named(Some("")).unwrap().is_none());
    assert!(class_tool.inspect_named(Some("bad")).unwrap().is_none());
    assert_eq!(class_tool.inspect_named(Some("java.util.Map")).unwrap().unwrap().get_type().name, "java.util.Map");
}

#[test]
fn method_is_abstract() {
    let class_tool = ClassTool::new();
    assert!(!class_tool.is_abstract());
    class_tool.set_type(Some(class(MY_ABSTRACT))).unwrap();
    assert!(class_tool.is_abstract());
}

#[test]
fn method_is_deprecated() {
    let class_tool = ClassTool::new();
    assert!(!class_tool.is_deprecated());
    class_tool.set_type(Some(class(MY_DEPRECATED))).unwrap();
    assert!(class_tool.is_deprecated());
}

#[test]
fn method_is_final() {
    let class_tool = ClassTool::new();
    assert!(!class_tool.is_final());
    class_tool.set_type(Some(class("java.lang.String"))).unwrap();
    assert!(class_tool.is_final());
}

#[test]
fn method_is_interface() {
    let class_tool = ClassTool::new();
    assert!(!class_tool.is_interface());
    class_tool.set_type(Some(class("java.util.Map"))).unwrap();
    assert!(class_tool.is_interface());
}

#[test]
fn method_is_private() {
    let class_tool = ClassTool::new();
    assert!(!class_tool.is_private());
    class_tool.set_type(Some(class(PRIVATE_STRICT_STATIC))).unwrap();
    assert!(class_tool.is_private());
}

#[test]
fn method_is_protected() {
    let class_tool = ClassTool::new();
    assert!(!class_tool.is_protected());
    class_tool.set_type(Some(class(PROTECTED_NO_DEFAULT_CTOR))).unwrap();
    assert!(class_tool.is_protected());
}

#[test]
fn method_is_public() {
    let class_tool = ClassTool::new();
    assert!(class_tool.is_public());
    class_tool.set_type(Some(class(PRIVATE_STRICT_STATIC))).unwrap();
    assert!(!class_tool.is_public());
}

#[test]
fn method_is_static() {
    let class_tool = ClassTool::new();
    assert!(!class_tool.is_static());
    class_tool.set_type(Some(class(PRIVATE_STRICT_STATIC))).unwrap();
    assert!(class_tool.is_static());
}

#[test]
fn method_set_class_class() {
    let class_tool = ClassTool::new();
    assert_eq!(class_tool.get_type().name, "java.lang.Object");
    class_tool.set_type(Some(class(velocity::tools::class_tool::CLASS))).unwrap();
    assert_eq!(class_tool.get_type().name, velocity::tools::class_tool::CLASS);
}

#[test]
fn method_supports_new_instance() {
    let class_tool = ClassTool::new();
    assert!(class_tool.supports_new_instance());
    class_tool.set_type(Some(class(PROTECTED_NO_DEFAULT_CTOR))).unwrap();
    assert!(!class_tool.supports_new_instance());
}
