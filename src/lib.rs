//! A port of Apache Velocity Engine 2.4.1, faithful to its rendering output: the same template and the same
//! variables render to the same bytes, Velocity's error messages included. Values are Java values from
//! [`java_compat::lang`], introspected the way `UberspectImpl` introspects them. [`tools`] ports the generic tools of
//! Velocity Tools 3.1.
//!
//! An [`Engine`] (`VelocityEngine`) renders a template against a [`Context`] (`VelocityContext`):
//!
//! ```
//! use java_compat::lang::JValue;
//! use velocity::{Context, Engine};
//!
//! # fn main() -> Result<(), velocity::VelocityError> {
//! let engine = Engine::mockserver();
//! let mut context = Context::new();
//! context.put("name", JValue::str("world"));
//! let out = engine.render("Hello, $name! #foreach($i in [1..3])$i#if($foreach.hasNext), #end#end", &mut context)?;
//! assert_eq!(out, "Hello, world! 1, 2, 3");
//! # Ok(())
//! # }
//! ```
//!
//! [`Engine::new`] is Velocity's default configuration; [`Engine::mockserver`] configures the engine as MockServer
//! does: `space.gobbling = lines`, hyphens allowed in identifiers, `$foreach` as the only scope,
//! `SecureUberspector`, a string resource loader, the custom `#ifnull` directive, the tool manager with the generic
//! toolbox and the render deadline. [`Engine::with_properties`] takes `velocity.properties` overrides, and
//! [`Engine::evaluate`] renders a template string as `Velocity.evaluate` does:
//!
//! ```
//! use velocity::{Context, Engine};
//!
//! let engine = Engine::new();
//! let error = engine.evaluate(&mut Context::new(), "example", "#if(").unwrap_err();
//! assert!(error.is_parse());
//! ```

#![warn(missing_docs)]

mod ast;
pub mod deadline;
pub mod directive;
pub mod error;
pub mod events;
pub mod field_methodizer;
pub mod handlers;
mod init;
pub mod io;
mod introspect;
mod jj;
mod jj_scan;
mod lexer;
pub mod log;
pub mod math;
mod parser;
pub mod range;
pub mod config;
pub mod props;
mod render;
pub mod resource;
mod scope;
pub mod tools;
pub mod uberspect;
pub mod uberspector;
pub mod vm;

use std::collections::HashMap;
use std::sync::Arc;

use java_compat::lang::JValue;
use parking_lot::Mutex;

pub use ast::Block;
pub use introspect::JClass;
pub use parser::unescape as string_literal_unescape;

/// How many parse failures lost their expected-token list because the replay of [`jj`] disagreed with the parser
/// (tests assert it stays zero).
#[doc(hidden)]
pub fn jj_fallbacks() -> usize {
    parser::JJ_FALLBACKS.load(std::sync::atomic::Ordering::Relaxed)
}
pub use error::{ErrorKind, Owner, StopTarget, VelocityError, VelocityErrorData};

/// Lookups a context falls back to (`VelocityContext`'s inner context chain).
pub trait Layer: Send + Sync {
    /// The value of `key` in this layer, `None` when the layer does not have it.
    fn get(&self, key: &str) -> Option<JValue>;

    /// The layer as `Any` when it is one a caller may look into (a chained [`Context`]); `None` by default.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }
}

impl Layer for Context {
    fn get(&self, key: &str) -> Option<JValue> {
        Context::get(self, key)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

impl Layer for HashMap<String, JValue> {
    fn get(&self, key: &str) -> Option<JValue> {
        HashMap::get(self, key).cloned()
    }
}

/// A `VelocityContext`: its own variables over read-only inner layers. `#set` and `#foreach` write (and remove)
/// only the own variables; a variable set to `null` is present with a null value.
#[derive(Default, Clone)]
pub struct Context {
    vars: HashMap<String, JValue>,
    inner: Vec<Arc<dyn Layer>>,
    cartridge: Option<Arc<events::EventCartridge>>,
    /// The introspection cache (`icacheGet` / `icachePut`), and how often a lookup found something in it.
    icache: HashMap<uberspector::MethodCacheKey, uberspector::IntrospectionCacheData>,
    icache_hits: usize,
}

impl Context {
    /// `new VelocityContext()`: an empty context.
    pub fn new() -> Context {
        Context::default()
    }

    /// `new VelocityContext(inner)`: reads fall through to `inner` (as it is now), writes stay here, and `inner`'s
    /// event cartridge carries over.
    pub fn wrap(inner: &Context) -> Context {
        Context { vars: HashMap::new(), inner: vec![Arc::new(inner.clone())], cartridge: inner.cartridge.clone(), icache: HashMap::new(), icache_hits: 0 }
    }

    /// Adds a layer below the ones already there.
    pub fn with_layer(mut self, layer: Arc<dyn Layer>) -> Context {
        self.inner.push(layer);
        self
    }

    /// `get(key)`: the context's own variable, else the first inner layer that has it.
    pub fn get(&self, key: &str) -> Option<JValue> {
        if let Some(v) = self.vars.get(key) {
            return Some(v.clone());
        }
        self.inner.iter().find_map(|l| l.get(key))
    }

    /// `containsKey(key)`, the inner layers included.
    pub fn contains_key(&self, key: &str) -> bool {
        self.vars.contains_key(key) || self.inner.iter().any(|l| l.get(key).is_some())
    }

    /// `put(key, value)`: sets the context's own variable; a null value is present as null.
    pub fn put(&mut self, key: &str, value: JValue) {
        self.vars.insert(key.to_string(), value);
    }

    /// `remove(key)`: removes the context's own variable; the inner layers are read-only.
    pub fn remove(&mut self, key: &str) -> Option<JValue> {
        self.vars.remove(key)
    }

    /// `icacheGet(key)`.
    pub fn icache_get(&mut self, key: &uberspector::MethodCacheKey) -> Option<uberspector::IntrospectionCacheData> {
        let found = self.icache.get(key).cloned();
        if found.is_some() {
            self.icache_hits += 1;
        }
        found
    }

    /// `icachePut(key, data)`.
    pub fn icache_put(&mut self, key: uberspector::MethodCacheKey, data: uberspector::IntrospectionCacheData) {
        self.icache.insert(key, data);
    }

    /// How many `icacheGet` calls found an entry.
    pub fn icache_hits(&self) -> usize {
        self.icache_hits
    }

    /// `EventCartridge.attachToContext`.
    pub fn attach_event_cartridge(&mut self, cartridge: Arc<events::EventCartridge>) {
        self.cartridge = Some(cartridge);
    }

    /// The event cartridge attached to the context, if any.
    pub fn event_cartridge(&self) -> Option<&events::EventCartridge> {
        self.cartridge.as_deref()
    }

    pub(crate) fn cartridge_arc(&self) -> Option<Arc<events::EventCartridge>> {
        self.cartridge.clone()
    }

    /// Every key, the chained contexts' included (`getKeys()`).
    pub fn all_keys(&self) -> Vec<String> {
        let mut keys = self.keys();
        for layer in &self.inner {
            if let Some(c) = layer.as_any().and_then(|a| a.downcast_ref::<Context>()) {
                for k in c.all_keys() {
                    if !keys.contains(&k) {
                        keys.push(k);
                    }
                }
            }
        }
        keys
    }

    /// The context's own keys (`getKeys()` of the outermost context).
    pub fn keys(&self) -> Vec<String> {
        self.vars.keys().cloned().collect()
    }
}

/// A processed resource: a parsed template, or raw content (`#include`).
pub enum Loaded {
    /// A template, parsed (`Template`).
    Template(Template),
    /// A resource's raw text (`ContentResource`).
    Content(String),
}

/// `org.apache.velocity.Template`: a named, parsed resource.
#[derive(Debug)]
pub struct Template {
    /// The template's name and what parsing it recorded.
    pub info: Arc<vm::TemplateInfo>,
    /// The parsed template.
    pub tree: Arc<Block>,
}

const RESOURCE_TEMPLATE: u8 = 1;
const RESOURCE_CONTENT: u8 = 2;

/// MockServer's custom `#ifnull` directive (`org.mockserver.templates.engine.velocity.directives.Ifnull`).
pub use directive::IFNULL_DIRECTIVE;

/// A `VelocityEngine` (`RuntimeInstance`): its configuration, the global macro namespace, the resource manager and
/// the application attributes.
pub struct Engine {
    /// The configuration the engine was initialised with (`RuntimeInstance`'s properties, read).
    pub config: config::RuntimeConfig,
    pub(crate) vms: vm::Velocimacros,
    resources: resource::ResourceManager<Loaded>,
    attributes: Mutex<HashMap<String, Arc<dyn std::any::Any + Send + Sync>>>,
    /// `runtimeDirectives` beyond the built-in ones.
    custom_directives: HashMap<String, Arc<dyn directive::Directive>>,
    /// Built-in directives taken out with `removeDirective`.
    removed_directives: std::collections::HashSet<String>,
    /// The application event cartridge (`event_handler.*.class`, or added programmatically).
    pub events: Arc<events::EventCartridge>,
    /// MockServer's string repository and the synthetic names in it, when this is its engine (`Engine::mockserver`).
    mockserver: Option<MockServerTemplates>,
    /// `UberspectImpl.getConversionHandler()`.
    conversion_handler: Option<Arc<dyn uberspect::TypeConversionHandler>>,
    /// `getLog()`, and whether it is `runtime.log.instance` (which every namespace then shares).
    log: Arc<dyn log::Logger>,
    log_instance: bool,
    /// `getUberspect()`.
    uberspect: Arc<dyn uberspector::Uberspect>,
    /// `resource.manager.instance`, used instead of the engine's own resource management.
    resource_manager: Option<Arc<dyn resource::ResourceManagerApi>>,
    /// `VelocimacroFactory.libModMap`: each library's loader and modification time when last loaded.
    lib_mod: Mutex<HashMap<String, (usize, i64)>>,
}

impl Default for Engine {
    fn default() -> Engine {
        Engine::new()
    }
}

/// Parses an interpolated string literal's content (`ASTStringLiteral.init`).
impl Engine {
    /// An engine with Velocity's defaults.
    pub fn new() -> Engine {
        Engine::with_properties(&props::ExtProperties::new()).expect("Velocity's defaults initialise")
    }

    /// An engine configured as MockServer's `VelocityTemplateEngine` configures its own: a string resource loader
    /// over a static repository of its own, caching parsed templates without re-checking them.
    pub fn mockserver() -> Engine {
        static INSTANCES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let name = format!("mockserver-velocity-templates-{}", INSTANCES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1);
        let mut p = config::mockserver_properties();
        p.set_property(config::CUSTOM_DIRECTIVES, IFNULL_DIRECTIVE.into());
        for (k, v) in [
            ("resource.loader.string.class", resource::STRING_LOADER),
            ("resource.loader.string.repository.static", "true"),
            ("resource.loader.string.repository.name", name.as_str()),
            ("resource.loader.string.repository.encoding", "UTF-8"),
            ("resource.loader.string.cache", "true"),
            ("resource.loader.string.modification_check_interval", "0"),
        ] {
            p.set_property(k, v.into());
        }
        let mut engine = Engine::with_properties(&p).expect("MockServer's configuration initialises");
        engine.mockserver = resource::static_repository(&name).map(|repository| MockServerTemplates { repository_name: name, repository, names: Default::default() });
        engine
    }

    /// `new VelocityEngine(); setProperty(…); init()`.
    pub fn with_properties(overrides: &props::ExtProperties) -> Result<Engine, VelocityError> {
        Engine::with_properties_and_attributes(overrides, Vec::new())
    }

    /// `setApplicationAttribute(…)` before `init()`, then `init()`.
    pub fn with_properties_and_attributes(overrides: &props::ExtProperties, attributes: Vec<(String, Arc<dyn std::any::Any + Send + Sync>)>) -> Result<Engine, VelocityError> {
        let config = config::RuntimeConfig::new(overrides);
        let (log, log_instance) = log::configured(&config.props)?;
        let mut custom_directives: HashMap<String, Arc<dyn directive::Directive>> = HashMap::new();
        for class in config.props.string_array(config::CUSTOM_DIRECTIVES) {
            use log::LogExt;
            let d = directive::load(&class).inspect_err(|e| log.thrown(log::Level::Error, || e.message(), &log::Throwable::from(e)))?;
            custom_directives.insert(d.name(), d);
            log.debug(|| format!("Loaded User Directive: {class}"));
        }
        let attributes = Mutex::new(attributes.into_iter().collect::<HashMap<_, _>>());
        let resources = resource::ResourceManager::new(&config.props, &attributes, &config.default_encoding)?;
        let conversion_handler = uberspect::configured_handler(&config.props)?;
        let uberspect = uberspector::configured(&config.uberspectors())?;
        let resource_manager = match config.props.get(config::RESOURCE_MANAGER_INSTANCE) {
            Some(props::PropValue::Object(o)) => o.downcast_ref::<Arc<dyn resource::ResourceManagerApi>>().cloned(),
            _ => None,
        };
        let engine = Engine {
            config,
            vms: vm::Velocimacros::new(),
            resources,
            attributes,
            custom_directives,
            removed_directives: Default::default(),
            events: events::EventCartridge::new(),
            mockserver: None,
            conversion_handler,
            log,
            log_instance,
            lib_mod: Mutex::new(HashMap::new()),
            uberspect,
            resource_manager,
        };
        engine.uberspect.init(&engine)?;
        if let Some(m) = &engine.resource_manager {
            m.initialize(&engine)?;
        }
        engine.init_event_handlers()?;
        engine.init_velocimacros()?;
        Ok(engine)
    }

    /// `getUberspect()`.
    pub fn uberspect(&self) -> Arc<dyn uberspector::Uberspect> {
        self.uberspect.clone()
    }

    /// `getLog()`.
    pub fn log(&self) -> Arc<dyn log::Logger> {
        self.log.clone()
    }

    /// `getLog(childNamespace)`: the configured instance, else `<runtime.log.name default>.<namespace>`.
    pub fn log_for(&self, namespace: &str) -> Arc<dyn log::Logger> {
        if self.log_instance {
            return self.log.clone();
        }
        Arc::new(log::NopLogger(format!("{}.{namespace}", self.config.props.string_or(log::RUNTIME_LOG_NAME, log::DEFAULT_RUNTIME_LOG_NAME))))
    }

    /// `getProperty(key)`.
    pub fn property(&self, key: &str) -> Option<props::PropValue> {
        self.config.props.get(key).cloned()
    }

    /// The uberspector's conversion handler (`UberspectImpl.getConversionHandler()`).
    pub fn conversion_handler(&self) -> Option<Arc<dyn uberspect::TypeConversionHandler>> {
        self.conversion_handler.clone()
    }

    /// `getApplicationAttribute(key)`.
    pub fn application_attribute(&self, key: &str) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        self.attributes.lock().get(key).cloned()
    }

    /// `setApplicationAttribute(key, value)`.
    pub fn set_application_attribute(&self, key: &str, value: Arc<dyn std::any::Any + Send + Sync>) {
        self.attributes.lock().insert(key.to_string(), value);
    }

    /// The string repository kept as the application attribute `name` (`repository.static = false`).
    pub fn string_repository(&self, name: &str) -> Option<Arc<resource::StringResourceRepository>> {
        self.application_attribute(name).and_then(|a| a.downcast::<resource::StringResourceRepository>().ok())
    }

    pub(crate) fn directive_type(&self, name: &str) -> Option<parser::DirectiveType> {
        parser::builtin_directive_type(name).filter(|_| !self.removed_directives.contains(name)).or_else(|| self.custom_directives.get(name).map(|d| d.directive_type()))
    }

    /// A built-in directive taken out with `removeDirective`.
    pub(crate) fn is_removed_directive(&self, name: &str) -> bool {
        self.removed_directives.contains(name)
    }

    pub(crate) fn custom_directive(&self, name: &str) -> Option<Arc<dyn directive::Directive>> {
        self.custom_directives.get(name).cloned()
    }

    /// `addDirective(directive)`: before templates using it are parsed.
    pub fn add_directive(&mut self, directive: Arc<dyn directive::Directive>) {
        self.custom_directives.insert(directive.name(), directive);
    }

    /// `removeDirective(name)`.
    pub fn remove_directive(&mut self, name: &str) {
        self.custom_directives.remove(name);
        if parser::builtin_directive_type(name).is_some() {
            self.removed_directives.insert(name.to_string());
        }
    }

    /// `loadDirective(directiveClass)`: a built-in directive class (`org.apache.velocity.runtime.directive.Include`)
    /// brings that directive back; any other is loaded like `runtime.custom_directives`.
    pub fn load_directive(&mut self, class: &str) -> Result<(), VelocityError> {
        const BUILTIN: [(&str, &str); 8] =
            [("Foreach", "foreach"), ("Include", "include"), ("Parse", "parse"), ("Macro", "macro"), ("Evaluate", "evaluate"), ("Break", "break"), ("Define", "define"), ("Stop", "stop")];
        if let Some(simple) = class.strip_prefix("org.apache.velocity.runtime.directive.") {
            if let Some((_, name)) = BUILTIN.iter().find(|(c, _)| *c == simple) {
                self.removed_directives.remove(*name);
                self.custom_directives.remove(*name);
                return Ok(());
            }
        }
        let d = directive::load(class)?;
        self.custom_directives.insert(d.name(), d);
        Ok(())
    }

    /// `initializeEventHandlers`: each class named under the four `event_handler.*.class` keys.
    fn init_event_handlers(&self) -> Result<(), VelocityError> {
        for key in [config::EVENTHANDLER_REFERENCEINSERTION, config::EVENTHANDLER_METHODEXCEPTION, config::EVENTHANDLER_INCLUDE, config::EVENTHANDLER_INVALIDREFERENCES] {
            for class in self.config.props.string_array(key) {
                if class.is_empty() {
                    continue;
                }
                let Some(factory) = events::handler_class(&class) else {
                    return Err(VelocityError::velocity(format!("The specified class for {key} ({class}) does not exist or is not accessible to the current classloader.")));
                };
                let h = factory();
                let only = match key {
                    config::EVENTHANDLER_REFERENCEINSERTION => events::Handler { reference_insertion: h.reference_insertion, ..Default::default() },
                    config::EVENTHANDLER_METHODEXCEPTION => events::Handler { method_exception: h.method_exception, ..Default::default() },
                    config::EVENTHANDLER_INCLUDE => events::Handler { include: h.include, ..Default::default() },
                    _ => events::Handler { invalid_reference: h.invalid_reference, ..Default::default() },
                };
                self.events.add(only);
            }
        }
        Ok(())
    }

    /// `VelocimacroFactory.initVelocimacro`: the libraries (`velocimacro.library.path`, else `velocimacros.vtl` or
    /// `VM_global_library.vm` when a loader has them), then the inline permissions.
    fn init_velocimacros(&self) -> Result<(), VelocityError> {
        use log::LogExt;
        let log = self.log_for("macro");
        log.trace(|| "initialization starting.".into());
        let mut libraries = self.config.props.string_array(config::VM_LIBRARY);
        if self.config.props.get(config::VM_LIBRARY).is_none() {
            log.debug(|| format!("\"{}\" is not set. Trying default library: {}", config::VM_LIBRARY, config::VM_LIBRARY_DEFAULT));
            if self.loader_name_for_resource(config::VM_LIBRARY_DEFAULT).is_some() {
                libraries = vec![config::VM_LIBRARY_DEFAULT.to_string()];
            } else {
                log.debug(|| format!("Default library {} not found. Trying old default library: VM_global_library.vm", config::VM_LIBRARY_DEFAULT));
                if self.loader_name_for_resource("VM_global_library.vm").is_some() {
                    libraries = vec!["VM_global_library.vm".to_string()];
                } else {
                    log.debug(|| "Old default library VM_global_library.vm not found.".into());
                }
            }
        }
        self.vms.begin_libraries(libraries.clone());
        for lib in libraries.iter().filter(|l| !l.is_empty()) {
            self.vms.set_register_from_lib(true);
            log.debug(|| format!("adding VMs from VM library: {lib}"));
            if let Err(e) = self.get_template(lib, None) {
                let msg = format!("Velocimacro: Error using VM library: {lib}");
                log.thrown(log::Level::Error, || msg.clone(), &log::Throwable::from(&e));
                return Err(VelocityError::velocity(msg).with_cause(java_compat::lang::JError::new(&e.class_name(), e.message())));
            }
            if let Some(stamp) = self.resources.stamp(lib) {
                self.lib_mod.lock().insert(lib.clone(), stamp);
            }
            log.trace(|| "VM library registration complete.".into());
            self.vms.set_register_from_lib(false);
        }
        let c = &self.config;
        log.debug(|| if c.vm_inline_allow { "allowInline = true: VMs can be defined inline in templates".into() } else { "allowInline = false: VMs can NOT be defined inline in templates".into() });
        log.debug(|| {
            if c.vm_replace_global {
                "allowInlineToOverride = true: VMs defined inline may replace previous VM definitions".into()
            } else {
                "allowInlineToOverride = false: VMs defined inline may NOT replace previous VM definitions".into()
            }
        });
        log.debug(|| {
            if c.vm_local_scope {
                "allowInlineLocal = true: VMs defined inline will be local to their defining template only.".into()
            } else {
                "allowInlineLocal = false: VMs defined inline will be global in scope if allowed.".into()
            }
        });
        log.debug(|| {
            if c.vm_library_autoreload {
                "autoload on: VM system will automatically reload global library macros".into()
            } else {
                "autoload off: VM system will not automatically reload global library macros".into()
            }
        });
        self.vms.finish_init(c.vm_inline_allow, c.vm_replace_global, c.vm_local_scope, c.vm_library_autoreload);
        log.trace(|| "Velocimacro: initialization complete.".into());
        Ok(())
    }

    /// `VelocimacroFactory.getVelocimacro`: with `velocimacro.library.autoreload`, a macro from a library that changed
    /// since it was loaded reloads the library first.
    pub(crate) fn velocimacro(&self, name: &str, rendering: Option<&vm::TemplateInfo>, source: Option<&vm::TemplateInfo>) -> Result<Option<Arc<vm::Macro>>, VelocityError> {
        let vp = self.vms.get(name, rendering, source);
        if vp.is_none() || !self.vms.autoload() {
            return Ok(vp);
        }
        let Some(lib) = self.vms.library_name(name, source) else { return Ok(vp) };
        let stamp = self.lib_mod.lock().get(&lib).copied();
        if let Some((loader, time)) = stamp {
            let current = self.resources.last_modified_in(loader, &lib);
            if current > time {
                use log::LogExt;
                let log = self.log_for("macro");
                log.debug(|| format!("auto-reloading VMs from VM library: {lib}"));
                self.lib_mod.lock().insert(lib.clone(), (loader, current));
                if let Err(e) = self.get_template(&lib, None) {
                    let msg = format!("Velocimacro: Error using VM library: {lib}");
                    log.thrown(log::Level::Error, || msg.clone(), &log::Throwable::from(&e));
                    return Err(VelocityError::velocity(msg).with_cause(java_compat::lang::JError::new(&e.class_name(), e.message())));
                }
                if let Some(stamp) = self.resources.stamp(&lib) {
                    self.lib_mod.lock().insert(lib.clone(), stamp);
                }
            }
        }
        // Velocity passes the templates swapped here.
        Ok(self.vms.get(name, source, rendering))
    }

    /// Runs `f` with the parser settings this engine implies, for a template being parsed into `template`.
    pub(crate) fn with_settings<T>(&self, template: &vm::TemplateInfo, f: impl FnOnce(&parser::Settings) -> T) -> T {
        let is_macro = |n: &str| self.vms.is_macro(n, Some(template));
        let directive = |n: &str| self.directive_type(n);
        let settings = parser::Settings { hyphen_allowed: self.config.hyphen_allowed, bc: self.config.gobbling == config::SpaceGobbling::Bc, gobbling: self.config.gobbling, strict_escape: self.config.strict_escape, directive: &directive, is_macro: &is_macro };
        f(&settings)
    }

    /// `rsvc.parse(reader, template)` then `nodeTree.init`: the nodes belong to `template`, and the macros they
    /// define are added to its namespace or the global one.
    pub(crate) fn parse_into(&self, template: &Arc<vm::TemplateInfo>, source: &str) -> Result<Arc<Block>, VelocityError> {
        let mut tree = self.with_settings(template, |settings| parser::Parser::new(source, settings).process()).map_err(|e| if e.is_parse() { e.in_template(&template.name) } else { e })?;
        init::init(&mut tree, template, &init::InitSettings { strict_ref: self.config.strict_ref, bc: self.config.gobbling == config::SpaceGobbling::Bc, structured: self.config.gobbling == config::SpaceGobbling::Structured }).map_err(|e| {
            // `ParseErrorException(TemplateInitException)`: its message and location.
            let mut p = VelocityError::parse(e.message.clone());
            p.template = e.template.clone();
            p.line = e.line;
            p.column = e.column;
            p
        })?;
        self.register_macros(&tree.nodes, template);
        Ok(Arc::new(tree))
    }

    /// Parses `source` as a new template named `name`.
    pub(crate) fn parse_named(&self, name: &str, source: &str) -> Result<(Arc<vm::TemplateInfo>, Arc<Block>), VelocityError> {
        let info = vm::TemplateInfo::new(name, &self.config.default_encoding);
        let tree = self.parse_into(&info, source).map_err(|e| e.lexical_in(None))?;
        Ok((info, tree))
    }

    fn register_macros(&self, nodes: &[ast::Node], template: &vm::TemplateInfo) {
        for n in nodes {
            match n {
                ast::Node::Directive(d) => {
                    for a in &d.args {
                        self.register_in_expr(a, template);
                    }
                    if d.name == "macro" {
                        self.register(d, template);
                    }
                    if let Some(b) = &d.block {
                        self.register_macros(&b.nodes, template);
                    }
                    if let Some(b) = &d.else_block {
                        self.register_macros(&b.nodes, template);
                    }
                }
                ast::Node::If(i) => {
                    self.register_in_expr(&i.condition, template);
                    self.register_macros(&i.then.nodes, template);
                    for (c, b) in &i.else_ifs {
                        self.register_in_expr(c, template);
                        self.register_macros(&b.nodes, template);
                    }
                    if let Some(b) = &i.otherwise {
                        self.register_macros(&b.nodes, template);
                    }
                }
                ast::Node::Set(s) => self.register_in_expr(&s.right, template),
                ast::Node::Reference(_) | ast::Node::Text(_) => {}
            }
        }
    }

    fn register_in_expr(&self, e: &ast::Expr, template: &vm::TemplateInfo) {
        use ast::Expr;
        match e {
            Expr::Str { interpolated: Some(b), .. } => self.register_macros(&b.nodes, template),
            Expr::Binary(_, l, r, _) | Expr::Range(l, r) => {
                self.register_in_expr(l, template);
                self.register_in_expr(r, template);
            }
            Expr::Not(x) | Expr::Negate(x) | Expr::Group(x) => self.register_in_expr(x, template),
            Expr::List(items) => items.iter().for_each(|i| self.register_in_expr(i, template)),
            Expr::Map(entries) => entries.iter().for_each(|(k, v)| {
                self.register_in_expr(k, template);
                self.register_in_expr(v, template);
            }),
            _ => {}
        }
    }

    /// `Macro.init`: `rsvc.addVelocimacro(name, body, args, template)`.
    fn register(&self, d: &ast::Directive, template: &vm::TemplateInfo) {
        let Some(ast::Expr::Word(name)) = d.args.first() else { return };
        let arg_name = |e: &ast::Expr| match e {
            ast::Expr::Reference(r) => r.first_image.trim_start_matches('$').to_string(),
            other => other.literal().trim_start_matches('$').to_string(),
        };
        let mut args = Vec::new();
        let mut i = 1;
        while i < d.args.len() {
            if d.arg_types[i] == ast::ArgType::DirectiveAssign {
                args.push(vm::MacroArg { name: arg_name(&d.args[i]), default: d.args.get(i + 1).cloned() });
                i += 2;
            } else {
                args.push(vm::MacroArg { name: arg_name(&d.args[i]), default: None });
                i += 1;
            }
        }
        let body = d.block.clone().unwrap_or_default();
        // `VelocimacroProxy.setLocation`: where the body starts.
        let (line, column) = (body.start.line, body.start.column);
        let m = vm::Macro { name: name.clone(), args, body, source_template: template.name.clone(), line, column };
        self.vms.add(m, template);
    }

    /// `getTemplate(name, encoding)`: through the resource manager, parsed when (re)loaded.
    pub fn get_template(&self, name: &str, encoding: Option<&str>) -> Result<Arc<Loaded>, VelocityError> {
        let encoding = encoding.map(str::to_string).unwrap_or_else(|| self.config.default_encoding.clone());
        if let Some(m) = &self.resource_manager {
            // Java returns the manager's `null`; here that is a missing resource.
            return m.get_resource(name, resource::ResourceType::Template, &encoding)?.ok_or_else(|| VelocityError::resource_not_found(format!("Unable to find resource '{name}'")));
        }
        self.resources.get(RESOURCE_TEMPLATE, name, &encoding, &|name, text| {
            let info = vm::TemplateInfo::new(name, &encoding);
            let tree = self.parse_into(&info, text).map_err(|e| e.lexical_in(Some(name)))?;
            Ok(Loaded::Template(Template { info, tree }))
        })
    }

    /// `getContent(name)`: the raw text (`#include`).
    pub fn get_content(&self, name: &str, encoding: Option<&str>) -> Result<Arc<Loaded>, VelocityError> {
        let encoding = encoding.map(str::to_string).unwrap_or_else(|| self.config.default_encoding.clone());
        self.resources.get(RESOURCE_CONTENT, name, &encoding, &|_, text| Ok(Loaded::Content(text.to_string())))
    }

    /// `resourceExists(name)`: some loader has it.
    pub fn resource_exists(&self, name: &str) -> bool {
        self.resources.loader_name_for(name).is_some()
    }

    /// `getLoaderNameForResource(name)`.
    pub fn loader_name_for_resource(&self, name: &str) -> Option<String> {
        if let Some(m) = &self.resource_manager {
            return m.loader_name_for_resource(name);
        }
        self.resources.loader_name_for(name)
    }

    /// `Template.merge(context, writer)`.
    /// `Template.merge(context, writer, macroLibraries)`.
    pub fn merge_with_libraries(&self, template: &Loaded, context: &mut Context, macro_libraries: &[&str]) -> Result<String, VelocityError> {
        use log::LogExt;
        let Loaded::Template(t) = template else { return Err(VelocityError::velocity("not a template")) };
        let mut libraries = Vec::new();
        for lib in macro_libraries {
            match self.get_template(lib, None) {
                Ok(l) => match &*l {
                    Loaded::Template(l) => libraries.push(l.info.clone()),
                    Loaded::Content(_) => {}
                },
                Err(e) if e.kind == ErrorKind::ResourceNotFound => {
                    self.log().error(|| format!("cannot find template {lib}"));
                    return Err(e);
                }
                Err(e) if e.kind == ErrorKind::ParseError => {
                    self.log_for("parser").thrown(log::Level::Error, || format!("syntax error in template {lib}: {}", e.message()), &log::Throwable::from(&e));
                    return Err(e);
                }
                Err(_) => return Err(VelocityError::runtime(java_compat::lang::JError::new("java.lang.RuntimeException", format!("parse failed in template  {lib}.")))),
            }
        }
        let mut out = String::new();
        render::Renderer::new(self).with_macro_libraries(libraries).merge(&t.info, &t.tree, context, &mut out)?;
        Ok(out)
    }

    /// `template.merge(context, writer)`: renders a template [`Engine::get_template`] loaded.
    pub fn merge(&self, template: &Loaded, context: &mut Context) -> Result<String, VelocityError> {
        let Loaded::Template(t) = template else { return Err(VelocityError::velocity("not a template")) };
        let _conversions = uberspect::ConversionsGuard::new(!self.config.is_secure());
        let mut out = String::new();
        render::Renderer::new(self).merge(&t.info, &t.tree, context, &mut out)?;
        Ok(out)
    }

    /// `mergeTemplate(name, encoding, context, writer)`.
    pub fn merge_template(&self, name: &str, encoding: Option<&str>, context: &mut Context) -> Result<String, VelocityError> {
        let t = self.get_template(name, encoding)?;
        self.merge(&t, context)
    }

    /// `invokeVelocimacro(vmName, logTag, params, context, writer)`: a global macro called with the context
    /// references named in `params`.
    pub fn invoke_velocimacro(&self, vm_name: &str, log_tag: Option<&str>, params: &[&str], context: &mut Context) -> Result<String, VelocityError> {
        use log::LogExt;
        let log_tag = log_tag.unwrap_or(vm_name);
        // Only the global namespace is scanned, so macros inline in templates are not found.
        if !self.vms.is_macro(vm_name, None) {
            let msg = format!("RuntimeInstance.invokeVelocimacro() : VM '{vm_name}' is not registered.");
            self.log().error(|| msg.clone());
            return Err(VelocityError::velocity(msg));
        }
        let mut template = format!("#{vm_name}(");
        for p in params {
            template.push_str(&format!(" ${p}"));
        }
        template.push_str(" )");
        self.evaluate(context, log_tag, &template)
    }

    /// `evaluate(context, writer, logTag, template)`: parses (without caching) and renders.
    pub fn evaluate(&self, context: &mut Context, log_tag: &str, source: &str) -> Result<String, VelocityError> {
        let (info, tree) = self.parse_named(log_tag, source)?;
        let _conversions = uberspect::ConversionsGuard::new(!self.config.is_secure());
        let mut out = String::new();
        render::Renderer::new(self).evaluate_top(&info, &tree, context, &mut out)?;
        Ok(out)
    }

    /// `VelocityTemplateEngine.renderTemplate`: each distinct template string is registered in MockServer's
    /// repository under a synthetic name from the engine's counter (`t0`, `t1`, …), which its errors then cite; one
    /// that fails to load is dropped and rendered through `evaluate` as `VelocityResponseTemplate`.
    pub fn render(&self, source: &str, context: &mut Context) -> Result<String, VelocityError> {
        let Some(templates) = &self.mockserver else { return self.evaluate(context, "VelocityResponseTemplate", source) };
        let name = templates.name(source);
        match self.get_template(&name, Some("UTF-8")) {
            Ok(t) => self.merge(&t, context),
            Err(_) => {
                templates.forget(source, &name);
                self.evaluate(context, "VelocityResponseTemplate", source)
            }
        }
    }
}

/// `VelocityTemplateEngine.EngineHolder`'s `templateRepository`, `templateNames` and `templateNameCounter`.
struct MockServerTemplates {
    repository_name: String,
    repository: Arc<resource::StringResourceRepository>,
    names: Mutex<TemplateNames>,
}

/// `templateNames`: template bodies to their names and when each was last used, access-ordered by that.
#[derive(Default)]
struct TemplateNames {
    by_body: HashMap<String, (String, u64)>,
    counter: u64,
    clock: u64,
}

/// `VelocityTemplateEngine.PARSED_TEMPLATE_CACHE_MAX`.
const PARSED_TEMPLATE_CACHE_MAX: usize = 1000;

impl MockServerTemplates {
    /// `templateNames.computeIfAbsent(template, …)`: the body's name, registering it under the next one when new;
    /// the least recently used body past the bound leaves the repository too.
    fn name(&self, source: &str) -> String {
        let mut names = self.names.lock();
        names.clock += 1;
        let clock = names.clock;
        if let Some((name, used)) = names.by_body.get_mut(source) {
            *used = clock;
            return name.clone();
        }
        let name = format!("t{}", names.counter);
        names.counter += 1;
        self.repository.put_with_encoding(&name, source, "UTF-8");
        names.by_body.insert(source.to_string(), (name.clone(), clock));
        if names.by_body.len() > PARSED_TEMPLATE_CACHE_MAX {
            let eldest = names.by_body.iter().min_by_key(|(_, (_, used))| *used).map(|(body, _)| body.clone());
            if let Some((evicted, _)) = eldest.and_then(|body| names.by_body.remove(&body)) {
                self.repository.remove(&evicted);
            }
        }
        name
    }

    /// A body that failed to load is neither cached nor counted against the bound; its name is not reused.
    fn forget(&self, source: &str, name: &str) {
        self.names.lock().by_body.remove(source);
        self.repository.remove(name);
    }
}

impl Drop for MockServerTemplates {
    /// The engine's static repository goes with it, as upstream releases a rebuilt engine's.
    fn drop(&mut self) {
        resource::remove_static_repository(&self.repository_name);
    }
}
