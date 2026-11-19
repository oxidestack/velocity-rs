//! What the tools see of Java classes through reflection (`ClassUtils`, `ClassTool`, `FieldTool`, the toolbox
//! configuration): classes by name, their modifiers, annotations, declared members, public static fields and
//! public no-argument constructors.
//!
//! A class is known by being registered: the JDK classes the tools reason about and every tool are built in;
//! applications (and tests, for their fixture classes) register their own. `for_name` is `Class.forName`.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use java_compat::lang::{JError, JObject, JValue};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::RwLock;

/// `java.lang.reflect.Modifier` bits.
pub mod modifier {
    /// `Modifier.PUBLIC`.
    pub const PUBLIC: i32 = 0x1;
    /// `Modifier.PRIVATE`.
    pub const PRIVATE: i32 = 0x2;
    /// `Modifier.PROTECTED`.
    pub const PROTECTED: i32 = 0x4;
    /// `Modifier.STATIC`.
    pub const STATIC: i32 = 0x8;
    /// `Modifier.FINAL`.
    pub const FINAL: i32 = 0x10;
    /// `Modifier.SYNCHRONIZED`.
    pub const SYNCHRONIZED: i32 = 0x20;
    /// `Modifier.VOLATILE`.
    pub const VOLATILE: i32 = 0x40;
    /// `Modifier.TRANSIENT`.
    pub const TRANSIENT: i32 = 0x80;
    /// `Modifier.NATIVE`.
    pub const NATIVE: i32 = 0x100;
    /// `Modifier.INTERFACE`.
    pub const INTERFACE: i32 = 0x200;
    /// `Modifier.ABSTRACT`.
    pub const ABSTRACT: i32 = 0x400;
    /// `Modifier.STRICT`.
    pub const STRICT: i32 = 0x800;
}

use modifier::*;

/// An annotation instance: its type and its members' values (`value` alone for the single-member ones).
#[derive(Clone, Debug)]
pub struct Annotation {
    /// The annotation type's fully qualified name.
    pub type_name: String,
    /// The members' values in declaration order, as (member name, value).
    pub values: Vec<(String, AnnotationValue)>,
}

/// The value of an annotation member: the member types the tools' annotations use.
#[derive(Clone, Debug)]
pub enum AnnotationValue {
    /// A `String` member.
    Str(String),
    /// A `String[]` member.
    Strings(Vec<String>),
    /// A `boolean` member.
    Bool(bool),
}

impl AnnotationValue {
    fn render(&self) -> String {
        let quote = |s: &str| format!("\"{}\"", java_compat::escape_java(s));
        match self {
            AnnotationValue::Str(s) => quote(s),
            AnnotationValue::Strings(v) => format!("{{{}}}", v.iter().map(|s| quote(s)).collect::<Vec<_>>().join(", ")),
            AnnotationValue::Bool(b) => b.to_string(),
        }
    }
}

/// The `@DefaultKey` annotation type: the default context key of a tool.
pub const DEFAULT_KEY: &str = "org.apache.velocity.tools.config.DefaultKey";
/// The `@ValidScope` annotation type: the scopes a tool may be configured in.
pub const VALID_SCOPE: &str = "org.apache.velocity.tools.config.ValidScope";
/// The `@InvalidScope` annotation type: the scopes a tool may not be configured in.
pub const INVALID_SCOPE: &str = "org.apache.velocity.tools.config.InvalidScope";
/// The `@SkipSetters` annotation type: the tool's setters are not called with configuration properties.
pub const SKIP_SETTERS: &str = "org.apache.velocity.tools.config.SkipSetters";
/// The `@Deprecated` annotation type.
pub const DEPRECATED: &str = "java.lang.Deprecated";
/// The annotation types declared `@Inherited`.
const INHERITED: [&str; 4] = [DEFAULT_KEY, VALID_SCOPE, INVALID_SCOPE, SKIP_SETTERS];

impl Annotation {
    /// An annotation of the named type with no member values.
    pub fn new(type_name: &str) -> Annotation {
        Annotation { type_name: type_name.into(), values: Vec::new() }
    }

    /// Adds the member `name` with `value`, builder-style.
    pub fn with(mut self, name: &str, value: AnnotationValue) -> Annotation {
        self.values.push((name.into(), value));
        self
    }

    /// `@DefaultKey(key)`.
    pub fn default_key(key: &str) -> Annotation {
        Annotation::new(DEFAULT_KEY).with("value", AnnotationValue::Str(key.into()))
    }

    /// `@ValidScope(scopes)`.
    pub fn valid_scope(scopes: &[&str]) -> Annotation {
        Annotation::new(VALID_SCOPE).with("value", AnnotationValue::Strings(scopes.iter().map(|s| s.to_string()).collect()))
    }

    /// `@InvalidScope(scopes)`.
    pub fn invalid_scope(scopes: &[&str]) -> Annotation {
        Annotation::new(INVALID_SCOPE).with("value", AnnotationValue::Strings(scopes.iter().map(|s| s.to_string()).collect()))
    }

    /// `@SkipSetters`.
    pub fn skip_setters() -> Annotation {
        Annotation::new(SKIP_SETTERS)
    }

    /// `@Deprecated` (with its default members).
    pub fn deprecated() -> Annotation {
        Annotation::new(DEPRECATED).with("forRemoval", AnnotationValue::Bool(false)).with("since", AnnotationValue::Str(String::new()))
    }

    /// The value of member `name`, `None` when the annotation has no such member.
    pub fn value(&self, name: &str) -> Option<&AnnotationValue> {
        self.values.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// `Annotation.toString()` as JDK 26 prints it: a lone `value` member without its name.
    pub fn java_string(&self) -> String {
        let members = match self.values.as_slice() {
            [(n, v)] if n == "value" => v.render(),
            vs => vs.iter().map(|(n, v)| format!("{n}={}", v.render())).collect::<Vec<_>>().join(", "),
        };
        format!("@{}({members})", self.type_name)
    }
}

/// Invokes a static member (or a constructor): the arguments, the result (`None` for `void`).
pub type StaticInvoker = Arc<dyn Fn(&[JValue]) -> Result<Option<JValue>, JError> + Send + Sync>;

/// A declared method or constructor.
#[derive(Clone)]
pub struct Member {
    /// The member's name; a constructor's is its class name.
    pub name: String,
    /// The `Modifier` bits, as `getModifiers()` returns them.
    pub modifiers: i32,
    /// The parameter types, in order.
    pub params: Vec<JType>,
    /// `None` for `void` (and constructors).
    pub returns: Option<JType>,
    /// Whether the last parameter is a varargs array (`isVarArgs()`).
    pub varargs: bool,
    /// The annotations declared on the member.
    pub annotations: Vec<Annotation>,
    /// For static methods and constructors that can be called.
    pub invoke: Option<StaticInvoker>,
}

impl Member {
    /// A member with these name, modifiers, parameter types and return type, not varargs, unannotated and not invocable.
    pub fn new(name: &str, modifiers: i32, params: &[JType], returns: Option<JType>) -> Member {
        Member { name: name.into(), modifiers, params: params.to_vec(), returns, varargs: false, annotations: Vec::new(), invoke: None }
    }

    /// Marks the member as varargs, builder-style.
    pub fn varargs(mut self) -> Member {
        self.varargs = true;
        self
    }

    /// Adds an annotation, builder-style.
    pub fn annotated(mut self, a: Annotation) -> Member {
        self.annotations.push(a);
        self
    }

    /// Makes the member invocable through `f`, builder-style.
    pub fn invoker(mut self, f: impl Fn(&[JValue]) -> Result<Option<JValue>, JError> + Send + Sync + 'static) -> Member {
        self.invoke = Some(Arc::new(f));
        self
    }

    /// Whether the `static` modifier is set.
    pub fn is_static(&self) -> bool {
        self.modifiers & STATIC != 0
    }
}

/// A declared field; static ones can be read.
#[derive(Clone)]
pub struct Field {
    /// The field's name.
    pub name: String,
    /// The `Modifier` bits, as `getModifiers()` returns them.
    pub modifiers: i32,
    /// The declared type of the field.
    pub field_type: JType,
    /// The annotations declared on the field.
    pub annotations: Vec<Annotation>,
    /// Reads the current value of a static field; `None` when the field cannot be read.
    pub get: Option<Arc<dyn Fn() -> JValue + Send + Sync>>,
}

impl Field {
    /// A `public static` field whose current value `get` reads.
    pub fn public_static(name: &str, final_: bool, field_type: JType, get: impl Fn() -> JValue + Send + Sync + 'static) -> Field {
        Field { name: name.into(), modifiers: PUBLIC | STATIC | if final_ { FINAL } else { 0 }, field_type, annotations: Vec::new(), get: Some(Arc::new(get)) }
    }

    /// A field with these name, modifiers and type, unannotated and unreadable.
    pub fn new(name: &str, modifiers: i32, field_type: JType) -> Field {
        Field { name: name.into(), modifiers, field_type, annotations: Vec::new(), get: None }
    }

    /// Whether the `static` modifier is set.
    pub fn is_static(&self) -> bool {
        self.modifiers & STATIC != 0
    }
}

/// Creates an instance through the public no-argument constructor.
pub type Constructor = Arc<dyn Fn() -> Result<JValue, JError> + Send + Sync>;

/// A Java class as reflection sees it.
#[derive(Clone)]
pub struct JavaClass {
    /// The fully qualified class name (`getName()`, `$` for nested classes).
    pub name: String,
    /// The `Modifier` bits, as `getModifiers()` returns them.
    pub modifiers: i32,
    /// `getSuperclass()`: `None` for `Object`, interfaces and primitives.
    pub superclass: Option<String>,
    /// `getInterfaces()`: the directly implemented interfaces' names.
    pub interfaces: Vec<String>,
    /// The annotations declared on the class itself.
    pub annotations: Vec<Annotation>,
    /// `getDeclaredConstructors()`.
    pub constructors: Vec<Member>,
    /// `getDeclaredMethods()`.
    pub methods: Vec<Member>,
    /// `getDeclaredFields()`.
    pub fields: Vec<Field>,
    /// `newInstance()` when the class has an accessible no-argument constructor and is instantiable.
    pub new_instance: Option<Constructor>,
    /// The public API templates call, for classes implemented in Rust.
    pub def: Option<Arc<ClassDef>>,
}

impl std::fmt::Debug for JavaClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_java_string())
    }
}

impl JavaClass {
    /// A public class extending `Object`, with no interfaces, annotations, members or constructor.
    pub fn new(name: &str) -> JavaClass {
        JavaClass {
            name: name.into(),
            modifiers: PUBLIC,
            superclass: Some(java_compat::reflect::OBJECT.into()),
            interfaces: Vec::new(),
            annotations: Vec::new(),
            constructors: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            new_instance: None,
            def: None,
        }
    }

    /// A public interface.
    pub fn interface(name: &str) -> JavaClass {
        JavaClass { modifiers: PUBLIC | INTERFACE | ABSTRACT, superclass: None, ..JavaClass::new(name) }
    }

    /// A tool class: its public API (methods declared by it become its declared methods), its superclass and
    /// interfaces, and a public no-argument constructor.
    pub fn tool(def: Arc<ClassDef>, new: impl Fn() -> JValue + Send + Sync + 'static) -> JavaClass {
        let mut c = JavaClass::new(&def.name);
        c.superclass = def.superclasses.first().map(|s| s.to_string()).or_else(|| Some(java_compat::reflect::OBJECT.into()));
        c.interfaces = def.interfaces.iter().map(|s| s.to_string()).collect();
        c.methods = declared_of(&def);
        c.constructors = vec![Member::new(&def.name, PUBLIC, &[], None)];
        c.new_instance = Some(Arc::new(move || Ok(new())));
        c.def = Some(def);
        c
    }

    /// Replaces the `Modifier` bits, builder-style.
    pub fn modifiers(mut self, m: i32) -> JavaClass {
        self.modifiers = m;
        self
    }

    /// Sets the superclass, `None` for none, builder-style.
    pub fn extends(mut self, superclass: Option<&str>) -> JavaClass {
        self.superclass = superclass.map(str::to_string);
        self
    }

    /// Replaces the implemented interfaces, builder-style.
    pub fn implements(mut self, interfaces: &[&str]) -> JavaClass {
        self.interfaces = interfaces.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Adds a class annotation, builder-style.
    pub fn annotated(mut self, a: Annotation) -> JavaClass {
        self.annotations.push(a);
        self
    }

    /// Adds a declared constructor, builder-style.
    pub fn constructor(mut self, m: Member) -> JavaClass {
        self.constructors.push(m);
        self
    }

    /// Adds a declared method, builder-style.
    pub fn method(mut self, m: Member) -> JavaClass {
        self.methods.push(m);
        self
    }

    /// Adds a declared field, builder-style.
    pub fn field(mut self, f: Field) -> JavaClass {
        self.fields.push(f);
        self
    }

    /// Makes the class instantiable through `new` (`newInstance()`), builder-style.
    pub fn instantiable(mut self, new: impl Fn() -> Result<JValue, JError> + Send + Sync + 'static) -> JavaClass {
        self.new_instance = Some(Arc::new(new));
        self
    }

    /// Attaches the Rust-implemented public API templates call, builder-style.
    pub fn with_def(mut self, def: Arc<ClassDef>) -> JavaClass {
        self.def = Some(def);
        self
    }

    /// Registers the class under its name, replacing any earlier registration.
    pub fn register(self) -> Arc<JavaClass> {
        let c = Arc::new(self);
        REGISTRY.write().insert(c.name.clone(), c.clone());
        c
    }

    /// `isInterface()`: whether the `interface` modifier is set.
    pub fn is_interface(&self) -> bool {
        self.modifiers & INTERFACE != 0
    }

    /// Whether the `public` modifier is set.
    pub fn is_public(&self) -> bool {
        self.modifiers & PUBLIC != 0
    }

    /// The class as a reflection type (`JType::class` of its name).
    pub fn jtype(&self) -> JType {
        JType::class(&self.name)
    }

    /// `getSimpleName()`.
    pub fn simple_name(&self) -> String {
        self.jtype().simple_name()
    }

    /// `getPackage().getName()`.
    pub fn package_name(&self) -> String {
        let outer = self.name.split('$').next().unwrap_or(&self.name);
        outer.rfind('.').map(|i| outer[..i].to_string()).unwrap_or_default()
    }

    /// `Class.toString()`: `class x.Y` or `interface x.Y`.
    pub fn to_java_string(&self) -> String {
        format!("{} {}", if self.is_interface() { "interface" } else { "class" }, self.name)
    }

    /// `getAnnotation(type)`: its own, else an `@Inherited` one of a superclass.
    pub fn annotation(&self, type_name: &str) -> Option<Annotation> {
        self.get_annotations().into_iter().find(|a| a.type_name == type_name)
    }

    /// `getAnnotations()`: its own, then the `@Inherited` ones of superclasses it does not declare.
    pub fn get_annotations(&self) -> Vec<Annotation> {
        let mut out = self.annotations.clone();
        let mut sup = self.super_class();
        while let Some(c) = sup {
            for a in &c.annotations {
                if INHERITED.contains(&a.type_name.as_str()) && !out.iter().any(|o| o.type_name == a.type_name) {
                    out.push(a.clone());
                }
            }
            sup = c.super_class();
        }
        out
    }

    /// `getSuperclass()` resolved.
    pub fn super_class(&self) -> Option<Arc<JavaClass>> {
        self.superclass.as_deref().and_then(for_name)
    }

    /// `newInstance()`: `InstantiationException` when there is no usable no-argument constructor.
    pub fn new_instance(&self) -> Result<JValue, JError> {
        if self.modifiers & (ABSTRACT | INTERFACE) != 0 {
            return Err(JError::without_message("java.lang.InstantiationException"));
        }
        match &self.new_instance {
            Some(f) => f(),
            None => Err(JError::new("java.lang.InstantiationException", self.name.clone())),
        }
    }

    /// `getFields()`: the public fields of the class, its superclasses and interfaces.
    pub fn public_fields(&self) -> Vec<Field> {
        let mut out: Vec<Field> = self.fields.iter().filter(|f| f.modifiers & PUBLIC != 0).cloned().collect();
        for i in &self.interfaces {
            if let Some(c) = for_name(i) {
                out.extend(c.public_fields());
            }
        }
        if let Some(s) = self.super_class() {
            out.extend(s.public_fields());
        }
        out
    }

    /// `getField(name)`.
    pub fn get_field(&self, name: &str) -> Result<Field, JError> {
        self.public_fields().into_iter().find(|f| f.name == name).ok_or_else(|| JError::new("java.lang.NoSuchFieldException", name))
    }

    /// `getMethod(name)` for a no-argument method, public, declared here or inherited.
    pub fn get_method(&self, name: &str, params: &[JType]) -> Option<Member> {
        if let Some(m) = self.methods.iter().find(|m| m.name == name && m.params == params && m.modifiers & PUBLIC != 0) {
            return Some(m.clone());
        }
        if let Some(def) = &self.def {
            if let Some(m) = def.methods.iter().find(|m| &*m.name == name && m.params == params) {
                return Some(Member::new(name, PUBLIC | if m.is_static { STATIC } else { 0 }, params, m.returns.clone()));
            }
        }
        self.super_class().and_then(|s| s.get_method(name, params))
    }

    /// `isAssignableFrom(other)`.
    pub fn is_assignable_from(&self, other: &str) -> bool {
        other == self.name || JType::class(&self.name).is_assignable_from(&JType::class(other)) || {
            let Some(c) = for_name(other) else { return false };
            c.superclass.as_deref().is_some_and(|s| self.is_assignable_from(s)) || c.interfaces.iter().any(|i| self.is_assignable_from(i))
        }
    }

    /// A `java.lang.Class` value for templates.
    pub fn value(&self) -> JValue {
        class_value(&self.name)
    }
}

/// The public methods a class definition declares itself.
fn declared_of(def: &ClassDef) -> Vec<Member> {
    def.methods
        .iter()
        .filter(|m| *m.declaring_class == *def.name)
        .map(|m| Member { name: m.name.to_string(), modifiers: PUBLIC | if m.is_static { STATIC } else { 0 }, params: m.params.clone(), returns: m.returns.clone(), varargs: m.varargs, annotations: Vec::new(), invoke: None })
        .collect()
}

static REGISTRY: LazyLock<RwLock<HashMap<String, Arc<JavaClass>>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

static BUILTINS: LazyLock<()> = LazyLock::new(|| {
    super::jdk::register();
    super::register_tool_classes();
});

/// `ClassUtils.getClass(name)` / `Class.forName`: `None` when no such class is known.
pub fn for_name(name: &str) -> Option<Arc<JavaClass>> {
    LazyLock::force(&BUILTINS);
    REGISTRY.read().get(name).cloned()
}

/// `ClassUtils.getClass(name)`, throwing like it.
pub fn get_class(name: &str) -> Result<Arc<JavaClass>, JError> {
    for_name(name).ok_or_else(|| JError::new("java.lang.ClassNotFoundException", name))
}

/// The registered class of a value (`obj.getClass()`), falling back to what its class definition says.
pub fn class_of(v: &JValue) -> Option<Arc<JavaClass>> {
    let name = java_compat::reflect::class_of(v)?.name();
    if let Some(c) = for_name(&name) {
        return Some(c);
    }
    let mut c = JavaClass::new(&name);
    if let Some(def) = java_compat::reflect::class_def(v) {
        c.superclass = def.superclasses.first().map(|s| s.to_string()).or(c.superclass);
        c.interfaces = def.interfaces.iter().map(|s| s.to_string()).collect();
        c.methods = declared_of(&def);
        if !def.public {
            c.modifiers = 0;
        }
        c.def = Some(def);
    }
    Some(Arc::new(c))
}

/// A `java.lang.Class` object as templates see it (`SecureIntrospectorImpl` lets only `getName` through).
pub fn class_value(name: &str) -> JValue {
    JValue::Object(Arc::new(crate::introspect::JClass {
        name: name.to_string(),
        def: java_compat::reflect::class_named(name),
        ty: Some(java_compat::reflect::JType::class(name)),
    }) as Arc<dyn JObject>)
}

/// The class a `java.lang.Class` value names.
pub fn as_class(v: &JValue) -> Option<Arc<JavaClass>> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<crate::introspect::JClass>().and_then(|c| for_name(&c.name).or_else(|| Some(Arc::new(JavaClass::new(&c.name))))),
        _ => None,
    }
}

/// `ClassUtils.getFieldValue(fieldPath)`: `a.b.C.FIELD`, static fields only.
pub fn field_value(path: &str) -> Result<JValue, JError> {
    let dot = path.rfind('.').ok_or_else(|| JError::new("java.lang.StringIndexOutOfBoundsException", "begin 0, end -1"))?;
    let class = get_class(&path[..dot])?;
    static_field_value(&class, &path[dot + 1..])
}

/// `ClassUtils.getFieldValue(clazz, fieldname)`.
pub fn static_field_value(class: &JavaClass, name: &str) -> Result<JValue, JError> {
    let field = class.get_field(name)?;
    if !field.is_static() {
        return Err(JError::new("java.lang.UnsupportedOperationException", format!("Field {name} in class {} is not static.  Only static fields are supported.", class.name)));
    }
    Ok(field.get.map(|g| g()).unwrap_or(JValue::Null))
}

/// `ClassUtils.findFactoryMethod(factory, target)`: `create<Target>()`, `new<Target>()` or `get<Target>()`.
pub fn find_factory_method(factory: &JavaClass, target: &JavaClass) -> Option<Member> {
    let simple = target.simple_name();
    ["create", "new", "get"].iter().find_map(|p| find_method(factory, &format!("{p}{simple}"), &[]))
}

/// `ClassUtils.findMethod(clazz, name, params)`: public or declared, up the hierarchy.
pub fn find_method(class: &JavaClass, name: &str, params: &[JType]) -> Option<Member> {
    if let Some(m) = class.get_method(name, params) {
        return Some(m);
    }
    let mut c = Some(Arc::new(class.clone()));
    while let Some(k) = c {
        if let Some(m) = k.methods.iter().find(|m| m.name == name && m.params == params) {
            return Some(m.clone());
        }
        c = k.super_class();
    }
    None
}
