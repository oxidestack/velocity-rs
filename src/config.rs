//! `RuntimeConstants` and the settings `RuntimeInstance` (and the nodes' `init()`) read from the configuration:
//! Velocity's `velocity.properties` defaults combined with the application's overrides.

use std::collections::HashSet;

use crate::props::{ExtProperties, PropValue};

/// `RuntimeConstants.RUNTIME_LOG_INSTANCE`: a logger object to log to (see `log::RUNTIME_LOG_INSTANCE`).
pub const RUNTIME_LOG_INSTANCE: &str = "runtime.log.instance";
/// `RuntimeConstants.RUNTIME_LOG_NAME`: the name of the logger used when no instance is set.
pub const RUNTIME_LOG_NAME: &str = "runtime.log.name";
/// `RuntimeConstants.RUNTIME_LOG_REFERENCE_LOG_INVALID`: whether invalid references are logged (default `true`).
pub const RUNTIME_LOG_REFERENCE_LOG_INVALID: &str = "runtime.log.log_invalid_references";
/// `RuntimeConstants.RUNTIME_LOG_METHOD_CALL_LOG_INVALID`: whether calls to missing methods are logged (default `true`).
pub const RUNTIME_LOG_METHOD_CALL_LOG_INVALID: &str = "runtime.log.log_invalid_method_calls";
/// `RuntimeConstants.RUNTIME_LOG_TRACK_LOCATION`: whether the template location being rendered is tracked for log messages.
pub const RUNTIME_LOG_TRACK_LOCATION: &str = "runtime.log.track_location";
/// `RuntimeConstants.MAX_NUMBER_LOOPS`: the most iterations a `#foreach` runs; `-1` (the default) for no limit.
pub const MAX_NUMBER_LOOPS: &str = "directive.foreach.max_loops";
/// `RuntimeConstants.SKIP_INVALID_ITERATOR`: whether `#foreach` over a value it cannot iterate renders nothing (default)
/// rather than failing.
pub const SKIP_INVALID_ITERATOR: &str = "directive.foreach.skip_invalid";
/// `RuntimeConstants.CHECK_EMPTY_OBJECTS`: whether `#if` treats empty strings, collections, maps and arrays as false (default `true`).
pub const CHECK_EMPTY_OBJECTS: &str = "directive.if.empty_check";
/// `RuntimeConstants.ERRORMSG_START`: text written before the error message when an `#include` argument is invalid.
pub const ERRORMSG_START: &str = "directive.include.output_error_start";
/// `RuntimeConstants.ERRORMSG_END`: text written after the error message when an `#include` argument is invalid.
pub const ERRORMSG_END: &str = "directive.include.output_error_end";
/// `RuntimeConstants.PARSE_DIRECTIVE_MAXDEPTH`: how deep `#parse` may nest (default `10`).
pub const PARSE_DIRECTIVE_MAXDEPTH: &str = "directive.parse.max_depth";
/// `RuntimeConstants.DEFINE_DIRECTIVE_MAXDEPTH`: how deep a `#define` block may render itself recursively (default `2`).
pub const DEFINE_DIRECTIVE_MAXDEPTH: &str = "directive.define.max_depth";
/// `RuntimeConstants.CONTEXT_SCOPE_CONTROL`: the key prefix whose `.<scope>` booleans turn on the `$foreach`, `$template`,
/// `$macro`, `$define`, `$evaluate` and other scope references.
pub const CONTEXT_SCOPE_CONTROL: &str = "context.scope_control";
/// `RuntimeConstants.CUSTOM_DIRECTIVES`: the class names of user directives to register.
pub const CUSTOM_DIRECTIVES: &str = "runtime.custom_directives";
/// `RuntimeConstants.RESOURCE_MANAGER_INSTANCE`: a resource manager object to use instead of a class.
pub const RESOURCE_MANAGER_INSTANCE: &str = "resource.manager.instance";
/// `RuntimeConstants.RESOURCE_MANAGER_CLASS`: the resource manager's class name.
pub const RESOURCE_MANAGER_CLASS: &str = "resource.manager.class";
/// `RuntimeConstants.RESOURCE_MANAGER_CACHE_CLASS`: the resource cache's class name.
pub const RESOURCE_MANAGER_CACHE_CLASS: &str = "resource.manager.cache.class";
/// `RuntimeConstants.RESOURCE_MANAGER_DEFAULTCACHE_SIZE`: the number of resources the default cache keeps.
pub const RESOURCE_MANAGER_DEFAULTCACHE_SIZE: &str = "resource.manager.cache.default_size";
/// `RuntimeConstants.RESOURCE_MANAGER_LOGWHENFOUND`: whether the resource manager logs which loader found a resource.
pub const RESOURCE_MANAGER_LOGWHENFOUND: &str = "resource.manager.log_when_found";
/// `RuntimeConstants.RESOURCE_LOADERS`: the names of the resource loaders to use, in order.
pub const RESOURCE_LOADERS: &str = "resource.loaders";
/// `RuntimeConstants.RESOURCE_LOADER`: the prefix of each loader's own keys (`resource.loader.<name>.*`).
pub const RESOURCE_LOADER: &str = "resource.loader";
/// `RuntimeConstants.FILE_RESOURCE_LOADER_PATH`: the file resource loader's search path.
pub const FILE_RESOURCE_LOADER_PATH: &str = "resource.loader.file.path";
/// `RuntimeConstants.FILE_RESOURCE_LOADER_CACHE`: whether the file resource loader caches the templates it loads.
pub const FILE_RESOURCE_LOADER_CACHE: &str = "resource.loader.file.cache";
/// `RuntimeConstants.INPUT_ENCODING`: the encoding templates are read in (default `UTF-8`).
pub const INPUT_ENCODING: &str = "resource.default_encoding";
/// `RuntimeConstants.ENCODING_DEFAULT`: the encoding used when `resource.default_encoding` is not set.
pub const ENCODING_DEFAULT: &str = "UTF-8";
/// `RuntimeConstants.EVENTHANDLER_REFERENCEINSERTION`: the `ReferenceInsertionEventHandler` class names to install.
pub const EVENTHANDLER_REFERENCEINSERTION: &str = "event_handler.reference_insertion.class";
/// `RuntimeConstants.EVENTHANDLER_METHODEXCEPTION`: the `MethodExceptionEventHandler` class names to install.
pub const EVENTHANDLER_METHODEXCEPTION: &str = "event_handler.method_exception.class";
/// `RuntimeConstants.EVENTHANDLER_INCLUDE`: the `IncludeEventHandler` class names to install.
pub const EVENTHANDLER_INCLUDE: &str = "event_handler.include.class";
/// `RuntimeConstants.EVENTHANDLER_INVALIDREFERENCES`: the `InvalidReferenceEventHandler` class names to install.
pub const EVENTHANDLER_INVALIDREFERENCES: &str = "event_handler.invalid_references.class";
/// `RuntimeConstants.EVENTHANDLER_INVALIDREFERENCES_QUIET`: whether invalid quiet references (`$!foo`) reach the invalid
/// reference handlers.
pub const EVENTHANDLER_INVALIDREFERENCES_QUIET: &str = "event_handler.invalid_references.quiet";
/// `RuntimeConstants.EVENTHANDLER_INVALIDREFERENCES_NULL`: whether references whose value is null (rather than undefined) reach
/// the invalid reference handlers.
pub const EVENTHANDLER_INVALIDREFERENCES_NULL: &str = "event_handler.invalid_references.null";
/// `RuntimeConstants.EVENTHANDLER_INVALIDREFERENCES_TESTED`: whether references tested with `#if($foo)` reach the invalid
/// reference handlers.
pub const EVENTHANDLER_INVALIDREFERENCES_TESTED: &str = "event_handler.invalid_references.tested";
/// `RuntimeConstants.VM_LIBRARY`: the velocimacro library templates loaded at start-up.
pub const VM_LIBRARY: &str = "velocimacro.library.path";
/// `RuntimeConstants.VM_LIBRARY_DEFAULT`: the velocimacro library template name used when none is configured.
pub const VM_LIBRARY_DEFAULT: &str = "velocimacros.vtl";
/// `RuntimeConstants.VM_LIBRARY_AUTORELOAD`: whether a changed velocimacro library is reloaded.
pub const VM_LIBRARY_AUTORELOAD: &str = "velocimacro.library.autoreload";
/// `RuntimeConstants.VM_PERM_ALLOW_INLINE`: whether templates may define macros with `#macro` (default `true`).
pub const VM_PERM_ALLOW_INLINE: &str = "velocimacro.inline.allow";
/// `RuntimeConstants.VM_PERM_ALLOW_INLINE_REPLACE_GLOBAL`: whether an inline macro may replace a library macro of the same name.
pub const VM_PERM_ALLOW_INLINE_REPLACE_GLOBAL: &str = "velocimacro.inline.replace_global";
/// `RuntimeConstants.VM_PERM_INLINE_LOCAL`: whether inline macros are visible only to the template defining them.
pub const VM_PERM_INLINE_LOCAL: &str = "velocimacro.inline.local_scope";
/// `RuntimeConstants.VM_ARGUMENTS_STRICT`: whether calling a macro with the wrong number of arguments is an error.
pub const VM_ARGUMENTS_STRICT: &str = "velocimacro.arguments.strict";
/// `RuntimeConstants.VM_ENABLE_BC_MODE`: Velocity 1.7 compatible macro calls: missing arguments fall back to the
/// caller's context, and null arguments render their literal.
pub const VM_ENABLE_BC_MODE: &str = "velocimacro.enable_bc_mode";
/// `RuntimeConstants.VM_MAX_DEPTH`: how deep macro calls may nest (default `20`).
pub const VM_MAX_DEPTH: &str = "velocimacro.max_depth";
/// `RuntimeConstants.VM_BODY_REFERENCE`: the reference a block macro (`#@name`) sees its body as (default `bodyContent`).
pub const VM_BODY_REFERENCE: &str = "velocimacro.body_reference";
/// `RuntimeConstants.RUNTIME_REFERENCES_STRICT`: strict reference mode: undefined references, properties and methods are errors.
pub const RUNTIME_REFERENCES_STRICT: &str = "runtime.strict_mode.enable";
/// `RuntimeConstants.RUNTIME_REFERENCES_STRICT_ESCAPE`: whether `\$foo` escapes the reference even when `$foo` is undefined.
pub const RUNTIME_REFERENCES_STRICT_ESCAPE: &str = "runtime.strict_mode.escape";
/// `RuntimeConstants.UBERSPECT_CLASSNAME`: the uberspector class names, chained in order.
pub const UBERSPECT_CLASSNAME: &str = "introspector.uberspect.class";
/// `RuntimeConstants.INTROSPECTOR_RESTRICT_PACKAGES`: the packages `SecureUberspector` forbids method calls on.
pub const INTROSPECTOR_RESTRICT_PACKAGES: &str = "introspector.restrict.packages";
/// `RuntimeConstants.INTROSPECTOR_RESTRICT_CLASSES`: the classes `SecureUberspector` forbids method calls on.
pub const INTROSPECTOR_RESTRICT_CLASSES: &str = "introspector.restrict.classes";
/// `RuntimeConstants.CONVERSION_HANDLER_CLASS`: the class converting method arguments to parameter types.
pub const CONVERSION_HANDLER_CLASS: &str = "introspector.conversion_handler.class";
/// `RuntimeConstants.CONVERSION_HANDLER_INSTANCE`: a conversion handler object to use instead of a class.
pub const CONVERSION_HANDLER_INSTANCE: &str = "introspector.conversion_handler.instance";
/// `RuntimeConstants.PARSER_POOL_SIZE`: how many parsers the runtime keeps for reuse.
pub const PARSER_POOL_SIZE: &str = "parser.pool.size";
/// `RuntimeConstants.PARSER_HYPHEN_ALLOWED`: whether identifiers may contain `-` (`$foo-bar`; default `false`).
pub const PARSER_HYPHEN_ALLOWED: &str = "parser.allow_hyphen_in_identifiers";
/// `RuntimeConstants.RUNTIME_STRING_INTERNING`: whether identifiers are interned (default `true`).
pub const RUNTIME_STRING_INTERNING: &str = "runtime.string_interning";
/// `RuntimeConstants.INTERPOLATE_STRINGLITERALS`: whether references and directives inside `"..."` literals are evaluated
/// (default `true`).
pub const INTERPOLATE_STRINGLITERALS: &str = "runtime.interpolate_string_literals";
/// `RuntimeConstants.IMMUTABLE_RANGES`: whether `[1..3]` makes an immutable list (default `true`).
pub const IMMUTABLE_RANGES: &str = "runtime.immutable_ranges";
/// `RuntimeConstants.STRICT_MATH`: whether a math error (null operand, division by zero) fails instead of yielding null.
pub const STRICT_MATH: &str = "runtime.strict_math";
/// `RuntimeConstants.CONTEXT_AUTOREFERENCE_KEY`: a key under which the context holds itself; unset by default.
pub const CONTEXT_AUTOREFERENCE_KEY: &str = "context.self_reference_key";
/// `RuntimeConstants.SPACE_GOBBLING`: how whitespace around directives is dropped: `none`, `bc`, `lines` (default) or
/// `structured`; see [`SpaceGobbling`].
pub const SPACE_GOBBLING: &str = "parser.space_gobbling";

/// `org.apache.velocity.util.introspection.SecureUberspector`: the uberspector refusing restricted classes and
/// packages.
pub const SECURE_UBERSPECTOR: &str = "org.apache.velocity.util.introspection.SecureUberspector";
/// `org.apache.velocity.util.introspection.UberspectImpl`: the default uberspector.
pub const UBERSPECT_IMPL: &str = "org.apache.velocity.util.introspection.UberspectImpl";
/// `org.apache.velocity.util.introspection.UberspectPublicFields`: an uberspector that also reads and writes
/// public fields.
pub const UBERSPECT_PUBLIC_FIELDS: &str = "org.apache.velocity.util.introspection.UberspectPublicFields";

/// `RuntimeConstants.SpaceGobbling`, in declaration order (the renderers compare it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpaceGobbling {
    /// `NONE`: whitespace is kept as written.
    None,
    /// `BC`: Velocity 1.7's gobbling, kept for backward compatibility.
    Bc,
    /// `LINES`: a line holding only a directive (and whitespace) disappears entirely; the default.
    Lines,
    /// `STRUCTURED`: `LINES`, plus block content loses the indentation its directive adds.
    Structured,
}

/// `velocity.properties`, as shipped in the jar.
const DEFAULTS: &str = include_str!("velocity.properties");

/// The configuration an engine initialises from, and the values read from it once.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// The combined properties: `velocity.properties` defaults, then the overrides.
    pub props: ExtProperties,
    /// `parser.space_gobbling`.
    pub gobbling: SpaceGobbling,
    /// `parser.allow_hyphen_in_identifiers`.
    pub hyphen_allowed: bool,
    /// `runtime.string_interning`.
    pub string_interning: bool,
    /// `runtime.strict_mode.enable`.
    pub strict_ref: bool,
    /// `runtime.strict_mode.escape`.
    pub strict_escape: bool,
    /// `runtime.strict_math`.
    pub strict_math: bool,
    /// `runtime.interpolate_string_literals`.
    pub interpolate_string_literals: bool,
    /// `runtime.immutable_ranges`.
    pub immutable_ranges: bool,
    /// `runtime.log.log_invalid_references`.
    pub log_invalid_references: bool,
    /// `runtime.log.log_invalid_method_calls`.
    pub log_invalid_method_calls: bool,
    /// `directive.foreach.max_loops`: `-1` for no limit.
    pub max_loops: i32,
    /// `directive.foreach.skip_invalid`.
    pub skip_invalid_iterator: bool,
    /// `directive.if.empty_check`.
    pub check_empty_objects: bool,
    /// `directive.parse.max_depth`.
    pub parse_max_depth: i32,
    /// `directive.define.max_depth`.
    pub define_max_depth: i32,
    /// `velocimacro.max_depth`.
    pub vm_max_depth: i32,
    /// `velocimacro.inline.allow`.
    pub vm_inline_allow: bool,
    /// `velocimacro.inline.replace_global`.
    pub vm_replace_global: bool,
    /// `velocimacro.inline.local_scope`.
    pub vm_local_scope: bool,
    /// `velocimacro.arguments.strict`.
    pub vm_arguments_strict: bool,
    /// `velocimacro.enable_bc_mode`.
    pub vm_bc_mode: bool,
    /// `velocimacro.body_reference`.
    pub vm_body_reference: String,
    /// `velocimacro.library.autoreload`.
    pub vm_library_autoreload: bool,
    /// `directive.include.output_error_start`, if set.
    pub include_error_start: Option<String>,
    /// `directive.include.output_error_end`, if set.
    pub include_error_end: Option<String>,
    /// `context.self_reference_key`, if set.
    pub self_reference_key: Option<String>,
    /// `resource.default_encoding`.
    pub default_encoding: String,
    /// The scopes whose `context.scope_control.<scope>` is `true`.
    pub scope_controls: HashSet<String>,
    /// `event_handler.invalid_references.quiet`.
    pub invalid_references_quiet: bool,
    /// `event_handler.invalid_references.null`.
    pub invalid_references_null: bool,
    /// `event_handler.invalid_references.tested`.
    pub invalid_references_tested: bool,
}

impl RuntimeConfig {
    /// `initializeProperties`: the defaults, then the overrides (`combine`, key by key).
    pub fn new(overrides: &ExtProperties) -> RuntimeConfig {
        let mut props = ExtProperties::new();
        props.load(DEFAULTS, None);
        props.combine(overrides);
        RuntimeConfig::from_props(props)
    }

    fn from_props(props: ExtProperties) -> RuntimeConfig {
        let gobbling = match props.string_or(SPACE_GOBBLING, "lines").to_uppercase().as_str() {
            "NONE" => SpaceGobbling::None,
            "BC" => SpaceGobbling::Bc,
            "STRUCTURED" => SpaceGobbling::Structured,
            _ => SpaceGobbling::Lines,
        };
        let mut scope_controls = HashSet::new();
        if let Some(scopes) = props.subset(CONTEXT_SCOPE_CONTROL) {
            for key in scopes.keys() {
                if scopes.boolean(key, false) {
                    scope_controls.insert(key.clone());
                }
            }
        }
        RuntimeConfig {
            gobbling,
            hyphen_allowed: props.boolean(PARSER_HYPHEN_ALLOWED, false),
            string_interning: props.boolean(RUNTIME_STRING_INTERNING, true),
            strict_ref: props.boolean(RUNTIME_REFERENCES_STRICT, false),
            strict_escape: props.boolean(RUNTIME_REFERENCES_STRICT_ESCAPE, false),
            strict_math: props.boolean(STRICT_MATH, false),
            interpolate_string_literals: props.boolean(INTERPOLATE_STRINGLITERALS, true),
            immutable_ranges: props.boolean(IMMUTABLE_RANGES, true),
            log_invalid_references: props.boolean(RUNTIME_LOG_REFERENCE_LOG_INVALID, true),
            log_invalid_method_calls: props.boolean(RUNTIME_LOG_METHOD_CALL_LOG_INVALID, true),
            max_loops: props.int(MAX_NUMBER_LOOPS, -1),
            skip_invalid_iterator: props.boolean(SKIP_INVALID_ITERATOR, true),
            check_empty_objects: props.boolean(CHECK_EMPTY_OBJECTS, true),
            parse_max_depth: props.int(PARSE_DIRECTIVE_MAXDEPTH, 10),
            define_max_depth: props.int(DEFINE_DIRECTIVE_MAXDEPTH, 2),
            vm_max_depth: props.int(VM_MAX_DEPTH, 20),
            vm_inline_allow: props.boolean(VM_PERM_ALLOW_INLINE, true),
            vm_replace_global: props.boolean(VM_PERM_ALLOW_INLINE_REPLACE_GLOBAL, false),
            vm_local_scope: props.boolean(VM_PERM_INLINE_LOCAL, false),
            vm_arguments_strict: props.boolean(VM_ARGUMENTS_STRICT, false),
            vm_bc_mode: props.boolean(VM_ENABLE_BC_MODE, false),
            vm_body_reference: props.string_or(VM_BODY_REFERENCE, "bodyContent"),
            vm_library_autoreload: props.boolean(VM_LIBRARY_AUTORELOAD, false),
            include_error_start: props.string(ERRORMSG_START),
            include_error_end: props.string(ERRORMSG_END),
            self_reference_key: props.string(CONTEXT_AUTOREFERENCE_KEY),
            default_encoding: props.string_or(INPUT_ENCODING, ENCODING_DEFAULT),
            scope_controls,
            invalid_references_quiet: props.boolean(EVENTHANDLER_INVALIDREFERENCES_QUIET, false),
            invalid_references_null: props.boolean(EVENTHANDLER_INVALIDREFERENCES_NULL, false),
            invalid_references_tested: props.boolean(EVENTHANDLER_INVALIDREFERENCES_TESTED, false),
            props,
        }
    }

    /// `isScopeControlEnabled`.
    pub fn scope_enabled(&self, name: &str) -> bool {
        self.scope_controls.contains(name)
    }

    /// The uberspector class names, in order (`introspector.uberspect.class`).
    pub fn uberspectors(&self) -> Vec<String> {
        self.props.string_array(UBERSPECT_CLASSNAME)
    }

    /// Whether `SecureUberspector` is among the uberspectors.
    pub fn is_secure(&self) -> bool {
        self.uberspectors().iter().any(|u| u == SECURE_UBERSPECTOR)
    }
}

/// MockServer 8.0.0's `VelocityTemplateEngine.buildVelocityEngine` properties (the string repository's own keys
/// aside, which the engine models by caching every template it parses).
pub fn mockserver_properties() -> ExtProperties {
    let mut p = ExtProperties::new();
    for (k, v) in [
        (RUNTIME_LOG_REFERENCE_LOG_INVALID, "true"),
        (RUNTIME_STRING_INTERNING, "true"),
        (MAX_NUMBER_LOOPS, "-1"),
        (CHECK_EMPTY_OBJECTS, "true"),
        (PARSE_DIRECTIVE_MAXDEPTH, "10"),
        (RUNTIME_REFERENCES_STRICT, "false"),
        ("context.scope_control.template", "false"),
        ("context.scope_control.evaluate", "false"),
        ("context.scope_control.foreach", "true"),
        ("context.scope_control.macro", "false"),
        ("context.scope_control.define", "false"),
        ("directive.set.null.allowed", "true"),
        (INTERPOLATE_STRINGLITERALS, "true"),
        (INPUT_ENCODING, "UTF-8"),
        (PARSER_POOL_SIZE, "50"),
        (SPACE_GOBBLING, "lines"),
        (PARSER_HYPHEN_ALLOWED, "true"),
        (RESOURCE_MANAGER_DEFAULTCACHE_SIZE, "1000"),
        (RESOURCE_LOADERS, "string"),
        (UBERSPECT_CLASSNAME, SECURE_UBERSPECTOR),
    ] {
        p.set_property(k, PropValue::Str(v.to_string()));
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_then_overrides() {
        let c = RuntimeConfig::new(&ExtProperties::new());
        assert_eq!(c.gobbling, SpaceGobbling::Lines);
        assert!(!c.hyphen_allowed);
        assert!(c.scope_enabled("foreach"));
        assert!(!c.scope_enabled("macro"));
        assert_eq!(c.uberspectors(), vec![UBERSPECT_IMPL]);
        let m = RuntimeConfig::new(&mockserver_properties());
        assert!(m.hyphen_allowed && m.is_secure());
        assert!(!m.scope_enabled("template"));
    }
}
