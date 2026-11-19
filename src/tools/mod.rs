//! The velocity-tools 3.1 generic tools, as typed classes the uberspect resolves overloads on, the toolbox
//! machinery that configures and creates them (`ToolManager`, `ToolContext`, the configuration classes), and
//! the `ToolContext` MockServer's `ToolManager` creates for every render ([`context`]).
//!
//! Each tool is a Rust struct with the Java class's public API as Rust methods (for callers in Rust) and a
//! registered [`ClassDef`] (for templates); the configuration state they share lives in [`safe`].

pub mod alternator;
mod bean;
pub mod class_tool;
pub mod classes;
pub mod classpath;
pub mod collection;
pub mod configuration;
pub mod conversion;
pub mod conversion_tool;
pub mod date;
pub mod display;
pub mod escape;
pub mod field;
pub mod import;
pub mod iter;
mod jdk;
pub mod json;
pub mod link;
pub mod log;
pub mod loop_tool;
pub mod markup;
pub mod misc;
pub mod math_tool;
pub mod num;
pub mod number;
pub mod resource_tool;
pub mod safe;
pub mod scope;
pub mod toolbox;
pub mod uri;
pub mod values;
pub mod xml;
pub mod xml_tool;

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use java_compat::lang::{JError, JObject, JValue};
use java_compat::reflect::{ClassDef, JType};
use java_compat::text::{Locale, LocaleData};

pub use safe::{FORMAT_CONFIG, LOCALE_CONFIG, SAFE_CONFIG};

pub(crate) type Ret = Result<Option<JValue>, JError>;

pub(crate) fn o() -> JType {
    JType::object()
}

pub(crate) fn s() -> JType {
    JType::string()
}

pub(crate) fn c(name: &str) -> JType {
    JType::class(name)
}

pub(crate) fn val(v: JValue) -> Ret {
    Ok(Some(v))
}

/// A `String` result, `null` for `None`.
pub(crate) fn text(v: Option<String>) -> Ret {
    Ok(Some(v.map(JValue::str).unwrap_or(JValue::Null)))
}

/// `String.valueOf(obj)`, `None` for `null`.
pub(crate) fn string_of(v: &JValue) -> Option<String> {
    (!v.is_null()).then(|| v.to_java_string())
}

pub(crate) fn npe() -> JError {
    JError::without_message("java.lang.NullPointerException")
}

static LOCALE_DATA: AtomicU8 = AtomicU8::new(0);

/// Which locale data the JVM has: a full JDK (the default), or only `java.base`'s (MockServer's trimmed runtime,
/// where every language but English formats with root data). Locales the tools and the uberspect create from
/// strings resolve from it.
pub fn set_locale_data(data: LocaleData) {
    LOCALE_DATA.store(if data == LocaleData::JavaBase { 1 } else { 0 }, Ordering::SeqCst);
}

/// Which locale data the JVM has, as set by `set_locale_data`: `LocaleData::Full` unless set to `java.base`'s.
pub fn locale_data() -> LocaleData {
    if LOCALE_DATA.load(Ordering::SeqCst) == 1 { LocaleData::JavaBase } else { LocaleData::Full }
}

/// `Locale.getDefault()`: `en`.
pub fn default_locale() -> Locale {
    Locale::default_locale().with_data(locale_data())
}

/// A locale in the JVM's locale data.
pub fn locale(language: &str, country: &str, variant: &str) -> Locale {
    Locale::new(language, country, variant).with_data(locale_data())
}

/// `new Object()`.
pub fn jdk_object() -> jdk::PlainObject {
    jdk::PlainObject
}

/// `System.currentTimeMillis()`.
pub fn now_millis() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// `Math.random()`: a uniformly distributed double in `[0, 1)`.
pub fn random() -> f64 {
    use std::hash::{BuildHasher, Hasher};
    static STATE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(STATE.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed));
    h.write_i64(now_millis());
    (h.finish() >> 11) as f64 / (1u64 << 53) as f64
}

/// `java.lang.Object`'s public methods other than `getClass` (which the introspector handles itself).
pub(crate) fn object_methods<T: JObject + 'static>(def: ClassDef) -> ClassDef {
    def.method_of::<T>("java.lang.Object", "toString", &[], Some(s()), |t, _, _| val(JValue::str(t.to_java_string())))
        .method_of::<T>("java.lang.Object", "hashCode", &[], Some(JType::Int), |t, _, _| val(JValue::Int((t as *const T as usize) as u32 as i32)))
        .method_of::<T>("java.lang.Object", "equals", &[o()], Some(JType::Boolean), |_, this, a| val(JValue::Bool(crate::render::same_value(this, &a[0]))))
}

/// Implements [`JObject`] for a tool whose class definition `$def()` builds.
macro_rules! tool_object {
    ($ty:ty, $def:expr) => {
        impl java_compat::lang::JObject for $ty {
            fn class_name(&self) -> String {
                java_compat::lang::JObject::class_def(self).map(|d| d.name.to_string()).unwrap_or_default()
            }

            fn invoke(&self, _name: &str, _args: &[java_compat::lang::JValue]) -> Option<Result<Option<java_compat::lang::JValue>, java_compat::lang::JError>> {
                None
            }

            fn to_java_string(&self) -> String {
                self.java_string()
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn class_def(&self) -> Option<std::sync::Arc<java_compat::reflect::ClassDef>> {
                static DEF: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new($def);
                Some(DEF.get())
            }
        }
    };
}
pub(crate) use tool_object;

/// `Object.toString()` of a tool: `class@hash`.
pub(crate) fn identity(o: &dyn JObject) -> String {
    java_compat::reflect::identity_string(o)
}

/// Registers every tool class (and the configuration base classes) with [`classes`].
pub(crate) fn register_tool_classes() {
    use classes::modifier::PUBLIC;
    use classes::{JavaClass, Member};
    let map = || c("java.util.Map");
    JavaClass::new(SAFE_CONFIG)
        .constructor(Member::new(SAFE_CONFIG, PUBLIC, &[], None))
        .method(Member::new("configure", PUBLIC, &[map()], None))
        .method(Member::new("isConfigLocked", PUBLIC, &[], Some(JType::Boolean)))
        .method(Member::new("isSafeMode", PUBLIC, &[], Some(JType::Boolean)))
        .register();
    JavaClass::new(LOCALE_CONFIG).extends(Some(SAFE_CONFIG)).method(Member::new("getLocale", PUBLIC, &[], Some(c("java.util.Locale")))).register();
    JavaClass::new(FORMAT_CONFIG).extends(Some(LOCALE_CONFIG)).method(Member::new("getFormat", PUBLIC, &[], Some(s()))).register();
    math_tool::class().register();
    number::class().register();
    xml_tool::class().register();
    date::date_tool_class().register();
    date::comparison_date_tool_class().register();
    display::class().register();
    collection::class().register();
    collection::sort_tool_class().register();
    escape::class().register();
    json::class().register();
    alternator::class().register();
    class_tool::class().register();
    field::class().register();
    loop_tool::class().register();
    values::class().register();
    markup::class().register();
    link::class().register();
    resource_tool::class().register();
    conversion_tool::class().register();
    misc::context_tool_class().register();
    misc::render_tool_class().register();
    misc::log_tool_class().register();
    misc::import_tool_class().register();
    JavaClass::new(import::IMPORT_SUPPORT)
        .extends(Some(SAFE_CONFIG))
        .annotated(classes::Annotation::invalid_scope(&[scope::APPLICATION, scope::SESSION, scope::REQUEST]))
        .instantiable(|| Ok(JValue::Null))
        .register();
}

/// MockServer 8.0.0's `ToolManager`: `CollectionTool`, `ComparisonDateTool`, `DisplayTool`, `EscapeTool`,
/// `MathTool` and `NumberTool` in the application scope (shared), `JsonTool` and `XmlTool` per request.
pub fn mockserver_tool_manager() -> &'static toolbox::ToolManager {
    static MANAGER: std::sync::LazyLock<toolbox::ToolManager> = std::sync::LazyLock::new(|| {
        use configuration::{FactoryConfiguration, ToolConfiguration, ToolboxConfiguration};
        let toolbox = |scope: &str, classes: &[&str]| {
            let mut t = ToolboxConfiguration::new();
            t.set_scope(scope);
            for c in classes {
                let mut tool = ToolConfiguration::new();
                tool.set_classname(Some(c));
                t.add_tool(tool);
            }
            t
        };
        let mut config = FactoryConfiguration::for_class(configuration::XML_CONFIGURATION, "");
        config.add_toolbox(toolbox(scope::APPLICATION, &[collection::CLASS, date::COMPARISON_DATE_TOOL, display::CLASS, escape::CLASS, math_tool::CLASS, number::CLASS]));
        config.add_toolbox(toolbox(scope::REQUEST, &[json::CLASS, xml_tool::CLASS]));
        let manager = toolbox::ToolManager::new(false, false).expect("no auto-configuration");
        manager.configure(&config).expect("MockServer's tools configure");
        manager
    });
    &MANAGER
}

/// A fresh `ToolContext` of MockServer's tool manager (`tools.createContext()`), to layer under the variables.
pub fn context() -> Arc<toolbox::ToolContext> {
    mockserver_tool_manager().create_context(None)
}
