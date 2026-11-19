//! `ToolInfo`, `Toolbox`, `ToolboxFactory`, `ToolContext` and `ToolManager`: tools created (lazily, per
//! context or once per application) from a [`FactoryConfiguration`] and handed to templates.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use java_compat::lang::{JArray, JError, JList, JMap, JObject, JValue, ListKind, MapKind};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;

use super::classes::{self, JavaClass};
use super::configuration::FactoryConfiguration;
use super::{c, o, s, val};

/// The tool property holding the current request path, which tool path restrictions check.
pub const PATH_KEY: &str = "requestPath";
/// The tool property holding the `ToolContext` itself.
pub const CONTEXT_KEY: &str = "velocityContext";
/// The tool property holding the `VelocityEngine`.
pub const ENGINE_KEY: &str = "velocityEngine";
/// The tool property holding the locale.
pub const LOCALE_KEY: &str = "locale";
/// The tool property holding the log (here, its name).
pub const LOG_KEY: &str = "log";
/// The tool property holding the tool's own key.
pub const TOOLKEY_KEY: &str = "key";
/// The tool property telling tools whether to catch their exceptions.
pub const CATCH_EXCEPTIONS_KEY: &str = "catchExceptions";
/// `ToolboxFactory.DEFAULT_SCOPE`.
pub const DEFAULT_SCOPE: &str = super::scope::REQUEST;
/// The method a tool is configured through, with a `Map` of properties.
pub const CONFIGURE_METHOD_NAME: &str = "configure";

fn illegal_state(m: String) -> JError {
    JError::new("java.lang.IllegalStateException", m)
}

// ---------------------------------------------------------------------------------------------------------
// ToolInfo

/// `ToolInfo`: how to create and configure one tool.
#[derive(Clone)]
pub struct ToolInfo {
    key: String,
    class: Arc<JavaClass>,
    factory: Option<Arc<JavaClass>>,
    restrict_to: Option<String>,
    restrict_exact: bool,
    properties: Option<JMap>,
    skip_setters: Option<bool>,
}

impl ToolInfo {
    /// A tool info for `class` under `key`, created through `factory`'s static method when given; no restriction or properties.
    pub fn new(key: &str, class: Arc<JavaClass>, factory: Option<Arc<JavaClass>>) -> ToolInfo {
        ToolInfo { key: key.to_string(), class, factory, restrict_to: None, restrict_exact: false, properties: None, skip_setters: None }
    }

    /// `restrictTo(path)`: `*` for everything, a trailing `*` for a prefix, else an exact path.
    pub fn restrict_to(&mut self, path: Option<&str>) {
        let path = path.map(|p| if p.starts_with('/') { p.to_string() } else { format!("/{p}") });
        match path {
            None => {
                self.restrict_exact = false;
                self.restrict_to = None;
            }
            Some(p) if p == "/*" || p == "*" => {
                self.restrict_exact = false;
                self.restrict_to = None;
            }
            Some(p) if p.ends_with('*') => {
                self.restrict_exact = false;
                self.restrict_to = Some(p[..p.len() - 1].to_string());
            }
            Some(p) => {
                self.restrict_exact = true;
                self.restrict_to = Some(p);
            }
        }
    }

    /// `setSkipSetters(skip)`: whether properties are not passed to the tool's setters.
    pub fn set_skip_setters(&mut self, skip: bool) {
        self.skip_setters = Some(skip);
    }

    fn props(&mut self) -> &JMap {
        self.properties.get_or_insert_with(|| JMap::new(MapKind::Hash))
    }

    /// `addProperties(parentProps)`: those not already set.
    pub fn add_properties(&mut self, parent: &JMap) {
        let props = self.props().clone();
        for (k, v) in parent.entries() {
            if !props.contains_key(&k) {
                props.put(k, v);
            }
        }
    }

    /// `putProperty(name, value)`: the previous value, `Null` when there was none.
    pub fn put_property(&mut self, name: &str, value: JValue) -> JValue {
        self.props().put(JValue::str(name), value)
    }

    /// `getKey()`.
    pub fn get_key(&self) -> &str {
        &self.key
    }

    /// `getClassname()`: the tool class's name.
    pub fn get_classname(&self) -> &str {
        &self.class.name
    }

    /// `getToolClass()`.
    pub fn get_tool_class(&self) -> &Arc<JavaClass> {
        &self.class
    }

    /// `getProperties()`: the configured properties, the shared map itself.
    pub fn get_properties(&mut self) -> JMap {
        self.props().clone()
    }

    /// `isSkipSetters()`: `@SkipSetters` unless set.
    pub fn is_skip_setters(&self) -> bool {
        self.skip_setters.unwrap_or_else(|| self.class.annotation(classes::SKIP_SETTERS).is_some())
    }

    /// `hasPermission(path)`.
    pub fn has_permission(&self, path: Option<&str>) -> bool {
        match (&self.restrict_to, path) {
            (None, _) => true,
            (Some(r), Some(p)) if self.restrict_exact => r == p,
            (Some(r), Some(p)) => p.starts_with(r.as_str()),
            (Some(_), None) => false,
        }
    }

    /// `create(dynamicProperties)`: a new instance, its setters called and `configure(Map)` with the
    /// dynamic properties overridden by the configured ones.
    pub fn create(&self, dynamic: Option<&JMap>) -> Result<JValue, JError> {
        let tool = self.new_instance()?;
        let props = match &self.properties {
            None => dynamic.cloned(),
            Some(p) => {
                let combined = JMap::new(MapKind::Hash);
                for m in [dynamic, Some(p)].into_iter().flatten() {
                    for (k, v) in m.entries() {
                        combined.put(k, v);
                    }
                }
                Some(combined)
            }
        };
        self.configure(&tool, props.as_ref())?;
        Ok(tool)
    }

    fn configure(&self, tool: &JValue, props: Option<&JMap>) -> Result<(), JError> {
        if let (false, Some(p)) = (self.is_skip_setters(), props) {
            for (k, v) in p.entries() {
                set_property(tool, &k.to_java_string(), v)?;
            }
        }
        if classes::find_method(&self.class, CONFIGURE_METHOD_NAME, &[c("java.util.Map")]).is_some() {
            let arg = JValue::Map(props.cloned().unwrap_or_else(|| JMap::new(MapKind::Hash)));
            if let Some(Err(e)) = crate::introspect::call(tool, CONFIGURE_METHOD_NAME, &[arg]) {
                return Err(JError::new("java.lang.RuntimeException", format!("Exception when invoking public void {}.configure(java.util.Map) on {}: {e}", self.class.name, tool.to_java_string())));
            }
        }
        Ok(())
    }

    fn new_instance(&self) -> Result<JValue, JError> {
        let name = &self.class.name;
        match &self.factory {
            None => self.class.new_instance().map_err(|e| illegal_state(format!("Exception while instantiating instance of \"{name}\": {e}"))),
            Some(f) => {
                let m = classes::find_factory_method(f, &self.class).and_then(|m| m.invoke).ok_or_else(|| JError::without_message("java.lang.NullPointerException"))?;
                Ok(m(&[]).map_err(|e| illegal_state(format!("Exception while instantiating instance of \"{name}\": {e}")))?.unwrap_or(JValue::Null))
            }
        }
    }
}

/// `PropertyUtils.setProperty(tool, name, value)` when the property is writeable: a public one-argument
/// `set<Name>` method; a value its parameter cannot take is an `IllegalArgumentException`.
fn set_property(tool: &JValue, name: &str, value: JValue) -> Result<(), JError> {
    let Some(def) = java_compat::reflect::class_def(tool) else { return Ok(()) };
    let mut chars = name.chars();
    let Some(first) = chars.next() else { return Ok(()) };
    let setter = format!("set{}{}", first.to_uppercase(), chars.as_str());
    let Some(m) = def.methods.iter().find(|m| *m.name == setter && m.params.len() == 1 && m.returns.is_none() && !m.is_static) else { return Ok(()) };
    let actual = java_compat::reflect::class_of(&value);
    if !value.is_null() && !crate::uberspect::is_convertible(&m.params[0], actual.as_ref(), false) {
        return Err(JError::new(
            "java.lang.IllegalArgumentException",
            format!("Cannot invoke {}.{setter} on bean class '{}' - argument type mismatch - had objects of type \"{}\" but expected signature \"{}\"", m.declaring_class, def.name, value.class_name(), m.params[0].type_name()),
        ));
    }
    match crate::uberspect::invoke(m, tool, &[value]) {
        Ok(_) => Ok(()),
        Err(crate::uberspect::CallError::Threw(e)) | Err(crate::uberspect::CallError::IllegalArgument(e)) => Err(e),
    }
}

// ---------------------------------------------------------------------------------------------------------
// Toolbox

/// `Toolbox`: the tools of one scope, created on first use and cached.
pub struct Toolbox {
    info: HashMap<String, ToolInfo>,
    properties: Option<JMap>,
    cache: Mutex<Option<HashMap<String, JValue>>>,
}

impl Toolbox {
    /// A toolbox of these tool infos and toolbox properties, with nothing created yet.
    pub fn new(info: HashMap<String, ToolInfo>, properties: Option<JMap>) -> Toolbox {
        Toolbox { info, properties, cache: Mutex::new(None) }
    }

    fn cache_data(&self, data: &HashMap<String, JValue>) {
        if !data.is_empty() {
            *self.cache.lock() = Some(data.clone());
        }
    }

    /// `getProperties()`: the toolbox's properties, `None` without any.
    pub fn get_properties(&self) -> Option<JMap> {
        self.properties.clone()
    }

    /// `get(key, path, context)`: cached, else created (when the path is permitted).
    pub fn get(&self, key: &str, path: Option<&str>, context: Option<&JMap>) -> Result<JValue, JError> {
        let mut cache = self.cache.lock();
        if let Some(t) = cache.as_ref().and_then(|c| c.get(key)).filter(|t| !t.is_null()) {
            if path.is_none() || self.info.get(key).is_none_or(|i| i.has_permission(path)) {
                return Ok(t.clone());
            }
            return Ok(JValue::Null);
        }
        match self.info.get(key) {
            Some(info) if path.is_none() || info.has_permission(path) => {
                let tool = info.create(context)?;
                cache.get_or_insert_with(HashMap::new).insert(key.to_string(), tool.clone());
                Ok(tool)
            }
            _ => Ok(JValue::Null),
        }
    }

    /// `getKeys()`: the configured tools and the cached data.
    pub fn get_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.info.keys().cloned().collect();
        if let Some(c) = self.cache.lock().as_ref() {
            for k in c.keys() {
                if !keys.contains(k) {
                    keys.push(k.clone());
                }
            }
        }
        keys
    }

    /// `getToolClassMap()`.
    pub fn get_tool_class_map(&self) -> HashMap<String, Arc<JavaClass>> {
        self.info.iter().map(|(k, i)| (k.clone(), i.class.clone())).collect()
    }

    /// `getAll(context)`: every tool created.
    pub fn get_all(&self, context: Option<&JMap>) -> Result<HashMap<String, JValue>, JError> {
        for key in self.info.keys() {
            self.get(key, None, context)?;
        }
        Ok(self.cache.lock().clone().unwrap_or_default())
    }
}

// ---------------------------------------------------------------------------------------------------------
// ToolboxFactory

/// `ToolboxFactory`: tool infos and properties by scope, and the configured data.
#[derive(Default, Clone)]
pub struct ToolboxFactory {
    scoped_info: HashMap<String, HashMap<String, ToolInfo>>,
    scoped_properties: HashMap<String, JMap>,
    data: Option<HashMap<String, JValue>>,
    global_properties: Option<JMap>,
}

impl ToolboxFactory {
    /// A factory with no tools, data or properties.
    pub fn new() -> ToolboxFactory {
        ToolboxFactory::default()
    }

    /// `configure(config)`: validates, then records data, tools and properties (tool properties first,
    /// then the toolbox's, then the factory's).
    pub fn configure(&mut self, config: &FactoryConfiguration) -> Result<(), JError> {
        config.validate()?;
        for d in config.data() {
            self.data.get_or_insert_with(HashMap::new).insert(d.get_key().unwrap_or("null").to_string(), d.get_converted_value()?);
        }
        for toolbox in config.get_toolboxes() {
            let scope = toolbox.get_scope().to_string();
            for tool in toolbox.get_tools() {
                let info = tool.create_info()?;
                self.scoped_info.entry(scope.clone()).or_default().insert(info.get_key().to_string(), info);
            }
            let props = toolbox.config.get_property_map()?;
            if !props.is_empty() {
                let existing = self.scoped_properties.entry(scope.clone()).or_insert_with(|| JMap::new(MapKind::Hash));
                for (k, v) in props.entries() {
                    existing.put(k, v);
                }
            }
            for info in self.scoped_info.entry(scope).or_default().values_mut() {
                info.add_properties(&props);
            }
        }
        let global = config.config.get_property_map()?;
        if !global.is_empty() {
            let g = self.global_properties.get_or_insert_with(|| JMap::new(MapKind::Hash));
            for (k, v) in global.entries() {
                g.put(k, v);
            }
        }
        for infos in self.scoped_info.values_mut() {
            for info in infos.values_mut() {
                info.add_properties(&global);
            }
        }
        Ok(())
    }

    /// `getGlobalProperty(name)`: a factory property, `Null` when unset.
    pub fn get_global_property(&self, name: &str) -> JValue {
        self.global_properties.as_ref().and_then(|g| g.get_str(name)).unwrap_or(JValue::Null)
    }

    /// `getData()`: the converted configuration data by key, `None` without any.
    pub fn get_data(&self) -> Option<&HashMap<String, JValue>> {
        self.data.as_ref()
    }

    /// `hasTools(scope)`: tools, or data for the application scope.
    pub fn has_tools(&self, scope: &str) -> bool {
        self.scoped_info.get(scope).is_some_and(|t| !t.is_empty()) || (self.data.is_some() && scope == super::scope::APPLICATION)
    }

    /// `createToolbox(scope)`.
    pub fn create_toolbox(&mut self, scope: &str) -> Toolbox {
        let tools = self.scoped_info.get(scope).cloned().unwrap_or_default();
        let properties = match self.scoped_properties.get(scope) {
            None => self.global_properties.clone(),
            Some(p) => {
                if let Some(g) = &self.global_properties {
                    for (k, v) in g.entries() {
                        p.put(k, v);
                    }
                }
                Some(p.clone())
            }
        };
        let toolbox = Toolbox::new(tools, properties);
        if let Some(d) = &self.data {
            if self.scoped_info.len() == 1 || scope == super::scope::APPLICATION {
                toolbox.cache_data(d);
            }
        }
        toolbox
    }
}

// ---------------------------------------------------------------------------------------------------------
// ToolContext

/// `ToolContext`: a Velocity context whose values are tools (from its toolboxes) and local puts.
pub struct ToolContext {
    toolboxes: Mutex<Vec<Arc<Toolbox>>>,
    tool_props: JMap,
    local: Mutex<HashMap<String, JValue>>,
    user_overwrite: AtomicBool,
    me: Weak<ToolContext>,
}

impl ToolContext {
    /// `new ToolContext(toolProps)`: the context itself is the `velocityContext` property.
    pub fn new(tool_props: Option<&JMap>) -> Arc<ToolContext> {
        let ctx = Arc::new_cyclic(|me: &Weak<ToolContext>| ToolContext {
            toolboxes: Mutex::new(Vec::new()),
            tool_props: JMap::new(MapKind::Hash),
            local: Mutex::new(HashMap::new()),
            user_overwrite: AtomicBool::new(true),
            me: me.clone(),
        });
        ctx.put_tool_property(CONTEXT_KEY, ctx.value());
        if let Some(p) = tool_props {
            for (k, v) in p.entries() {
                ctx.tool_props.put(k, v);
            }
        }
        ctx
    }

    /// The context as a value (a weak handle, so tools keeping it do not keep it alive).
    pub fn value(&self) -> JValue {
        JValue::Object(Arc::new(ContextRef(self.me.clone())) as Arc<dyn JObject>)
    }

    /// `setUserCanOverwriteTools(overwrite)`: whether a local value hides a tool of the same key.
    pub fn set_user_can_overwrite_tools(&self, overwrite: bool) {
        self.user_overwrite.store(overwrite, Ordering::SeqCst);
    }

    /// `getUserCanOverwriteTools()`.
    pub fn get_user_can_overwrite_tools(&self) -> bool {
        self.user_overwrite.load(Ordering::SeqCst)
    }

    /// `addToolbox(toolbox)`: searched after those already added.
    pub fn add_toolbox(&self, toolbox: Arc<Toolbox>) {
        self.toolboxes.lock().push(toolbox);
    }

    /// `getToolbox()`: every tool, created.
    pub fn get_toolbox(&self) -> Result<HashMap<String, JValue>, JError> {
        let mut all = HashMap::new();
        for t in self.toolboxes.lock().clone() {
            all.extend(t.get_all(Some(&self.tool_props))?);
        }
        Ok(all)
    }

    /// `getToolClassMap()`: every toolbox's tool classes by key, earlier toolboxes winning.
    pub fn get_tool_class_map(&self) -> HashMap<String, Arc<JavaClass>> {
        let mut out = HashMap::new();
        for t in self.toolboxes.lock().iter().rev() {
            out.extend(t.get_tool_class_map());
        }
        out
    }

    /// `putVelocityEngine(engine)`: the engine, its log, and `catchExceptions=false` when it has a method
    /// exception handler.
    pub fn put_velocity_engine(&self, engine: &Arc<crate::Engine>) {
        self.put_tool_property(ENGINE_KEY, engine_value(engine));
        self.put_tool_property(LOG_KEY, JValue::str("org.apache.velocity.tools"));
        if engine.config.props.get(crate::config::EVENTHANDLER_METHODEXCEPTION).is_some() {
            self.put_tool_property(CATCH_EXCEPTIONS_KEY, JValue::Bool(false));
        }
    }

    /// `putToolProperty(key, value)`: the previous value, `Null` when there was none.
    pub fn put_tool_property(&self, key: &str, value: JValue) -> JValue {
        self.tool_props.put(JValue::str(key), value)
    }

    /// `putToolProperties(props)`.
    pub fn put_tool_properties(&self, props: &JMap) {
        for (k, v) in props.entries() {
            self.tool_props.put(k, v);
        }
    }

    /// The tool properties tools are created with, the shared map itself.
    pub fn tool_properties(&self) -> JMap {
        self.tool_props.clone()
    }

    /// `put(key, value)`: a local value; the previous one, `Null` when there was none.
    pub fn put(&self, key: &str, value: JValue) -> JValue {
        self.local.lock().insert(key.to_string(), value).unwrap_or(JValue::Null)
    }

    /// `get(key)`: a local value before a tool, unless users may not overwrite tools.
    pub fn get(&self, key: &str) -> Result<JValue, JError> {
        let overwrite = self.get_user_can_overwrite_tools();
        let v = if overwrite { self.internal_get(key) } else { self.find_tool(key)? };
        if !v.is_null() {
            return Ok(v);
        }
        if overwrite { self.find_tool(key) } else { Ok(self.internal_get(key)) }
    }

    fn internal_get(&self, key: &str) -> JValue {
        self.local.lock().get(key).cloned().unwrap_or(JValue::Null)
    }

    fn find_tool(&self, key: &str) -> Result<JValue, JError> {
        let path = self.tool_props.get_str(PATH_KEY).and_then(|p| p.as_str().map(str::to_string));
        for t in self.toolboxes.lock().clone() {
            let tool = t.get(key, path.as_deref(), Some(&self.tool_props))?;
            if !tool.is_null() {
                return Ok(tool);
            }
        }
        Ok(JValue::Null)
    }

    /// `keySet()`: every toolbox's keys and the local ones.
    pub fn key_set(&self) -> Vec<String> {
        let mut keys = Vec::new();
        for t in self.toolboxes.lock().iter() {
            for k in t.get_keys() {
                if !keys.contains(&k) {
                    keys.push(k);
                }
            }
        }
        for k in self.local.lock().keys() {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
        keys
    }

    /// `containsKey(key)`: among the tool keys and local keys.
    pub fn contains_key(&self, key: &str) -> bool {
        self.key_set().iter().any(|k| k == key)
    }

    /// `remove(key)`: a local value removed, `Null` when there was none; tools stay.
    pub fn remove(&self, key: &str) -> JValue {
        self.local.lock().remove(key).unwrap_or(JValue::Null)
    }

    /// `putAll(map)`: local values, keyed by the keys' string form.
    pub fn put_all(&self, map: &JMap) {
        let mut l = self.local.lock();
        for (k, v) in map.entries() {
            l.insert(k.to_java_string(), v);
        }
    }
}

/// A `ToolContext` is a layer of a Velocity context: its tools and local values.
impl crate::Layer for ToolContext {
    fn get(&self, key: &str) -> Option<JValue> {
        ToolContext::get(self, key).ok().filter(|v| !v.is_null())
    }
}

/// A `ToolContext` value (`org.apache.velocity.context.Context`).
pub struct ContextRef(Weak<ToolContext>);

/// The Java class name of `ToolContext`.
pub const TOOL_CONTEXT: &str = "org.apache.velocity.tools.ToolContext";

impl ContextRef {
    /// The context, `None` once it is dropped.
    pub fn context(&self) -> Option<Arc<ToolContext>> {
        self.0.upgrade()
    }
}

/// The `ToolContext` behind a context value.
pub fn as_tool_context(v: &JValue) -> Option<Arc<ToolContext>> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<ContextRef>().and_then(ContextRef::context),
        _ => None,
    }
}

fn with_ctx<F: Fn(&ToolContext, &[JValue]) -> super::Ret + Send + Sync + 'static>(f: F) -> impl Fn(&ContextRef, &JValue, &[JValue]) -> super::Ret + Send + Sync {
    move |r, _, a| match r.context() {
        Some(ctx) => f(&ctx, a),
        None => Err(super::npe()),
    }
}

fn context_def() -> Arc<ClassDef> {
    ClassDef::new("org.apache.velocity.context.Context").interface().register();
    type C = ContextRef;
    fn key(v: &JValue) -> String {
        v.to_java_string()
    }
    ClassDef::new(TOOL_CONTEXT)
        .implements(&["org.apache.velocity.context.Context"])
        .method::<C>("get", &[s()], Some(o()), with_ctx(|c, a| val(c.get(&key(&a[0]))?)))
        .method::<C>("put", &[s(), o()], Some(o()), with_ctx(|c, a| val(c.put(&key(&a[0]), a[1].clone()))))
        .method::<C>("containsKey", &[s()], Some(JType::Boolean), with_ctx(|c, a| val(JValue::Bool(c.contains_key(&key(&a[0]))))))
        .method::<C>("getKeys", &[], Some(JType::array(s())), with_ctx(|c, _| val(JArray::strings(c.key_set()))))
        .method::<C>("keySet", &[], Some(c("java.util.Set")), with_ctx(|c, _| val(JValue::List(JList::new(ListKind::Set, c.key_set().into_iter().map(JValue::str).collect())))))
        .method::<C>("remove", &[s()], Some(o()), with_ctx(|c, a| val(c.remove(&key(&a[0])))))
        .method::<C>("getToolbox", &[], Some(c("java.util.Map")), with_ctx(|c, _| val(JValue::Map(JMap::from_entries(MapKind::Hash, c.get_toolbox()?.into_iter().map(|(k, v)| (JValue::str(k), v)))))))
        .method::<C>("getUserCanOverwriteTools", &[], Some(JType::Boolean), with_ctx(|c, _| val(JValue::Bool(c.get_user_can_overwrite_tools()))))
        .method::<C>("setUserCanOverwriteTools", &[JType::Boolean], None, with_ctx(|c, a| {
            c.set_user_can_overwrite_tools(matches!(a[0], JValue::Bool(true)));
            Ok(None)
        }))
        .method::<C>("putToolProperty", &[s(), o()], Some(o()), with_ctx(|c, a| val(c.put_tool_property(&key(&a[0]), a[1].clone()))))
        .method::<C>("toString", &[], Some(s()), |r, _, _| val(JValue::str(java_compat::reflect::identity_string(r))))
        .register()
}

impl JObject for ContextRef {
    fn class_name(&self) -> String {
        TOOL_CONTEXT.into()
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<super::Ret> {
        None
    }

    fn to_java_string(&self) -> String {
        java_compat::reflect::identity_string(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<ClassDef>> {
        static DEF: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(context_def);
        Some(DEF.get())
    }
}

/// A `VelocityEngine` as a Java object (the `velocityEngine` tool property).
pub struct EngineValue(pub Arc<crate::Engine>);

/// The Java class name of `VelocityEngine`.
pub const VELOCITY_ENGINE: &str = "org.apache.velocity.app.VelocityEngine";

impl JObject for EngineValue {
    fn class_name(&self) -> String {
        VELOCITY_ENGINE.into()
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<super::Ret> {
        None
    }

    fn to_java_string(&self) -> String {
        java_compat::reflect::identity_string(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<ClassDef>> {
        static DEF: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(|| ClassDef::new(VELOCITY_ENGINE).register());
        Some(DEF.get())
    }
}

/// The engine as a template value.
pub fn engine_value(engine: &Arc<crate::Engine>) -> JValue {
    JValue::Object(Arc::new(EngineValue(engine.clone())) as Arc<dyn JObject>)
}

/// The engine behind an engine value.
pub fn as_engine(v: &JValue) -> Option<Arc<crate::Engine>> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<EngineValue>().map(|e| e.0.clone()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------------------
// ToolManager

/// `ToolManager`: a factory configured once; application tools shared, request tools per context.
pub struct ToolManager {
    factory: Mutex<ToolboxFactory>,
    velocity: Mutex<Option<Arc<crate::Engine>>>,
    application: Mutex<Option<Arc<Toolbox>>>,
    user_overwrite: AtomicBool,
}

impl ToolManager {
    /// `new ToolManager(autoConfig, includeDefaults)`: auto-configuration reads the system property only.
    pub fn new(auto_config: bool, _include_defaults: bool) -> Result<ToolManager, JError> {
        let m = ToolManager { factory: Mutex::new(ToolboxFactory::new()), velocity: Mutex::new(None), application: Mutex::new(None), user_overwrite: AtomicBool::new(true) };
        if auto_config {
            if let Some(sys) = super::configuration::find_from_system_property()? {
                m.configure(&sys)?;
            }
        }
        Ok(m)
    }

    /// `configure(config)`: the configuration validated and taken in; the application toolbox is created anew on next use.
    pub fn configure(&self, config: &FactoryConfiguration) -> Result<(), JError> {
        *self.application.lock() = None;
        self.factory.lock().configure(config)
    }

    /// `configure(path)`: `RuntimeException` when nothing is found there.
    pub fn configure_path(&self, path: &str) -> Result<(), JError> {
        match super::configuration::find(path)? {
            Some(c) => self.configure(&c),
            None => Err(JError::new("java.lang.RuntimeException", format!("Could not find any configuration at {path}"))),
        }
    }

    /// `setVelocityEngine(engine)`: the engine new contexts get as their `velocityEngine` property.
    pub fn set_velocity_engine(&self, engine: Option<Arc<crate::Engine>>) {
        *self.velocity.lock() = engine;
    }

    /// `getVelocityEngine()`: `None` when unset.
    pub fn get_velocity_engine(&self) -> Option<Arc<crate::Engine>> {
        self.velocity.lock().clone()
    }

    /// `setUserCanOverwriteTools(overwrite)`: the setting new contexts get.
    pub fn set_user_can_overwrite_tools(&self, o: bool) {
        self.user_overwrite.store(o, Ordering::SeqCst);
    }

    /// `getUserCanOverwriteTools()`.
    pub fn get_user_can_overwrite_tools(&self) -> bool {
        self.user_overwrite.load(Ordering::SeqCst)
    }

    /// `createContext(toolProps)`.
    pub fn create_context(&self, tool_props: Option<&JMap>) -> Arc<ToolContext> {
        let ctx = ToolContext::new(tool_props);
        ctx.set_user_can_overwrite_tools(self.get_user_can_overwrite_tools());
        if let Some(e) = self.get_velocity_engine() {
            ctx.put_velocity_engine(&e);
        }
        if self.has_application_tools() {
            if let Some(a) = self.get_application_toolbox() {
                ctx.add_toolbox(a);
            }
        }
        if self.has_request_tools() {
            ctx.add_toolbox(Arc::new(self.factory.lock().create_toolbox(super::scope::REQUEST)));
        }
        ctx
    }

    /// `hasRequestTools()`: whether the `request` scope has tools.
    pub fn has_request_tools(&self) -> bool {
        self.factory.lock().has_tools(super::scope::REQUEST)
    }

    /// `hasApplicationTools()`: whether the `application` scope has tools or there is configuration data.
    pub fn has_application_tools(&self) -> bool {
        self.factory.lock().has_tools(super::scope::APPLICATION)
    }

    /// `getApplicationToolbox()`: created once.
    pub fn get_application_toolbox(&self) -> Option<Arc<Toolbox>> {
        let mut app = self.application.lock();
        if app.is_none() && self.has_application_tools() {
            *app = Some(Arc::new(self.factory.lock().create_toolbox(super::scope::APPLICATION)));
        }
        app.clone()
    }

    /// `getRequestToolbox()`: a new toolbox of the `request` scope's tools.
    pub fn get_request_toolbox(&self) -> Toolbox {
        self.factory.lock().create_toolbox(super::scope::REQUEST)
    }
}
