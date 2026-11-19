//! `org.apache.velocity.tools.generic.ClassTool` (`$class`): a class's modifiers, annotations and declared
//! members, over the class registry ([`super::classes`]).

use std::sync::{Arc, Weak};

use java_compat::lang::{JArray, JError, JList, JMap, JObject, JValue, ListKind};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;

use super::classes::{self, Annotation, JavaClass, Member, modifier::*};
use super::safe::{self, SafeConfig, Tool};
use super::values::ValueParser;
use super::{c, o, s, tool_object, val};

/// The class name of `org.apache.velocity.tools.generic.ClassTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.ClassTool";
/// `ClassTool.INSPECT_KEY`: the configuration key naming the class to inspect.
pub const INSPECT_KEY: &str = "inspect";
/// `ClassTool.SHOW_DEPRECATED_KEY`: the configuration key for listing deprecated members too.
pub const SHOW_DEPRECATED_KEY: &str = "showDeprecated";

struct State {
    ty: Arc<JavaClass>,
    show_deprecated: bool,
}

/// Port of `org.apache.velocity.tools.generic.ClassTool` (`$class`): reflection over one class, its members filtered by
/// safe mode and the deprecated setting.
pub struct ClassTool {
    safe: SafeConfig,
    st: Mutex<State>,
    me: Weak<ClassTool>,
}

fn illegal(m: &str) -> JError {
    JError::new("java.lang.IllegalArgumentException", m)
}

impl ClassTool {
    /// `new ClassTool()`: inspecting `java.lang.Object`.
    pub fn new() -> Arc<ClassTool> {
        let object = classes::for_name(java_compat::reflect::OBJECT).expect("Object is registered");
        ClassTool::with(object, false)
    }

    fn with(ty: Arc<JavaClass>, show_deprecated: bool) -> Arc<ClassTool> {
        Arc::new_cyclic(|me| ClassTool { safe: SafeConfig::default(), st: Mutex::new(State { ty, show_deprecated }), me: me.clone() })
    }

    /// `new ClassTool(parent, type)`: `IllegalArgumentException` for a `null` type or parent.
    pub fn child(parent: Option<&ClassTool>, ty: Option<Arc<JavaClass>>) -> Result<Arc<ClassTool>, JError> {
        let ty = ty.ok_or_else(|| illegal("target type is null or invalid"))?;
        let parent = parent.ok_or_else(|| illegal("parent tool must not be null"))?;
        let t = ClassTool::with(ty, parent.get_show_deprecated());
        t.safe.set_safe_mode(parent.safe.is_safe_mode());
        t.safe.set_lock_config(parent.safe.is_config_locked());
        Ok(t)
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: reads `inspect` and `showDeprecated`; `IllegalArgumentException` for an unknown
    /// class to inspect.
    pub fn configure(&self, params: &JMap) -> Result<(), JError> {
        safe::try_configure(self, params)
    }

    /// `isSafeMode()`: whether only public classes and members are shown.
    pub fn is_safe_mode(&self) -> bool {
        self.safe.is_safe_mode()
    }

    /// `setType(type)`.
    pub fn set_type(&self, ty: Option<Arc<JavaClass>>) -> Result<(), JError> {
        self.st.lock().ty = ty.ok_or_else(|| illegal("target type is null or invalid"))?;
        Ok(())
    }

    /// `setType` by class name (for classes registered under that name).
    pub fn set_type_named(&self, name: &str) -> Result<(), JError> {
        self.set_type(classes::for_name(name))
    }

    /// `getShowDeprecated()`: whether deprecated members are listed, `false` unless configured.
    pub fn get_show_deprecated(&self) -> bool {
        self.st.lock().show_deprecated
    }

    /// `getType()`: the inspected class.
    pub fn get_type(&self) -> Arc<JavaClass> {
        self.st.lock().ty.clone()
    }

    /// `inspect(String name)`: `null` for no name or an unknown class.
    pub fn inspect_named(&self, name: Option<&str>) -> Result<Option<Arc<ClassTool>>, JError> {
        let Some(name) = name else { return Ok(None) };
        let class = classes::for_name(name);
        if class.is_none() {
            super::log::error(CLASS, &format!("Could not load Class for {name}"));
        }
        Ok(self.inspect_class(class))
    }

    /// `inspect(Object obj)`: its class.
    pub fn inspect_object(&self, obj: &JValue) -> Option<Arc<ClassTool>> {
        if obj.is_null() {
            return None;
        }
        self.inspect_class(classes::class_of(obj))
    }

    /// `inspect(Class type)`: `null` for a non-public class in safe mode.
    pub fn inspect_class(&self, ty: Option<Arc<JavaClass>>) -> Option<Arc<ClassTool>> {
        let tool = ClassTool::child(Some(self), Some(ty?)).ok()?;
        if self.is_safe_mode() && !tool.is_public() {
            return None;
        }
        Some(tool)
    }

    /// `getSuper()`.
    pub fn get_super(&self) -> Option<Arc<ClassTool>> {
        self.inspect_class(self.get_type().super_class())
    }

    /// `getPackage()`: the inspected class's package name.
    pub fn get_package(&self) -> String {
        self.get_type().package_name()
    }

    /// `getName()`: the inspected class's simple name.
    pub fn get_name(&self) -> String {
        self.get_type().simple_name()
    }

    /// `getFullName()`: the inspected class's fully qualified name.
    pub fn get_full_name(&self) -> String {
        self.get_type().name.clone()
    }

    /// `supportsNewInstance()`: whether `newInstance()` succeeds.
    pub fn supports_new_instance(&self) -> bool {
        self.get_type().new_instance().is_ok()
    }

    /// `isDeprecated()`: whether the inspected class carries `@Deprecated`.
    pub fn is_deprecated(&self) -> bool {
        self.get_type().annotation(classes::DEPRECATED).is_some()
    }

    fn has(&self, m: i32) -> bool {
        self.get_type().modifiers & m != 0
    }

    /// `isPublic()`: the class's `public` modifier.
    pub fn is_public(&self) -> bool {
        self.has(PUBLIC)
    }

    /// `isProtected()`: the class's `protected` modifier.
    pub fn is_protected(&self) -> bool {
        self.has(PROTECTED)
    }

    /// `isPrivate()`: the class's `private` modifier.
    pub fn is_private(&self) -> bool {
        self.has(PRIVATE)
    }

    /// `isStatic()`: the class's `static` modifier.
    pub fn is_static(&self) -> bool {
        self.has(STATIC)
    }

    /// `isFinal()`: the class's `final` modifier.
    pub fn is_final(&self) -> bool {
        self.has(FINAL)
    }

    /// `isInterface()`: whether the class is an interface.
    pub fn is_interface(&self) -> bool {
        self.has(INTERFACE)
    }

    /// `isStrict()`: the class's `strictfp` modifier.
    pub fn is_strict(&self) -> bool {
        self.has(STRICT)
    }

    /// `isAbstract()`: the class's `abstract` modifier.
    pub fn is_abstract(&self) -> bool {
        self.has(ABSTRACT)
    }

    fn keep(&self, sub: &Sub) -> bool {
        (!self.is_safe_mode() || sub.is_public()) && (self.get_show_deprecated() || !sub.is_deprecated())
    }

    fn subs(&self, kind: SubKind, members: impl Iterator<Item = Sub>) -> Vec<Sub> {
        let _ = kind;
        let mut subs: Vec<Sub> = members.filter(|s| self.keep(s)).collect();
        subs.sort_by(|a, b| super::xml::java_compare(&a.get_unique_name(), &b.get_unique_name()));
        subs
    }

    /// `getMethods()`: the declared methods, sorted by unique name.
    pub fn get_methods(&self) -> Vec<Sub> {
        let ty = self.get_type();
        self.subs(SubKind::Method, ty.methods.iter().map(|m| Sub::member(SubKind::Method, &ty, m)))
    }

    /// `getConstructors()`: the declared constructors, sorted by unique name.
    pub fn get_constructors(&self) -> Vec<Sub> {
        let ty = self.get_type();
        self.subs(SubKind::Constructor, ty.constructors.iter().map(|m| Sub::member(SubKind::Constructor, &ty, m)))
    }

    /// `getFields()`: the declared fields, sorted by name.
    pub fn get_fields(&self) -> Vec<Sub> {
        let ty = self.get_type();
        self.subs(
            SubKind::Field,
            ty.fields.iter().map(|f| Sub {
                kind: SubKind::Field,
                declaring: ty.name.clone(),
                name: f.name.clone(),
                modifiers: f.modifiers,
                params: Vec::new(),
                returns: Some(f.field_type.clone()),
                varargs: false,
                annotations: f.annotations.clone(),
                get: f.get.clone(),
            }),
        )
    }

    /// `getTypes()`: the non-primitive types the members use (array components rather than arrays).
    pub fn get_types(&self) -> Vec<JType> {
        let mut types: Vec<JType> = Vec::new();
        let mut add = |t: &JType| {
            let t = t.component().cloned().unwrap_or_else(|| t.clone());
            if !t.is_primitive() && !types.contains(&t) {
                types.push(t);
            }
        };
        let safe = self.is_safe_mode();
        for m in self.get_methods() {
            if !safe || m.is_public() {
                if let Some(r) = &m.returns {
                    add(r);
                }
                m.params.iter().for_each(&mut add);
            }
        }
        for c in self.get_constructors() {
            if !safe || c.is_public() {
                c.params.iter().for_each(&mut add);
            }
        }
        for f in self.get_fields() {
            if !safe || f.is_public() {
                if let Some(t) = &f.returns {
                    add(t);
                }
            }
        }
        types
    }

    /// `getAnnotations()`: the inspected class's annotations.
    pub fn get_annotations(&self) -> Vec<Annotation> {
        self.get_type().get_annotations()
    }

    fn java_string(&self) -> String {
        self.get_type().to_java_string()
    }
}

impl Tool for ClassTool {
    fn safe(&self) -> &SafeConfig {
        &self.safe
    }

    fn try_configure_values(&self, values: &ValueParser) -> Result<(), JError> {
        let sd = values.get_boolean_or(SHOW_DEPRECATED_KEY, self.get_show_deprecated());
        self.st.lock().show_deprecated = sd;
        if let Some(name) = values.get_string(INSPECT_KEY) {
            let class = classes::for_name(&name);
            if class.is_none() {
                super::log::error(CLASS, &format!("Could not load Class for {name}"));
            }
            self.set_type(class)?;
        }
        Ok(())
    }
}

/// Which kind of member a `Sub` describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubKind {
    /// A method (`MethodSub`).
    Method,
    /// A constructor (`ConstructorSub`).
    Constructor,
    /// A field (`FieldSub`).
    Field,
}

/// `ClassTool.MethodSub` / `ConstructorSub` / `FieldSub`.
#[derive(Clone)]
pub struct Sub {
    /// Which kind of member this is.
    pub kind: SubKind,
    declaring: String,
    name: String,
    modifiers: i32,
    /// The parameter types of a method or constructor, empty for a field.
    pub params: Vec<JType>,
    /// A method's return type (`None`: void), a field's type.
    pub returns: Option<JType>,
    varargs: bool,
    annotations: Vec<Annotation>,
    get: Option<Arc<dyn Fn() -> JValue + Send + Sync>>,
}

impl Sub {
    fn member(kind: SubKind, class: &JavaClass, m: &Member) -> Sub {
        Sub { kind, declaring: class.name.clone(), name: m.name.clone(), modifiers: m.modifiers, params: m.params.clone(), returns: m.returns.clone(), varargs: m.varargs, annotations: m.annotations.clone(), get: None }
    }

    /// `getName()`: a constructor is named after its class's simple name.
    pub fn get_name(&self) -> String {
        match self.kind {
            SubKind::Constructor => JType::class(&self.declaring).simple_name(),
            _ => self.name.clone(),
        }
    }

    /// `getUniqueName()`: the name, then `_` and the parameters' simple names (`Array`/`VarArgs` suffixes).
    pub fn get_unique_name(&self) -> String {
        if self.kind == SubKind::Field || self.params.is_empty() {
            return self.get_name();
        }
        let mut out = format!("{}_", self.get_name());
        for (i, p) in self.params.iter().enumerate() {
            match p.component() {
                Some(c) => {
                    out.push_str(&c.simple_name());
                    out.push_str(if i == self.params.len() - 1 && self.varargs { "VarArgs" } else { "Array" });
                }
                None => out.push_str(&p.simple_name()),
            }
        }
        out
    }

    fn signature(&self, full: bool) -> String {
        if self.params.is_empty() {
            return format!("{}()", self.get_name());
        }
        let parts: Vec<String> = self
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| match p.component() {
                Some(c) => format!("{}{}", if full { c.name() } else { c.simple_name() }, if i == self.params.len() - 1 && self.varargs { "..." } else { "[]" }),
                None => if full { p.name() } else { p.simple_name() },
            })
            .collect();
        format!("{}({})", self.get_name(), parts.join(","))
    }

    /// `getSignature()`: the name and the parameters' simple type names, as `name(int,String[])`.
    pub fn get_signature(&self) -> String {
        self.signature(false)
    }

    /// `getJavadocRef()`: the full signature (a field's name).
    pub fn get_javadoc_ref(&self) -> String {
        if self.kind == SubKind::Field { self.name.clone() } else { self.signature(true) }
    }

    /// `getParameterCount()`.
    pub fn get_parameter_count(&self) -> usize {
        self.params.len()
    }

    /// `isVarArgs()`: whether the last parameter is variadic.
    pub fn is_var_args(&self) -> bool {
        self.varargs
    }

    /// `isVoid()`: whether a method returns nothing (always true for a constructor).
    pub fn is_void(&self) -> bool {
        self.returns.is_none()
    }

    /// `MethodSub.getPropertyName()`: `getX`/`isX` without parameters, `setX` with one.
    pub fn get_property_name(&self) -> Option<String> {
        let n = &self.name;
        let uncap = |s: &str| -> String {
            let u: Vec<u16> = s.encode_utf16().collect();
            if u.len() > 1 { String::from_utf16_lossy(&u[..1]).to_lowercase() + &String::from_utf16_lossy(&u[1..]) } else { s.to_lowercase() }
        };
        match self.params.len() {
            0 if n.starts_with("get") && n.len() > 3 => Some(uncap(&n[3..])),
            0 if n.starts_with("is") && n.len() > 2 => Some(uncap(&n[2..])),
            1 if n.starts_with("set") && n.len() > 3 => Some(uncap(&n[3..])),
            _ => None,
        }
    }

    /// `FieldSub.getStaticValue()`.
    pub fn get_static_value(&self) -> JValue {
        if self.is_static() { self.get.as_ref().map(|g| g()).unwrap_or(JValue::Null) } else { JValue::Null }
    }

    /// `getAnnotations()`: the member's annotations.
    pub fn get_annotations(&self) -> Vec<Annotation> {
        self.annotations.clone()
    }

    /// `isDeprecated()`: whether the member carries `@Deprecated`.
    pub fn is_deprecated(&self) -> bool {
        self.annotations.iter().any(|a| a.type_name == classes::DEPRECATED)
    }

    fn has(&self, m: i32) -> bool {
        self.modifiers & m != 0
    }

    /// `isPublic()`: the member's `public` modifier.
    pub fn is_public(&self) -> bool {
        self.has(PUBLIC)
    }

    /// `isStatic()`: the member's `static` modifier.
    pub fn is_static(&self) -> bool {
        self.has(STATIC)
    }

    /// `toString()`: `method name(params)`, `field name`.
    pub fn java_string(&self) -> String {
        let kind = match self.kind {
            SubKind::Method => "method",
            SubKind::Constructor => "constructor",
            SubKind::Field => "field",
        };
        format!("{kind} {}", self.get_javadoc_ref())
    }
}

fn classes_value(types: &[JType]) -> JValue {
    JValue::Array(Arc::new(JArray::new(c("java.lang.Class"), types.iter().map(|t| classes::class_value(&t.name())).collect())))
}

/// An annotation as a value: `annotationType()` and `toString()`.
pub struct AnnotationObject(pub Annotation);

fn annotation_def() -> Arc<ClassDef> {
    type A = AnnotationObject;
    ClassDef::new("java.lang.annotation.Annotation")
        .interface()
        .method::<A>("annotationType", &[], Some(c("java.lang.Class")), |a, _, _| val(classes::class_value(&a.0.type_name)))
        .method::<A>("toString", &[], Some(s()), |a, _, _| val(JValue::str(a.0.java_string())))
        .register()
}

impl JObject for AnnotationObject {
    fn class_name(&self) -> String {
        format!("jdk.proxy2.$Proxy({})", self.0.type_name)
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<super::Ret> {
        None
    }

    fn to_java_string(&self) -> String {
        self.0.java_string()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<ClassDef>> {
        static DEF: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(annotation_def);
        Some(DEF.get())
    }
}

fn annotations_value(a: Vec<Annotation>) -> JValue {
    JValue::List(JList::new(ListKind::Fixed, a.into_iter().map(|a| JValue::Object(Arc::new(AnnotationObject(a)) as Arc<dyn JObject>)).collect()))
}

fn sub_def() -> Arc<ClassDef> {
    type S = Sub;
    let b = || JType::Boolean;
    ClassDef::new("org.apache.velocity.tools.generic.ClassTool$Sub")
        .method::<S>("getName", &[], Some(s()), |t, _, _| val(JValue::str(t.get_name())))
        .method::<S>("getUniqueName", &[], Some(s()), |t, _, _| val(JValue::str(t.get_unique_name())))
        .method::<S>("getJavadocRef", &[], Some(s()), |t, _, _| val(JValue::str(t.get_javadoc_ref())))
        .method::<S>("getSignature", &[], Some(s()), |t, _, _| val(JValue::str(t.get_signature())))
        .method::<S>("getParameters", &[], Some(JType::array(c("java.lang.Class"))), |t, _, _| val(classes_value(&t.params)))
        .method::<S>("getParameterCount", &[], Some(JType::Int), |t, _, _| val(JValue::Int(t.get_parameter_count() as i32)))
        .method::<S>("takesParameters", &[], Some(b()), |t, _, _| val(JValue::Bool(t.get_parameter_count() > 0)))
        .method::<S>("isVarArgs", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_var_args())))
        .method::<S>("isVoid", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_void())))
        .method::<S>("getReturns", &[], Some(c("java.lang.Class")), |t, _, _| val(classes::class_value(&t.returns.as_ref().map(|r| r.name()).unwrap_or_else(|| "void".into()))))
        .method::<S>("getType", &[], Some(c("java.lang.Class")), |t, _, _| val(t.returns.as_ref().map(|r| classes::class_value(&r.name())).unwrap_or(JValue::Null)))
        .method::<S>("getPropertyName", &[], Some(s()), |t, _, _| super::text(t.get_property_name()))
        .method::<S>("getStaticValue", &[], Some(o()), |t, _, _| val(t.get_static_value()))
        .method::<S>("getAnnotations", &[], Some(c("java.util.List")), |t, _, _| val(annotations_value(t.get_annotations())))
        .method::<S>("isDeprecated", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_deprecated())))
        .method::<S>("isPublic", &[], Some(b()), |t, _, _| val(JValue::Bool(t.has(PUBLIC))))
        .method::<S>("isProtected", &[], Some(b()), |t, _, _| val(JValue::Bool(t.has(PROTECTED))))
        .method::<S>("isPrivate", &[], Some(b()), |t, _, _| val(JValue::Bool(t.has(PRIVATE))))
        .method::<S>("isStatic", &[], Some(b()), |t, _, _| val(JValue::Bool(t.has(STATIC))))
        .method::<S>("isFinal", &[], Some(b()), |t, _, _| val(JValue::Bool(t.has(FINAL))))
        .method::<S>("isInterface", &[], Some(b()), |t, _, _| val(JValue::Bool(t.has(INTERFACE))))
        .method::<S>("isNative", &[], Some(b()), |t, _, _| val(JValue::Bool(t.has(NATIVE))))
        .method::<S>("isStrict", &[], Some(b()), |t, _, _| val(JValue::Bool(t.has(STRICT))))
        .method::<S>("isSynchronized", &[], Some(b()), |t, _, _| val(JValue::Bool(t.has(SYNCHRONIZED))))
        .method::<S>("isTransient", &[], Some(b()), |t, _, _| val(JValue::Bool(t.has(TRANSIENT))))
        .method::<S>("isVolatile", &[], Some(b()), |t, _, _| val(JValue::Bool(t.has(VOLATILE))))
        .method::<S>("isAbstract", &[], Some(b()), |t, _, _| val(JValue::Bool(t.has(ABSTRACT))))
        .method::<S>("toString", &[], Some(s()), |t, _, _| val(JValue::str(t.java_string())))
        .register()
}

impl JObject for Sub {
    fn class_name(&self) -> String {
        match self.kind {
            SubKind::Method => "org.apache.velocity.tools.generic.ClassTool$MethodSub",
            SubKind::Constructor => "org.apache.velocity.tools.generic.ClassTool$ConstructorSub",
            SubKind::Field => "org.apache.velocity.tools.generic.ClassTool$FieldSub",
        }
        .into()
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<super::Ret> {
        None
    }

    fn to_java_string(&self) -> String {
        self.java_string()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<ClassDef>> {
        static DEF: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(sub_def);
        Some(DEF.get())
    }
}

fn subs_value(subs: Vec<Sub>) -> JValue {
    JValue::List(JList::new(ListKind::Fixed, subs.into_iter().map(|s| JValue::Object(Arc::new(s) as Arc<dyn JObject>)).collect()))
}

fn opt_tool(t: Option<Arc<ClassTool>>) -> super::Ret {
    val(t.map(|t| t.value()).unwrap_or(JValue::Null))
}

fn def() -> Arc<ClassDef> {
    type T = ClassTool;
    let me = || c(CLASS);
    let b = || JType::Boolean;
    let def = ClassDef::new(CLASS)
        .extends(&[safe::SAFE_CONFIG])
        .method::<T>("getShowDeprecated", &[], Some(b()), |t, _, _| val(JValue::Bool(t.get_show_deprecated())))
        .method::<T>("getType", &[], Some(c("java.lang.Class")), |t, _, _| val(t.get_type().value()))
        .method::<T>("inspect", &[s()], Some(me()), |t, _, a| opt_tool(t.inspect_named(a[0].as_str())?))
        .method::<T>("inspect", &[o()], Some(me()), |t, _, a| opt_tool(t.inspect_object(&a[0])))
        .method::<T>("getSuper", &[], Some(me()), |t, _, _| opt_tool(t.get_super()))
        .method::<T>("inspect", &[c("java.lang.Class")], Some(me()), |t, _, a| opt_tool(t.inspect_class(classes::as_class(&a[0]))))
        .method::<T>("getPackage", &[], Some(s()), |t, _, _| val(JValue::str(t.get_package())))
        .method::<T>("getName", &[], Some(s()), |t, _, _| val(JValue::str(t.get_name())))
        .method::<T>("getFullName", &[], Some(s()), |t, _, _| val(JValue::str(t.get_full_name())))
        .method::<T>("supportsNewInstance", &[], Some(b()), |t, _, _| val(JValue::Bool(t.supports_new_instance())))
        .method::<T>("isDeprecated", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_deprecated())))
        .method::<T>("isPublic", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_public())))
        .method::<T>("isProtected", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_protected())))
        .method::<T>("isPrivate", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_private())))
        .method::<T>("isStatic", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_static())))
        .method::<T>("isFinal", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_final())))
        .method::<T>("isInterface", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_interface())))
        .method::<T>("isStrict", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_strict())))
        .method::<T>("isAbstract", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_abstract())))
        .method::<T>("getMethods", &[], Some(c("java.util.List")), |t, _, _| val(subs_value(t.get_methods())))
        .method::<T>("getConstructors", &[], Some(c("java.util.List")), |t, _, _| val(subs_value(t.get_constructors())))
        .method::<T>("getFields", &[], Some(c("java.util.List")), |t, _, _| val(subs_value(t.get_fields())))
        .method::<T>("getTypes", &[], Some(c("java.util.Set")), |t, _, _| {
            val(JValue::List(JList::new(ListKind::Set, t.get_types().iter().map(|ty| classes::class_value(&ty.name())).collect())))
        })
        .method::<T>("getAnnotations", &[], Some(c("java.util.List")), |t, _, _| val(annotations_value(t.get_annotations())))
        .method::<T>("toString", &[], Some(s()), |t, _, _| val(JValue::str(t.java_string())));
    super::object_methods::<T>(safe::safe_config_methods::<T>(def)).register()
}

tool_object!(ClassTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || ClassTool::new().value())
        .annotated(Annotation::default_key("class"))
        .annotated(Annotation::valid_scope(&[super::scope::APPLICATION]))
        .constructor(Member::new(CLASS, PROTECTED, &[c(CLASS), c("java.lang.Class")], None))
}
