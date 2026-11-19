//! `org.apache.velocity.app.FieldMethodizer`: a class's public static fields as `$obj.get('NAME')` / `$obj.NAME`.

use std::collections::HashMap;
use std::sync::Arc;

use java_compat::lang::{JError, JObject, JValue};
use java_compat::reflect::{ClassDef, JType, LazyClass};

/// The fields it was given, by name.
#[derive(Default)]
pub struct FieldMethodizer {
    fields: HashMap<String, JValue>,
}

impl FieldMethodizer {
    /// `new FieldMethodizer()`: no fields.
    pub fn new() -> FieldMethodizer {
        FieldMethodizer::default()
    }

    /// `addObject`: the public static fields of a class, as name and value.
    pub fn add_fields<'a>(&mut self, fields: impl IntoIterator<Item = (&'a str, JValue)>) {
        self.fields.extend(fields.into_iter().map(|(k, v)| (k.to_string(), v)));
    }

    /// `new FieldMethodizer("org.apache.velocity.runtime.RuntimeSingleton")`: `RuntimeConstants`' fields.
    pub fn runtime_singleton() -> FieldMethodizer {
        let mut f = FieldMethodizer::new();
        f.add_fields(RUNTIME_CONSTANTS.iter().cloned());
        f
    }

    /// `get(fieldName)`: null for an unknown field.
    pub fn get(&self, name: &str) -> JValue {
        self.fields.get(name).cloned().unwrap_or(JValue::Null)
    }

    /// The methodizer as a context value.
    pub fn value(self) -> JValue {
        JValue::Object(Arc::new(self))
    }
}

fn def() -> Arc<ClassDef> {
    ClassDef::new("org.apache.velocity.app.FieldMethodizer")
        .method::<FieldMethodizer>("get", &[JType::string()], Some(JType::object()), |f, _, a| Ok(Some(f.get(&a[0].to_java_string()))))
        .register()
}

impl JObject for FieldMethodizer {
    fn class_name(&self) -> String {
        "org.apache.velocity.app.FieldMethodizer".into()
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<Result<Option<JValue>, JError>> {
        None
    }

    fn to_java_string(&self) -> String {
        java_compat::reflect::identity_string(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<ClassDef>> {
        static DEF: LazyClass = LazyClass::new(def);
        Some(DEF.get())
    }
}

/// `RuntimeConstants`' constant fields (`RuntimeSingleton` implements it).
static RUNTIME_CONSTANTS: std::sync::LazyLock<Vec<(&'static str, JValue)>> = std::sync::LazyLock::new(|| {
    vec![
        ("RUNTIME_LOG_INSTANCE", JValue::str("runtime.log.instance")),
        ("RUNTIME_LOG_NAME", JValue::str("runtime.log.name")),
        ("RUNTIME_LOG_REFERENCE_LOG_INVALID", JValue::str("runtime.log.log_invalid_references")),
        ("RUNTIME_LOG_METHOD_CALL_LOG_INVALID", JValue::str("runtime.log.log_invalid_method_calls")),
        ("RUNTIME_LOG_TRACK_LOCATION", JValue::str("runtime.log.track_location")),
        ("MAX_NUMBER_LOOPS", JValue::str("directive.foreach.max_loops")),
        ("SKIP_INVALID_ITERATOR", JValue::str("directive.foreach.skip_invalid")),
        ("CHECK_EMPTY_OBJECTS", JValue::str("directive.if.empty_check")),
        ("ERRORMSG_START", JValue::str("directive.include.output_error_start")),
        ("ERRORMSG_END", JValue::str("directive.include.output_error_end")),
        ("PARSE_DIRECTIVE_MAXDEPTH", JValue::str("directive.parse.max_depth")),
        ("DEFINE_DIRECTIVE_MAXDEPTH", JValue::str("directive.define.max_depth")),
        ("PROVIDE_SCOPE_CONTROL", JValue::str("provide.scope.control")),
        ("CONTEXT_SCOPE_CONTROL", JValue::str("context.scope_control")),
        ("CUSTOM_DIRECTIVES", JValue::str("runtime.custom_directives")),
        ("RESOURCE_MANAGER_INSTANCE", JValue::str("resource.manager.instance")),
        ("RESOURCE_MANAGER_CLASS", JValue::str("resource.manager.class")),
        ("RESOURCE_MANAGER_CACHE_CLASS", JValue::str("resource.manager.cache.class")),
        ("RESOURCE_MANAGER_DEFAULTCACHE_SIZE", JValue::str("resource.manager.cache.default_size")),
        ("RESOURCE_MANAGER_LOGWHENFOUND", JValue::str("resource.manager.log_when_found")),
        ("RESOURCE_LOADERS", JValue::str("resource.loaders")),
        ("RESOURCE_LOADER", JValue::str("resource.loader")),
        ("FILE_RESOURCE_LOADER_PATH", JValue::str("resource.loader.file.path")),
        ("FILE_RESOURCE_LOADER_CACHE", JValue::str("resource.loader.file.cache")),
        ("RESOURCE_LOADER_CLASS", JValue::str("class")),
        ("RESOURCE_LOADER_INSTANCE", JValue::str("instance")),
        ("RESOURCE_LOADER_CACHE", JValue::str("cache")),
        ("RESOURCE_LOADER_PATHS", JValue::str("path")),
        ("RESOURCE_LOADER_CHECK_INTERVAL", JValue::str("modification_check_interval")),
        ("DS_RESOURCE_LOADER_DATASOURCE_URL", JValue::str("resource.loader.ds.resource.datasource_url")),
        ("DS_RESOURCE_LOADER_TABLE", JValue::str("resource.loader.ds.resource.table")),
        ("DS_RESOURCE_LOADER_KEY_COLUMN", JValue::str("resource.loader.ds.resource.key_column")),
        ("DS_RESOURCE_LOADER_TEMPLATE_COLUMN", JValue::str("resource.loader.ds.resource.template_column")),
        ("DS_RESOURCE_LOADER_TIMESTAMP_COLUMN", JValue::str("resource.loader.ds.resource.timestamp_column")),
        ("DS_RESOURCE_LOADER_STMT_POOL_MAX_SIZE", JValue::str("resource.loader.ds.statements_pool_max_size")),
        ("INPUT_ENCODING", JValue::str("resource.default_encoding")),
        ("ENCODING_DEFAULT", JValue::str("UTF-8")),
        ("EVENTHANDLER_REFERENCEINSERTION", JValue::str("event_handler.reference_insertion.class")),
        ("EVENTHANDLER_METHODEXCEPTION", JValue::str("event_handler.method_exception.class")),
        ("EVENTHANDLER_INCLUDE", JValue::str("event_handler.include.class")),
        ("EVENTHANDLER_INVALIDREFERENCES", JValue::str("event_handler.invalid_references.class")),
        ("EVENTHANDLER_INVALIDREFERENCES_QUIET", JValue::str("event_handler.invalid_references.quiet")),
        ("EVENTHANDLER_INVALIDREFERENCES_NULL", JValue::str("event_handler.invalid_references.null")),
        ("EVENTHANDLER_INVALIDREFERENCES_TESTED", JValue::str("event_handler.invalid_references.tested")),
        ("VM_LIBRARY", JValue::str("velocimacro.library.path")),
        ("VM_LIBRARY_DEFAULT", JValue::str("velocimacros.vtl")),
        ("VM_LIBRARY_AUTORELOAD", JValue::str("velocimacro.library.autoreload")),
        ("VM_PERM_ALLOW_INLINE", JValue::str("velocimacro.inline.allow")),
        ("VM_PERM_ALLOW_INLINE_REPLACE_GLOBAL", JValue::str("velocimacro.inline.replace_global")),
        ("VM_PERM_INLINE_LOCAL", JValue::str("velocimacro.inline.local_scope")),
        ("VM_ARGUMENTS_STRICT", JValue::str("velocimacro.arguments.strict")),
        ("VM_ENABLE_BC_MODE", JValue::str("velocimacro.enable_bc_mode")),
        ("VM_MAX_DEPTH", JValue::str("velocimacro.max_depth")),
        ("VM_BODY_REFERENCE", JValue::str("velocimacro.body_reference")),
        ("VM_MESSAGES_ON", JValue::str("velocimacro.messages.on")),
        ("RUNTIME_REFERENCES_STRICT", JValue::str("runtime.strict_mode.enable")),
        ("RUNTIME_REFERENCES_STRICT_ESCAPE", JValue::str("runtime.strict_mode.escape")),
        ("UBERSPECT_CLASSNAME", JValue::str("introspector.uberspect.class")),
        ("INTROSPECTOR_RESTRICT_PACKAGES", JValue::str("introspector.restrict.packages")),
        ("INTROSPECTOR_RESTRICT_CLASSES", JValue::str("introspector.restrict.classes")),
        ("CONVERSION_HANDLER_CLASS", JValue::str("introspector.conversion_handler.class")),
        ("CONVERSION_HANDLER_INSTANCE", JValue::str("introspector.conversion_handler.instance")),
        ("PARSER_CLASS", JValue::str("parser.class")),
        ("DEFAULT_PARSER_CLASS", JValue::str("org.apache.velocity.runtime.parser.StandardParser")),
        ("PARSER_POOL_CLASS", JValue::str("parser.pool.class")),
        ("PARSER_POOL_SIZE", JValue::str("parser.pool.size")),
        ("PARSER_HYPHEN_ALLOWED", JValue::str("parser.allow_hyphen_in_identifiers")),
        ("RUNTIME_STRING_INTERNING", JValue::str("runtime.string_interning")),
        ("INTERPOLATE_STRINGLITERALS", JValue::str("runtime.interpolate_string_literals")),
        ("IMMUTABLE_RANGES", JValue::str("runtime.immutable_ranges")),
        ("STRICT_MATH", JValue::str("runtime.strict_math")),
        ("CONTEXT_AUTOREFERENCE_KEY", JValue::str("context.self_reference_key")),
        ("SPACE_GOBBLING", JValue::str("parser.space_gobbling")),
        ("DEFAULT_RUNTIME_PROPERTIES", JValue::str("org/apache/velocity/runtime/defaults/velocity.properties")),
        ("DEFAULT_RUNTIME_DIRECTIVES", JValue::str("org/apache/velocity/runtime/defaults/directive.properties")),
        ("DEFAULT_RUNTIME_LOG_NAME", JValue::str("org.apache.velocity")),
        ("RESOURCE_LOADER_IDENTIFIER", JValue::str("_RESOURCE_LOADER_IDENTIFIER_")),
        ("NUMBER_OF_PARSERS", JValue::Int(20)),
    ]
});
