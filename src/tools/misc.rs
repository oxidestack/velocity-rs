//! The small request- and application-scoped tools: `ContextTool` (`$context`), `RenderTool` (`$render`),
//! `LogTool` (`$log`) and `ImportTool` (`$import`).

use std::sync::{Arc, Weak};

use java_compat::lang::{JError, JList, JMap, JObject, JValue, ListKind};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;

use super::classes::{Annotation, JavaClass};
use super::safe::{self, SafeConfig, Tool};
use super::toolbox::{self, ToolContext};
use super::values::ValueParser;
use super::{c, identity, o, s, tool_object, val};

// ---------------------------------------------------------------------------------------------------------
// ContextTool

/// The class name of `org.apache.velocity.tools.generic.ContextTool`.
pub const CONTEXT_TOOL: &str = "org.apache.velocity.tools.generic.ContextTool";

/// `ContextTool`: the tool context's keys and values.
pub struct ContextTool {
    safe: SafeConfig,
    context: Mutex<JValue>,
    me: Weak<ContextTool>,
}

impl ContextTool {
    /// `new ContextTool()`: no context until configured.
    pub fn new() -> Arc<ContextTool> {
        Arc::new_cyclic(|me| ContextTool { safe: SafeConfig::default(), context: Mutex::new(JValue::Null), me: me.clone() })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: takes the tool context from the configuration's `velocityContext`; nothing once
    /// locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    fn ctx(&self) -> Option<Arc<ToolContext>> {
        toolbox::as_tool_context(&self.context.lock())
    }

    /// `getThis()`: the context.
    pub fn get_this(&self) -> JValue {
        self.context.lock().clone()
    }

    /// `getToolbox()`: every tool of the context.
    pub fn get_toolbox(&self) -> Result<Option<JMap>, JError> {
        match self.ctx() {
            Some(c) => Ok(Some(JMap::from_entries(java_compat::lang::MapKind::Hash, c.get_toolbox()?.into_iter().map(|(k, v)| (JValue::str(k), v))))),
            None => Ok(None),
        }
    }

    /// `getKeys()`: the context's keys (without dotted ones in safe mode).
    pub fn get_keys(&self) -> Vec<String> {
        let keys = self.ctx().map(|c| c.key_set()).unwrap_or_default();
        if self.safe.is_safe_mode() { keys.into_iter().filter(|k| !k.contains('.')).collect() } else { keys }
    }

    /// `getValues()`: the values of those keys.
    pub fn get_values(&self) -> Result<Vec<JValue>, JError> {
        let Some(c) = self.ctx() else { return Err(super::npe()) };
        let mut values: Vec<JValue> = Vec::new();
        for k in self.get_keys() {
            let v = c.get(&k)?;
            if !values.iter().any(|x| java_compat::lang::java_equals(x, &v)) {
                values.push(v);
            }
        }
        Ok(values)
    }

    /// `contains(key)`: whether `get(key)` is non-`null`; `NullPointerException` without a context.
    pub fn contains(&self, key: &JValue) -> Result<bool, JError> {
        Ok(!self.get(key)?.is_null())
    }

    /// `get(key)`: `null` for dotted keys in safe mode.
    pub fn get(&self, key: &JValue) -> Result<JValue, JError> {
        let k = key.to_java_string();
        if self.safe.is_safe_mode() && k.contains('.') {
            return Ok(JValue::Null);
        }
        match self.ctx() {
            Some(c) => c.get(&k),
            None => Err(super::npe()),
        }
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for ContextTool {
    fn safe(&self) -> &SafeConfig {
        &self.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        *self.context.lock() = values.get_value(toolbox::CONTEXT_KEY);
    }
}

fn strings_set(v: Vec<String>) -> JValue {
    JValue::List(JList::new(ListKind::Set, v.into_iter().map(JValue::str).collect()))
}

fn context_tool_def() -> Arc<ClassDef> {
    type T = ContextTool;
    let def = ClassDef::new(CONTEXT_TOOL)
        .extends(&[safe::SAFE_CONFIG])
        .implements(&["java.io.Serializable"])
        .method::<T>("getThis", &[], Some(c("org.apache.velocity.context.Context")), |t, _, _| val(t.get_this()))
        .method::<T>("getToolbox", &[], Some(c("java.util.Map")), |t, _, _| val(t.get_toolbox()?.map(JValue::Map).unwrap_or(JValue::Null)))
        .method::<T>("getKeys", &[], Some(c("java.util.Set")), |t, _, _| val(strings_set(t.get_keys())))
        .method::<T>("getValues", &[], Some(c("java.util.Set")), |t, _, _| val(JValue::List(JList::new(ListKind::Set, t.get_values()?))))
        .method::<T>("contains", &[o()], Some(JType::Boolean), |t, _, a| val(JValue::Bool(t.contains(&a[0])?)))
        .method::<T>("get", &[o()], Some(o()), |t, _, a| val(t.get(&a[0])?));
    super::object_methods::<T>(safe::safe_config_methods::<T>(def)).register()
}

tool_object!(ContextTool, context_tool_def);

pub(crate) fn context_tool_class() -> JavaClass {
    JavaClass::tool(context_tool_def(), || ContextTool::new().value())
        .annotated(Annotation::default_key("context"))
        .annotated(Annotation::invalid_scope(&[super::scope::APPLICATION, super::scope::SESSION]))
}

// ---------------------------------------------------------------------------------------------------------
// RenderTool

/// The class name of `org.apache.velocity.tools.generic.RenderTool`.
pub const RENDER_TOOL: &str = "org.apache.velocity.tools.generic.RenderTool";
/// `RenderTool.DEFAULT_PARSE_DEPTH`: how many times `recurse` evaluates at most.
pub const DEFAULT_PARSE_DEPTH: i32 = 20;
/// `RenderTool.KEY_PARSE_DEPTH`: the configuration key for the parse depth.
pub const KEY_PARSE_DEPTH: &str = "parse.depth";
/// `RenderTool.KEY_CATCH_EXCEPTIONS`: the configuration key for returning `null` instead of throwing.
pub const KEY_CATCH_EXCEPTIONS: &str = "catch.exceptions";
/// `RenderTool.KEY_FORCE_THREAD_SAFE`: the configuration key for evaluating in a child of the tool's context.
pub const KEY_FORCE_THREAD_SAFE: &str = "forceThreadSafe";

struct RenderState {
    engine: Option<Arc<crate::Engine>>,
    context: JValue,
    parse_depth: i32,
    catch_exceptions: bool,
    force_thread_safe: bool,
}

/// `RenderTool`: templates evaluated from strings, once or until they stop changing.
pub struct RenderTool {
    safe: SafeConfig,
    st: Mutex<RenderState>,
    me: Weak<RenderTool>,
}

/// The engine `Velocity.evaluate` uses when a render tool has none.
fn singleton() -> Arc<crate::Engine> {
    static ENGINE: std::sync::LazyLock<Arc<crate::Engine>> = std::sync::LazyLock::new(|| Arc::new(crate::Engine::new()));
    ENGINE.clone()
}

impl RenderTool {
    /// `new RenderTool()`: parse depth 20, exceptions caught, thread safety forced.
    pub fn new() -> Arc<RenderTool> {
        Arc::new_cyclic(|me| RenderTool {
            safe: SafeConfig::default(),
            st: Mutex::new(RenderState { engine: None, context: JValue::Null, parse_depth: DEFAULT_PARSE_DEPTH, catch_exceptions: true, force_thread_safe: true }),
            me: me.clone(),
        })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: reads the parse depth, `catch.exceptions` and `forceThreadSafe` (off in request
    /// scope); nothing once locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    /// `setVelocityEngine(engine)`: the engine templates evaluate with, `None` for the shared default engine.
    pub fn set_velocity_engine(&self, engine: Option<Arc<crate::Engine>>) {
        self.st.lock().engine = engine;
    }

    /// `setParseDepth(depth)`: ignored (logged) once locked.
    pub fn set_parse_depth(&self, depth: i32) {
        if !self.safe.is_config_locked() {
            self.st.lock().parse_depth = depth;
        } else if self.get_parse_depth() != depth {
            super::log::error(RENDER_TOOL, "Attempt was made to alter parse depth while config was locked.");
        }
    }

    /// `setVelocityContext(context)`.
    pub fn set_velocity_context(&self, context: JValue) -> Result<(), JError> {
        if !self.safe.is_config_locked() {
            if context.is_null() {
                return Err(JError::new("java.lang.NullPointerException", "context must not be null"));
            }
            self.st.lock().context = context;
        } else if !java_compat::lang::java_equals(&self.st.lock().context, &context) {
            super::log::error(RENDER_TOOL, "Attempt was made to set a new context while config was locked.");
        }
        Ok(())
    }

    /// `getParseDepth()`.
    pub fn get_parse_depth(&self) -> i32 {
        self.st.lock().parse_depth
    }

    /// `setCatchExceptions(catch)`: ignored (logged) once locked.
    pub fn set_catch_exceptions(&self, catch: bool) {
        if !self.safe.is_config_locked() {
            self.st.lock().catch_exceptions = catch;
        } else if self.get_catch_exceptions() != catch {
            super::log::error(RENDER_TOOL, "Attempt was made to alter catchE while config was locked.");
        }
    }

    /// `getCatchExceptions()`: whether evaluation errors yield `null` rather than an exception.
    pub fn get_catch_exceptions(&self) -> bool {
        self.st.lock().catch_exceptions
    }

    /// `eval(vtl)`: in (a child of, when thread safety is forced) the tool's context.
    pub fn eval(&self, vtl: Option<&str>) -> Result<Option<String>, JError> {
        let ctx = self.st.lock().context.clone();
        self.eval_in(&ctx, vtl)
    }

    /// `recurse(vtl)`.
    pub fn recurse(&self, vtl: Option<&str>) -> Result<Option<String>, JError> {
        let ctx = self.st.lock().context.clone();
        self.recurse_in(&ctx, vtl)
    }

    /// `eval(ctx, vtl)`: `null` when evaluation fails and exceptions are caught.
    pub fn eval_in(&self, ctx: &JValue, vtl: Option<&str>) -> Result<Option<String>, JError> {
        match self.internal_eval(ctx, vtl) {
            Ok(v) => Ok(v),
            Err(e) if self.get_catch_exceptions() => {
                super::log::error(RENDER_TOOL, &format!("evaluation failed: {e}"));
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    fn internal_eval(&self, ctx: &JValue, vtl: Option<&str>) -> Result<Option<String>, JError> {
        let Some(vtl) = vtl else { return Ok(None) };
        let (engine, force) = {
            let st = self.st.lock();
            (st.engine.clone().unwrap_or_else(singleton), st.force_thread_safe)
        };
        let tool_context = toolbox::as_tool_context(ctx);
        let mut context = crate::Context::new();
        if let Some(tc) = &tool_context {
            context = context.with_layer(tc.clone());
        }
        let out = engine.evaluate(&mut context, "RenderTool.eval()", vtl).map_err(|e| JError::new(&e.class_name(), e.message()))?;
        // Without forced thread safety the template writes straight into the tool context.
        if !force {
            if let Some(tc) = &tool_context {
                for k in context.keys() {
                    tc.put(&k, context.get(&k).unwrap_or(JValue::Null));
                }
            }
        }
        Ok(Some(out))
    }

    /// `recurse(ctx, vtl)`: evaluated again while the output changes, up to the parse depth.
    pub fn recurse_in(&self, ctx: &JValue, vtl: Option<&str>) -> Result<Option<String>, JError> {
        let mut vtl = vtl.map(str::to_string);
        let mut count = 0;
        loop {
            let result = self.eval_in(ctx, vtl.as_deref())?;
            match result {
                None => return Ok(None),
                Some(r) if Some(&r) == vtl.as_ref() => return Ok(Some(r)),
                Some(r) => {
                    if count < self.get_parse_depth() {
                        vtl = Some(r);
                        count += 1;
                    } else {
                        super::log::error(RENDER_TOOL, &format!("recursion exceeded the maximum parse depth of {} on the following template: {}", self.get_parse_depth(), vtl.unwrap_or_default()));
                        return Ok(Some(r));
                    }
                }
            }
        }
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for RenderTool {
    fn safe(&self) -> &SafeConfig {
        &self.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        if let Some(d) = values.get_integer(KEY_PARSE_DEPTH) {
            self.set_parse_depth(d);
        }
        if let Some(c) = values.get_boolean(KEY_CATCH_EXCEPTIONS) {
            self.set_catch_exceptions(c);
        }
        let mut st = self.st.lock();
        st.force_thread_safe = values.get_boolean_or(KEY_FORCE_THREAD_SAFE, st.force_thread_safe);
        if values.get_string("scope").as_deref() == Some(super::scope::REQUEST) {
            st.force_thread_safe = false;
        }
    }
}

fn render_tool_def() -> Arc<ClassDef> {
    type R = RenderTool;
    let ctx = || c("org.apache.velocity.context.Context");
    let def = ClassDef::new(RENDER_TOOL)
        .extends(&[safe::SAFE_CONFIG])
        .method::<R>("setVelocityEngine", &[c(toolbox::VELOCITY_ENGINE)], None, |t, _, a| {
            t.set_velocity_engine(toolbox::as_engine(&a[0]));
            Ok(None)
        })
        .method::<R>("setParseDepth", &[JType::Int], None, |t, _, a| {
            t.set_parse_depth(super::num::int_value(&a[0]));
            Ok(None)
        })
        .method::<R>("setVelocityContext", &[ctx()], None, |t, _, a| {
            t.set_velocity_context(a[0].clone())?;
            Ok(None)
        })
        .method::<R>("getParseDepth", &[], Some(JType::Int), |t, _, _| val(JValue::Int(t.get_parse_depth())))
        .method::<R>("setCatchExceptions", &[JType::Boolean], None, |t, _, a| {
            t.set_catch_exceptions(matches!(a[0], JValue::Bool(true)));
            Ok(None)
        })
        .method::<R>("getCatchExceptions", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.get_catch_exceptions())))
        .method::<R>("eval", &[s()], Some(s()), |t, _, a| super::text(t.eval(a[0].as_str())?))
        .method::<R>("recurse", &[s()], Some(s()), |t, _, a| super::text(t.recurse(a[0].as_str())?))
        .method::<R>("eval", &[ctx(), s()], Some(s()), |t, _, a| super::text(t.eval_in(&a[0], a[1].as_str())?))
        .method::<R>("recurse", &[ctx(), s()], Some(s()), |t, _, a| super::text(t.recurse_in(&a[0], a[1].as_str())?));
    super::object_methods::<R>(safe::safe_config_methods::<R>(def)).register()
}

tool_object!(RenderTool, render_tool_def);

pub(crate) fn render_tool_class() -> JavaClass {
    JavaClass::tool(render_tool_def(), || RenderTool::new().value()).annotated(Annotation::default_key("render")).annotated(Annotation::invalid_scope(&[super::scope::SESSION]))
}

// ---------------------------------------------------------------------------------------------------------
// LogTool

/// The class name of `org.apache.velocity.tools.generic.LogTool`.
pub const LOG_TOOL: &str = "org.apache.velocity.tools.generic.LogTool";

/// `LogTool`: messages to the tools' log.
pub struct LogTool {
    safe: SafeConfig,
    logger: Mutex<String>,
    me: Weak<LogTool>,
}

impl LogTool {
    /// `new LogTool()`: logging under its class name.
    pub fn new() -> Arc<LogTool> {
        Arc::new_cyclic(|me| LogTool { safe: SafeConfig::default(), logger: Mutex::new(LOG_TOOL.into()), me: me.clone() })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: picks the logger name; nothing once locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    /// `error`/`warn`/`info`/`debug`/`trace(message)`: `message` (`null` for `None`) at `level` to the tool's logger.
    pub fn log(&self, level: super::log::Level, message: Option<&str>) {
        super::log::log(level, &self.logger.lock(), message.unwrap_or("null"));
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for LogTool {
    fn safe(&self) -> &SafeConfig {
        &self.safe
    }

    /// `SafeConfig.initLogger`: `loggerName`, else `tools.<key>` under the engine's log, else the class.
    fn configure_values(&self, values: &ValueParser) {
        let name = match values.get_string(safe::LOGGER_NAME_KEY) {
            Some(n) => n,
            None => {
                let key = values.get_string(toolbox::TOOLKEY_KEY);
                match (values.get_boolean_or(safe::USE_CLASS_LOGGER_KEY, false), values.get_value(toolbox::ENGINE_KEY).is_null(), key) {
                    (false, false, Some(k)) => format!("org.apache.velocity.tools.{k}"),
                    _ => LOG_TOOL.into(),
                }
            }
        };
        *self.logger.lock() = name;
    }
}

fn log_tool_def() -> Arc<ClassDef> {
    use super::log::Level;
    type L = LogTool;
    let levels = [("error", Level::Error), ("warn", Level::Warn), ("info", Level::Info), ("debug", Level::Debug), ("trace", Level::Trace)];
    let mut def = ClassDef::new(LOG_TOOL).extends(&[safe::SAFE_CONFIG]).implements(&["java.io.Serializable"]);
    for (name, level) in levels {
        def = def.method::<L>(name, &[s()], None, move |t, _, a| {
            t.log(level, a[0].as_str());
            Ok(None)
        });
    }
    super::object_methods::<L>(safe::safe_config_methods::<L>(def)).register()
}

tool_object!(LogTool, log_tool_def);

pub(crate) fn log_tool_class() -> JavaClass {
    JavaClass::tool(log_tool_def(), || LogTool::new().value()).annotated(Annotation::default_key("log")).annotated(Annotation::valid_scope(&[super::scope::APPLICATION]))
}

// ---------------------------------------------------------------------------------------------------------
// ImportTool

/// The class name of `org.apache.velocity.tools.generic.ImportTool`.
pub const IMPORT_TOOL: &str = "org.apache.velocity.tools.generic.ImportTool";

/// `ImportTool`: the content of resources and remote URLs.
pub struct ImportTool {
    safe: SafeConfig,
    me: Weak<ImportTool>,
}

impl ImportTool {
    /// `new ImportTool()`.
    pub fn new() -> Arc<ImportTool> {
        Arc::new_cyclic(|me| ImportTool { safe: SafeConfig::default(), me: me.clone() })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: nothing once the configuration is locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    /// `read(resource)`: `null` for nothing (or nothing found).
    pub fn read(&self, resource: Option<&str>) -> Option<String> {
        let r = resource.filter(|r| !r.is_empty())?;
        super::import::resource_string(r, IMPORT_TOOL)
    }

    /// `fetch(url)`: `null` for nothing or a failure (logged).
    pub fn fetch(&self, url: Option<&str>) -> Option<String> {
        let u = url.filter(|u| !u.is_empty())?;
        super::import::fetch(u)
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for ImportTool {
    fn safe(&self) -> &SafeConfig {
        &self.safe
    }
}

fn import_tool_def() -> Arc<ClassDef> {
    type I = ImportTool;
    let def = ClassDef::new(IMPORT_TOOL)
        .extends(&[safe::SAFE_CONFIG])
        .implements(&["java.io.Serializable"])
        .method::<I>("read", &[s()], Some(s()), |t, _, a| super::text(t.read(a[0].as_str())))
        .method::<I>("fetch", &[s()], Some(s()), |t, _, a| super::text(t.fetch(a[0].as_str())));
    super::object_methods::<I>(safe::safe_config_methods::<I>(def)).register()
}

tool_object!(ImportTool, import_tool_def);

pub(crate) fn import_tool_class() -> JavaClass {
    JavaClass::tool(import_tool_def(), || ImportTool::new().value()).annotated(Annotation::default_key("import")).annotated(Annotation::valid_scope(&[super::scope::REQUEST]))
}
