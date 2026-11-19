//! `BaseTestCase`'s engine, context and assertions, and a way to write the suite's small Java fixture classes.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use java_compat::lang::{JError, JObject, JValue};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;
use velocity::props::{ExtProperties, PropValue};
use velocity::{Context, Engine, VelocityError};

/// A Java number by class, for tests that build boxed numbers of every width.
pub fn byte(v: i8) -> JValue {
    JValue::Byte(v)
}

pub fn short(v: i16) -> JValue {
    JValue::Short(v)
}

pub fn big_integer(v: i64) -> JValue {
    JValue::BigInteger(v.to_string().into())
}

pub fn big_decimal(s: &str) -> JValue {
    JValue::BigDecimal(s.into())
}

/// `BigDecimal.valueOf(double)`.
pub fn big_decimal_of(d: f64) -> JValue {
    JValue::BigDecimal(java_compat::double_to_string(d).into())
}

pub fn s(v: &str) -> JValue {
    JValue::str(v)
}

/// `BaseTestCase`: a file + string resource engine (repository `string.repo` kept in the application attributes),
/// created on first use, after the test has set its properties (`setUpEngine`).
pub struct Base {
    /// `log`: the engine's `runtime.log.instance`, writing to stderr.
    pub log: Arc<crate::test_logger::TestLogger>,
    pub props: ExtProperties,
    engine: Option<Engine>,
    pub context: Context,
    pub string_repo_name: String,
}

pub const STRING_REPO: &str = "string.repo";

impl Default for Base {
    fn default() -> Base {
        Base::new()
    }
}

impl Base {
    pub fn new() -> Base {
        let log = Arc::new(crate::test_logger::TestLogger::new(false, false));
        let mut props = ExtProperties::new();
        let instance: Arc<dyn velocity::log::Logger> = log.clone();
        props.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
        props.set_property("resource.loaders", "file,string".into());
        props.add_property("string.resource.loader.class", velocity::resource::STRING_LOADER.into());
        props.add_property("string.resource.loader.repository.name", STRING_REPO.into());
        props.add_property("string.resource.loader.repository.static", "false".into());
        Base { log, props, engine: None, context: Context::new(), string_repo_name: STRING_REPO.into() }
    }

    /// `engine.setProperty(key, value)` before the engine initialises.
    pub fn set_property(&mut self, key: &str, value: impl Into<PropValue>) {
        assert!(self.engine.is_none(), "set properties before the engine is used");
        self.props.set_property(key, value.into());
    }

    pub fn add_property(&mut self, key: &str, value: impl Into<PropValue>) {
        assert!(self.engine.is_none(), "set properties before the engine is used");
        self.props.add_property(key, value.into());
    }

    pub fn engine_mut(&mut self) -> &mut Engine {
        self.engine();
        self.engine.as_mut().expect("initialised")
    }

    pub fn engine(&mut self) -> &Engine {
        if self.engine.is_none() {
            self.engine = Some(Engine::with_properties(&self.props).expect("engine initialises"));
        }
        self.engine.as_ref().expect("initialised")
    }

    pub fn put(&mut self, key: &str, value: JValue) {
        self.context.put(key, value);
    }

    /// The log tag `evaluate` uses: the template, or its first 15 characters.
    fn tag(vtl: &str) -> String {
        let units: Vec<u16> = vtl.encode_utf16().collect();
        if units.len() <= 15 { vtl.to_string() } else { String::from_utf16_lossy(&units[..15]) }
    }

    pub fn evaluate(&mut self, vtl: &str) -> Result<String, VelocityError> {
        let tag = Base::tag(vtl);
        self.engine();
        let engine = self.engine.as_ref().expect("initialised");
        engine.evaluate(&mut self.context, &tag, vtl)
    }

    #[track_caller]
    pub fn assert_eval_equals(&mut self, expected: &str, vtl: &str) {
        match self.evaluate(vtl) {
            Ok(out) => assert_eq!(out, expected, "template {vtl:?}"),
            Err(e) => panic!("template {vtl:?} threw {} ({})", e.class_name(), e.message()),
        }
    }

    #[track_caller]
    pub fn assert_schmoo(&mut self, template: &str) {
        self.assert_eval_equals(template, template);
    }

    /// `assertEvalException(evil, exceptionType)`: the exception, which must be an instance of `class` if given.
    #[track_caller]
    pub fn assert_eval_exception(&mut self, evil: &str, class: Option<&str>) -> VelocityError {
        // The expected failure is not logged.
        self.log.off();
        let result = self.evaluate(evil);
        self.log.on();
        match result {
            Ok(out) => panic!("Template '{evil}' should have thrown an exception, rendered {out:?}"),
            Err(e) => {
                if let Some(c) = class {
                    assert!(e.is_a(c), "Was expecting template '{evil}' to throw {c} not {} ({})", e.class_name(), e.message());
                }
                e
            }
        }
    }

    /// `assertEvalExceptionAt(evil, template, line, col)`: the message holds `template[line L, column C]`, not at its
    /// start.
    #[track_caller]
    pub fn assert_eval_exception_at(&mut self, evil: &str, template: &str, line: i32, col: i32) -> VelocityError {
        let loc = format!("{template}[line {line}, column {col}]");
        let e = self.assert_eval_exception(evil, None);
        let m = e.message();
        assert!(m.find(&loc).is_some_and(|i| i >= 1), "Was expecting exception at {loc} instead of {m}");
        e
    }

    /// The string repository (`getStringRepository`).
    pub fn repository(&mut self) -> Arc<velocity::resource::StringResourceRepository> {
        let name = self.string_repo_name.clone();
        self.engine().string_repository(&name).expect("the string loader keeps its repository")
    }

    pub fn add_template(&mut self, name: &str, vtl: &str) {
        self.repository().put(name, vtl);
    }

    pub fn remove_template(&mut self, name: &str) {
        self.repository().remove(name);
    }

    #[track_caller]
    pub fn assert_tmpl_equals(&mut self, expected: &str, template: &str) {
        self.engine();
        let engine = self.engine.as_ref().expect("initialised");
        match engine.merge_template(template, Some("utf-8"), &mut self.context) {
            Ok(out) => assert_eq!(out, expected, "template {template}"),
            Err(e) => panic!("template {template} threw {} ({})", e.class_name(), e.message()),
        }
    }

    #[track_caller]
    pub fn assert_context_value(&mut self, key: &str, expected: &JValue) {
        let v = self.context.get(key).unwrap_or(JValue::Null);
        assert!(java_compat::lang::java_equals(&v, expected), "{key}: expected {expected:?}, found {v:?}");
    }
}

type Method = Arc<dyn Fn(&Bean, &[JValue]) -> Result<Option<JValue>, JError> + Send + Sync>;

/// An instance of one of the suite's Java fixture classes: its class definition and mutable fields.
pub struct Bean {
    pub class: Arc<ClassDef>,
    pub fields: Mutex<HashMap<String, JValue>>,
    to_string: Option<Method>,
}

impl Bean {
    pub fn field(&self, name: &str) -> JValue {
        self.fields.lock().get(name).cloned().unwrap_or(JValue::Null)
    }

    pub fn set_field(&self, name: &str, v: JValue) {
        self.fields.lock().insert(name.to_string(), v);
    }
}

impl JObject for Bean {
    fn class_name(&self) -> String {
        self.class.name.to_string()
    }

    fn invoke(&self, _name: &str, _args: &[JValue]) -> Option<Result<Option<JValue>, JError>> {
        None
    }

    fn to_java_string(&self) -> String {
        match &self.to_string {
            Some(f) => f(self, &[]).ok().flatten().map(|v| v.to_java_string()).unwrap_or_else(|| "null".into()),
            None => java_compat::reflect::identity_string(self),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<ClassDef>> {
        Some(self.class.clone())
    }
}

/// Declares a fixture class: `class("IfNullTestCase$NullToString").method("getAsString", &[], Some(string()), |b, a| …)`.
pub struct BeanClass {
    def: ClassDef,
    to_string: Option<Method>,
}

pub fn class(name: &str) -> BeanClass {
    BeanClass { def: ClassDef::new(name), to_string: None }
}

pub fn object() -> JType {
    JType::object()
}

pub fn string() -> JType {
    JType::string()
}

impl BeanClass {
    pub fn extends(mut self, supers: &[&str]) -> BeanClass {
        self.def = self.def.extends(supers);
        self
    }

    pub fn implements(mut self, interfaces: &[&str]) -> BeanClass {
        self.def = self.def.implements(interfaces);
        self
    }

    pub fn interface(mut self) -> BeanClass {
        self.def = self.def.interface();
        self
    }

    pub fn enum_of(mut self, constants: &[&str]) -> BeanClass {
        self.def = self.def.enum_of(constants);
        self
    }

    pub fn deprecated(mut self) -> BeanClass {
        self.def = self.def.deprecated();
        self
    }

    pub fn deprecated_method(mut self, name: &str) -> BeanClass {
        self.def = self.def.deprecated_method(name);
        self
    }

    pub fn non_public(mut self) -> BeanClass {
        self.def = self.def.non_public();
        self
    }

    pub fn method(
        mut self,
        name: &str,
        params: &[JType],
        returns: Option<JType>,
        f: impl Fn(&Bean, &[JValue]) -> Result<Option<JValue>, JError> + Send + Sync + 'static,
    ) -> BeanClass {
        let f: Method = Arc::new(f);
        if name == "toString" && params.is_empty() {
            self.to_string = Some(f.clone());
        }
        self.def = self.def.method::<Bean>(name, params, returns, move |b, _, a| f(b, a));
        self
    }

    /// A method declared by a supertype (`declaring`), as inherited.
    pub fn method_of(
        mut self,
        declaring: &str,
        name: &str,
        params: &[JType],
        returns: Option<JType>,
        f: impl Fn(&Bean, &[JValue]) -> Result<Option<JValue>, JError> + Send + Sync + 'static,
    ) -> BeanClass {
        self.def = self.def.method_of::<Bean>(declaring, name, params, returns, move |b, _, a| f(b, a));
        self
    }

    /// A `public static` method.
    pub fn static_method(mut self, name: &str, params: &[JType], returns: Option<JType>, f: impl Fn(&[JValue]) -> Result<Option<JValue>, JError> + Send + Sync + 'static) -> BeanClass {
        let declaring = self.def.name.clone();
        self.def.methods.push(java_compat::reflect::JMethod {
            name: name.into(),
            params: params.to_vec(),
            returns,
            declaring_class: declaring,
            is_static: true,
            varargs: false,
            invoke: Arc::new(move |_, a| f(a)),
        });
        self
    }

    /// Registers the class and returns a constructor for fresh instances.
    pub fn build(self) -> BeanFactory {
        let has_to_string = self.to_string.is_some();
        let mut def = self.def;
        // `java.lang.Object`'s methods, after the class's own.
        if !has_to_string {
            def = def.method_of::<Bean>("java.lang.Object", "toString", &[], Some(JType::string()), |b, _, _| Ok(Some(JValue::str(b.to_java_string()))));
        }
        def = def
            .method_of::<Bean>("java.lang.Object", "hashCode", &[], Some(JType::Int), |b, _, _| Ok(Some(JValue::Int((b as *const Bean as usize) as u32 as i32))))
            .method_of::<Bean>("java.lang.Object", "equals", &[JType::object()], Some(JType::Boolean), |b, _, a| {
                Ok(Some(JValue::Bool(matches!(&a[0], JValue::Object(o) if std::ptr::addr_eq(Arc::as_ptr(o), b as *const Bean)))))
            });
        BeanFactory { class: def.register(), to_string: self.to_string }
    }
}

#[derive(Clone)]
pub struct BeanFactory {
    pub class: Arc<ClassDef>,
    to_string: Option<Method>,
}

impl BeanFactory {
    /// `Foo.class`.
    pub fn class_object(&self) -> JValue {
        velocity::JClass::of(self.class.clone())
    }

    pub fn new_instance(&self) -> JValue {
        self.with_fields(&[])
    }

    pub fn with_fields(&self, fields: &[(&str, JValue)]) -> JValue {
        let b = Bean { class: self.class.clone(), fields: Mutex::new(fields.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()), to_string: self.to_string.clone() };
        JValue::Object(Arc::new(b))
    }
}

/// `new Object()`.
pub fn new_object() -> JValue {
    static OBJECT: std::sync::LazyLock<BeanFactory> = std::sync::LazyLock::new(|| class("java.lang.Object").build());
    OBJECT.new_instance()
}

/// Java `null` / `String` helpers for `Ok(Some(...))` results.
pub fn ret(v: JValue) -> Result<Option<JValue>, JError> {
    Ok(Some(v))
}

/// `TEST_COMPARE_DIR`: the suite's resources (Maven copies them to the test classes directory).
pub fn resources() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("upstream/velocity-engine-core/src/test/resources")
}

pub fn resource(path: &str) -> String {
    resources().join(path).to_string_lossy().into_owned()
}

/// `normalizeNewlines`.
pub fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

pub fn file_contents(path: &str) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    String::from_utf8_lossy(&bytes).into_owned()
}

impl Base {
    /// `getTemplate(name).merge(context, writer)`.
    pub fn merge(&mut self, name: &str) -> Result<String, VelocityError> {
        self.engine();
        let engine = self.engine.as_ref().expect("initialised");
        let t = engine.get_template(name, None)?;
        engine.merge(&t, &mut self.context)
    }

    /// `checkTemplate` / `testFile`: merges `name` and compares with `compare_dir/<base>.cmp`, newlines normalised.
    #[track_caller]
    pub fn assert_matches_compare(&mut self, name: &str, compare_file: &str) {
        let out = match self.merge(name) {
            Ok(o) => o,
            Err(e) => panic!("{name} threw {} ({})", e.class_name(), e.message()),
        };
        let expected = file_contents(compare_file);
        assert_eq!(normalize_newlines(&out), normalize_newlines(&expected), "Output incorrect for {name}");
    }
}

/// `BaseTestCase.getFileName(dir, base, ext)`: `base` under `dir` (canonical, when there is a `dir`), then `.ext`.
pub fn get_file_name(dir: Option<&str>, base: &str, ext: &str) -> String {
    let mut buf = match dir {
        Some(dir) => {
            let base_path = std::path::Path::new(base);
            let file = if base_path.is_absolute() {
                base_path.to_path_buf()
            } else if dir.is_empty() {
                // `new File("", child)` resolves against the file system root.
                std::path::Path::new("/").join(base)
            } else {
                std::path::Path::new(dir).join(base)
            };
            let file = if file.is_absolute() { file } else { std::env::current_dir().expect("a working directory").join(file) };
            canonical(&file)
        }
        None => base.to_string(),
    };
    if !ext.is_empty() {
        buf.push('.');
        buf.push_str(ext);
    }
    buf
}

/// `File.getCanonicalPath()` for a path that may not exist: `.` and `..` resolved, no trailing separator.
fn canonical(path: &std::path::Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in path.components() {
        match c {
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::Normal(p) => parts.push(p.to_string_lossy().into_owned()),
            _ => {}
        }
    }
    format!("/{}", parts.join("/"))
}
