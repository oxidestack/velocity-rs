//! `org.apache.velocity.util.ExtProperties` with `DeprecationAwareExtProperties`: ordered, multi-valued
//! properties where a string with commas adds several values, deprecated keys are translated to their current
//! names, `${key}` interpolates, and typed getters parse (and cache) string values.

use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use java_compat::lang::JError;

/// A property value: strings (possibly several), parsed scalars, or an object set programmatically
/// (`runtime.log.instance`, event handler and resource loader instances).
#[derive(Clone)]
pub enum PropValue {
    /// A string, as read from a file or set.
    Str(String),
    /// Several values for one key (a key repeated, or a comma-separated string).
    List(Vec<PropValue>),
    /// A boolean set programmatically.
    Bool(bool),
    /// An integer set programmatically.
    Int(i64),
    /// A floating-point number set programmatically.
    Double(f64),
    /// Any other object set programmatically, read back with [`PropValue::as_object`].
    Object(Arc<dyn Any + Send + Sync>),
}

impl fmt::Debug for PropValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PropValue::Str(s) => write!(f, "{s:?}"),
            PropValue::List(l) => f.debug_list().entries(l).finish(),
            PropValue::Bool(b) => write!(f, "{b}"),
            PropValue::Int(i) => write!(f, "{i}"),
            PropValue::Double(d) => write!(f, "{d}"),
            PropValue::Object(_) => f.write_str("<object>"),
        }
    }
}

impl PropValue {
    /// `toString()`.
    pub fn java_string(&self) -> String {
        match self {
            PropValue::Str(s) => s.clone(),
            PropValue::List(l) => format!("[{}]", l.iter().map(PropValue::java_string).collect::<Vec<_>>().join(", ")),
            PropValue::Bool(b) => b.to_string(),
            PropValue::Int(i) => i.to_string(),
            PropValue::Double(d) => java_compat::double_to_string(*d),
            PropValue::Object(_) => "<object>".into(),
        }
    }

    /// The object of an [`PropValue::Object`] value, when it is a `T`; `None` otherwise.
    pub fn as_object<T: 'static>(&self) -> Option<&T> {
        match self {
            PropValue::Object(o) => o.downcast_ref::<T>(),
            _ => None,
        }
    }
}

impl From<&str> for PropValue {
    fn from(s: &str) -> PropValue {
        PropValue::Str(s.to_string())
    }
}

impl From<String> for PropValue {
    fn from(s: String) -> PropValue {
        PropValue::Str(s)
    }
}

impl From<bool> for PropValue {
    fn from(b: bool) -> PropValue {
        PropValue::Bool(b)
    }
}

impl From<i64> for PropValue {
    fn from(i: i64) -> PropValue {
        PropValue::Int(i)
    }
}

impl From<i32> for PropValue {
    fn from(i: i32) -> PropValue {
        PropValue::Int(i as i64)
    }
}

/// `DeprecatedRuntimeConstants` → `RuntimeConstants`, where the names differ.
const DEPRECATED: &[(&str, &str)] = &[
    ("runtime.log.invalid.references", "runtime.log.log_invalid_references"),
    ("directive.foreach.maxloops", "directive.foreach.max_loops"),
    ("directive.foreach.skip.invalid", "directive.foreach.skip_invalid"),
    ("directive.if.emptycheck", "directive.if.empty_check"),
    ("directive.include.output.errormsg.start", "directive.include.output_error_start"),
    ("directive.include.output.errormsg.end", "directive.include.output_error_end"),
    ("directive.parse.max.depth", "directive.parse.max_depth"),
    ("directive.define.max.depth", "directive.define.max_depth"),
    ("userdirective", "runtime.custom_directives"),
    ("resource.manager.defaultcache.size", "resource.manager.cache.default_size"),
    ("resource.manager.logwhenfound", "resource.manager.log_when_found"),
    ("resource.loader", "resource.loaders"),
    ("file.resource.loader.path", "resource.loader.file.path"),
    ("file.resource.loader.cache", "resource.loader.file.cache"),
    ("modificationCheckInterval", "modification_check_interval"),
    ("ds.resource.loader.resource.datasource", "resource.loader.ds.resource.datasource_url"),
    ("ds.resource.loader.resource.keycolumn", "resource.loader.ds.resource.key_column"),
    ("ds.resource.loader.resource.templatecolumn", "resource.loader.ds.resource.template_column"),
    ("ds.resource.loader.resource.timestampcolumn", "resource.loader.ds.resource.timestamp_column"),
    ("input.encoding", "resource.default_encoding"),
    ("eventhandler.referenceinsertion.class", "event_handler.reference_insertion.class"),
    ("eventhandler.methodexception.class", "event_handler.method_exception.class"),
    ("eventhandler.include.class", "event_handler.include.class"),
    ("eventhandler.invalidreferences.class", "event_handler.invalid_references.class"),
    ("velocimacro.library", "velocimacro.library.path"),
    ("VM_global_library.vm", "velocimacros.vtl"),
    ("velocimacro.permissions.allow.inline", "velocimacro.inline.allow"),
    ("velocimacro.permissions.allow.inline.to.replace.global", "velocimacro.inline.replace_global"),
    ("velocimacro.permissions.allow.inline.local.scope", "velocimacro.inline.local_scope"),
    ("velocimacro.max.depth", "velocimacro.max_depth"),
    ("velocimacro.body.reference", "velocimacro.body_reference"),
    ("runtime.references.strict", "runtime.strict_mode.enable"),
    ("runtime.references.strict.escape", "runtime.strict_mode.escape"),
    ("runtime.introspector.uberspect", "introspector.uberspect.class"),
    ("runtime.interpolate.string.literals", "runtime.interpolate_string_literals"),
    ("runtime.strict.math", "runtime.strict_math"),
    ("context.autoreference.key", "context.self_reference_key"),
    ("space.gobbling", "parser.space_gobbling"),
    ("velocimacro.arguments.preserve_literals", "velocimacro.enable_bc_mode"),
];

/// `DeprecationAwareExtProperties.translateKey`.
pub fn translate_key(key: &str) -> String {
    if let Some((_, new)) = DEPRECATED.iter().find(|(old, _)| *old == key) {
        return new.to_string();
    }
    if let Some(i) = key.find(".resource.loader.") {
        return format!("resource.loader.{}{}", &key[..i + 1], &key[i + 17..]);
    }
    if let Some(scope) = key.strip_suffix(".provide.scope.control") {
        return format!("context.scope_control.{scope}");
    }
    key.to_string()
}

fn class_cast(key: &str, what: &str) -> JError {
    JError::new("java.lang.ClassCastException", format!("'{key}' doesn't map to a {what} object"))
}

/// `endsWithSlash`: an odd number of trailing backslashes.
fn ends_with_slash(s: &str) -> bool {
    (s.len() - s.trim_end_matches('\\').len()) % 2 == 1
}

/// `unescape`: `\\` → `\`.
fn unescape(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '\\' && chars[i + 1] == '\\' {
            out.push('\\');
            i += 2;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out.into_iter().collect()
}

/// `PropertiesTokenizer`: comma-separated, a backslash before the comma keeps it, tokens trimmed.
fn tokenize(s: &str) -> Vec<String> {
    let raw: Vec<&str> = s.split(',').filter(|t| !t.is_empty()).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let mut buffer = String::new();
        while i < raw.len() {
            let token = raw[i];
            i += 1;
            if ends_with_slash(token) {
                buffer.push_str(&token[..token.len() - 1]);
                buffer.push(',');
            } else {
                buffer.push_str(token);
                break;
            }
        }
        out.push(buffer.trim().to_string());
    }
    out
}

#[derive(Clone, Default, Debug)]
/// Port of `org.apache.velocity.util.ExtProperties` (and `DeprecationAwareExtProperties`): keys with one or
/// several values, kept in the order first set, with deprecated 1.x keys translated to their 2.x names.
pub struct ExtProperties {
    values: HashMap<String, PropValue>,
    keys_as_listed: Vec<String>,
    initialized: bool,
    /// The deprecated keys already warned about (`warned`).
    warned: std::sync::Arc<parking_lot::Mutex<std::collections::HashSet<String>>>,
}

thread_local! {
    /// `DeprecationAwareExtProperties.logger` (`org.apache.velocity.deprecation`), replaceable per thread.
    static DEPRECATION_LOGGER: std::cell::RefCell<std::sync::Arc<dyn crate::log::Logger>> =
        std::cell::RefCell::new(std::sync::Arc::new(crate::log::NopLogger("org.apache.velocity.deprecation".into())));
}

/// Replaces `DeprecationAwareExtProperties.logger` on this thread.
pub fn set_deprecation_logger(logger: std::sync::Arc<dyn crate::log::Logger>) {
    DEPRECATION_LOGGER.with(|l| *l.borrow_mut() = logger);
}

impl ExtProperties {
    /// `new ExtProperties()`: empty.
    pub fn new() -> ExtProperties {
        ExtProperties::default()
    }

    /// `translateKey`, warning once per deprecated key.
    fn translate(&self, key: &str) -> String {
        let new = translate_key(key);
        if new != key && self.warned.lock().insert(key.to_string()) {
            use crate::log::LogExt;
            let logger = DEPRECATION_LOGGER.with(|l| l.borrow().clone());
            logger.warn(|| format!("configuration key '{key}' has been deprecated in favor of '{new}'"));
        }
        new
    }

    /// Whether any property was ever added (or a file loaded).
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// `load(InputStream)`: `key = value` lines, `#` comments, trailing-backslash continuations, `include`.
    pub fn load(&mut self, text: &str, base_path: Option<&std::path::Path>) {
        let mut lines = text.lines();
        loop {
            let mut buffer = String::new();
            let mut property = None;
            for line in lines.by_ref() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if ends_with_slash(line) {
                    buffer.push_str(&line[..line.len() - 1]);
                } else {
                    buffer.push_str(line);
                    property = Some(std::mem::take(&mut buffer));
                    break;
                }
            }
            let Some(line) = property else { break };
            let Some(eq) = line.find('=').filter(|e| *e > 0) else { continue };
            let key = line[..eq].trim().to_string();
            let value = line[eq + 1..].trim().to_string();
            if value.is_empty() {
                continue;
            }
            if key.eq_ignore_ascii_case("include") {
                let v = value.strip_prefix("./").unwrap_or(&value);
                let file = if value.starts_with('/') { std::path::PathBuf::from(&value) } else { base_path.map(|b| b.join(v)).unwrap_or_else(|| v.into()) };
                if let Ok(text) = std::fs::read_to_string(&file) {
                    self.load(&text, file.parent());
                }
            } else {
                self.add_property(&key, PropValue::Str(value));
            }
        }
        self.initialized = true;
    }

    /// `getProperty(key)`: the raw value, `None` when the key is unset.
    pub fn get(&self, key: &str) -> Option<&PropValue> {
        self.values.get(&self.translate(key))
    }

    /// `containsKey(key)`.
    pub fn contains_key(&self, key: &str) -> bool {
        self.values.contains_key(&self.translate(key))
    }

    fn put(&mut self, key: &str, value: PropValue) {
        self.values.insert(self.translate(key), value);
    }

    /// `addProperty`: a string with commas adds each token.
    pub fn add_property(&mut self, key: &str, value: PropValue) {
        match value {
            PropValue::Str(s) if s.find(',').is_some_and(|i| i > 0) => {
                for token in tokenize(&s) {
                    self.add_internal(key, PropValue::Str(unescape(&token)));
                }
            }
            PropValue::Str(s) => self.add_internal(key, PropValue::Str(unescape(&s))),
            other => self.add_internal(key, other),
        }
        self.initialized = true;
    }

    fn add_internal(&mut self, key: &str, value: PropValue) {
        match self.get(key).cloned() {
            Some(current @ PropValue::Str(_)) => self.put(key, PropValue::List(vec![current, value])),
            Some(PropValue::List(mut l)) => {
                l.push(value);
                self.put(key, PropValue::List(l));
            }
            _ => {
                if !self.contains_key(key) {
                    self.keys_as_listed.push(self.translate(key));
                }
                self.put(key, value);
            }
        }
    }

    /// `setProperty(key, value)`: replaces any value the key had (a string with commas still adds each token).
    pub fn set_property(&mut self, key: &str, value: PropValue) {
        self.clear_property(key);
        self.add_property(key, value);
    }

    /// `clearProperty(key)`: removes the key and its value.
    pub fn clear_property(&mut self, key: &str) {
        let key = self.translate(key);
        if self.values.remove(&key).is_some() {
            if let Some(i) = self.keys_as_listed.iter().position(|k| *k == key) {
                self.keys_as_listed.remove(i);
            }
        }
    }

    /// `combine`: every key of `other`, set (replacing) in listed order.
    pub fn combine(&mut self, other: &ExtProperties) {
        for key in &other.keys_as_listed {
            if let Some(v) = other.values.get(key) {
                self.set_property(key, v.clone());
            }
        }
    }

    /// `getKeys()`: the keys, in the order first set.
    pub fn keys(&self) -> &[String] {
        &self.keys_as_listed
    }

    /// `subset(prefix)`: keys starting with the prefix, renamed without it (and its dot); `None` when none match.
    pub fn subset(&self, prefix: &str) -> Option<ExtProperties> {
        let mut c = ExtProperties::new();
        let mut valid = false;
        for key in &self.keys_as_listed {
            if key.starts_with(prefix) {
                valid = true;
                let new_key = if key.len() == prefix.len() { prefix.to_string() } else { key[prefix.len() + 1..].to_string() };
                // `addPropertyDirect` translates deprecated keys (`modificationCheckInterval`).
                let new_key = c.translate(&new_key);
                if !c.values.contains_key(&new_key) {
                    c.keys_as_listed.push(new_key.clone());
                }
                if let Some(v) = self.values.get(key) {
                    c.values.insert(new_key, v.clone());
                }
            }
        }
        valid.then_some(c)
    }

    /// `interpolate`: `${key}` replaced by the property's value, recursively.
    fn interpolate(&self, base: &str, prior: &mut Vec<String>) -> Result<String, JError> {
        let mut result = String::new();
        let mut rest = base;
        while let Some(begin) = rest.find("${") {
            let Some(end) = rest[begin..].find('}').map(|e| e + begin) else { break };
            result.push_str(&rest[..begin]);
            let variable = &rest[begin + 2..end];
            if prior.iter().any(|p| p == variable) {
                let initial = prior.remove(0);
                prior.push(variable.to_string());
                return Err(JError::new("java.lang.IllegalStateException", format!("infinite loop in property interpolation of {initial}: {}", prior.join("->"))));
            }
            prior.push(variable.to_string());
            match self.get(variable) {
                Some(v) => {
                    let text = v.java_string();
                    result.push_str(&self.interpolate(&text, prior)?);
                    prior.pop();
                }
                None => {
                    result.push_str("${");
                    result.push_str(variable);
                    result.push('}');
                }
            }
            rest = &rest[end + 1..];
        }
        result.push_str(rest);
        Ok(result)
    }

    fn interpolated(&self, s: &str) -> Result<String, JError> {
        self.interpolate(s, &mut vec![s.to_string()])
    }

    /// `getString(key, default)`: the first value of a list, interpolated.
    pub fn get_string(&self, key: &str, default: Option<&str>) -> Result<Option<String>, JError> {
        match self.get(key) {
            Some(PropValue::Str(s)) => self.interpolated(s).map(Some),
            None => default.map(|d| self.interpolated(d)).transpose(),
            Some(PropValue::List(l)) => match l.first() {
                Some(PropValue::Str(s)) => self.interpolated(s).map(Some),
                _ => Err(class_cast(key, "String")),
            },
            Some(_) => Err(class_cast(key, "String")),
        }
    }

    /// `getString`, with the `RuntimeInstance` trimming and no error.
    pub fn string(&self, key: &str) -> Option<String> {
        self.get_string(key, None).ok().flatten().map(|s| s.trim().to_string())
    }

    /// `getString(key, default)` with no error: `default` when the key is unset or not a string.
    pub fn string_or(&self, key: &str, default: &str) -> String {
        self.get_string(key, Some(default)).ok().flatten().unwrap_or_else(|| default.to_string())
    }

    /// `getStringArray`.
    pub fn get_string_array(&self, key: &str) -> Result<Vec<String>, JError> {
        match self.get(key) {
            None => Ok(Vec::new()),
            Some(PropValue::Str(s)) => Ok(vec![s.clone()]),
            Some(PropValue::List(l)) => l
                .iter()
                .map(|v| match v {
                    PropValue::Str(s) => Ok(s.clone()),
                    _ => Err(class_cast(key, "String/List")),
                })
                .collect(),
            Some(_) => Err(class_cast(key, "String/List")),
        }
    }

    /// [`ExtProperties::get_string_array`] with no error: empty when the key is unset or not strings.
    pub fn string_array(&self, key: &str) -> Vec<String> {
        self.get_string_array(key).unwrap_or_default()
    }

    /// `getBoolean(key, default)`: `true`/`on`/`yes`, `false`/`off`/`no`; anything else is `false`.
    pub fn get_boolean(&self, key: &str, default: bool) -> Result<bool, JError> {
        match self.get(key) {
            None => Ok(default),
            Some(PropValue::Bool(b)) => Ok(*b),
            Some(PropValue::Str(s)) => Ok(matches!(s.trim().to_lowercase().as_str(), "true" | "on" | "yes")),
            Some(_) => Err(class_cast(key, "Boolean")),
        }
    }

    /// [`ExtProperties::get_boolean`] with no error: `default` when the value is not a boolean.
    pub fn boolean(&self, key: &str, default: bool) -> bool {
        self.get_boolean(key, default).unwrap_or(default)
    }

    /// `getInteger(key, default)`: `Integer.valueOf` of a string (a `NumberFormatException` otherwise).
    pub fn get_int(&self, key: &str, default: i32) -> Result<i32, JError> {
        match self.get(key) {
            None => Ok(default),
            Some(PropValue::Int(i)) => Ok(*i as i32),
            Some(PropValue::Double(d)) => Ok(*d as i32),
            Some(PropValue::Str(s)) => crate::uberspect::parse_integral(s, 32).map(|v| v as i32),
            Some(_) => Err(class_cast(key, "Integer")),
        }
    }

    /// [`ExtProperties::get_int`] with no error: `default` when the value is not an integer.
    pub fn int(&self, key: &str, default: i32) -> i32 {
        self.get_int(key, default).unwrap_or(default)
    }

    /// `ExtProperties(String file)`: loads a file, `include`s relative to its directory.
    pub fn from_file(path: &std::path::Path) -> std::io::Result<ExtProperties> {
        let bytes = std::fs::read(path)?;
        // `Properties` files are ISO-8859-1.
        let text: String = bytes.iter().map(|b| *b as char).collect();
        let mut p = ExtProperties::new();
        p.load(&text, path.parent());
        Ok(p)
    }

    fn required<T>(&self, key: &str, v: Result<Option<T>, JError>) -> Result<T, JError> {
        v?.ok_or_else(|| JError::new("java.util.NoSuchElementException", format!("'{key} doesn't map to an existing object")))
    }

    fn number(&self, key: &str, what: &str, parse: impl Fn(&str) -> Result<f64, JError>) -> Result<Option<f64>, JError> {
        match self.get(key) {
            None => Ok(None),
            Some(PropValue::Int(i)) => Ok(Some(*i as f64)),
            Some(PropValue::Double(d)) => Ok(Some(*d)),
            Some(PropValue::Str(s)) => parse(s).map(Some),
            Some(_) => Err(class_cast(key, what)),
        }
    }

    /// `getByte(key)`.
    pub fn get_byte(&self, key: &str) -> Result<i8, JError> {
        let v = self.number(key, "Byte", |s| crate::uberspect::parse_integral(s, 8).map(|v| v as f64)).map(|v| v.map(|v| v as i8));
        self.required(key, v)
    }

    /// `getShort(key)`.
    pub fn get_short(&self, key: &str) -> Result<i16, JError> {
        let v = self.number(key, "Short", |s| crate::uberspect::parse_integral(s, 16).map(|v| v as f64)).map(|v| v.map(|v| v as i16));
        self.required(key, v)
    }

    /// `getFloat(key)`.
    pub fn get_float(&self, key: &str) -> Result<f32, JError> {
        let v = self.number(key, "Float", |s| crate::uberspect::parse_double(s).map(|d| d as f32 as f64)).map(|v| v.map(|v| v as f32));
        self.required(key, v)
    }

    /// `getDouble(key)`.
    pub fn get_double(&self, key: &str) -> Result<f64, JError> {
        let v = self.number(key, "Double", crate::uberspect::parse_double);
        self.required(key, v)
    }

    /// `getLong(key, default)`: `default` when unset; `Err` (`NumberFormatException`, `ClassCastException`) when the
    /// value is not a long.
    pub fn get_long(&self, key: &str, default: i64) -> Result<i64, JError> {
        match self.get(key) {
            None => Ok(default),
            Some(PropValue::Int(i)) => Ok(*i),
            Some(PropValue::Double(d)) => Ok(*d as i64),
            Some(PropValue::Str(s)) => crate::uberspect::parse_integral(s, 64),
            Some(_) => Err(class_cast(key, "Long")),
        }
    }

    /// The object set for a key (`getProperty` returning a non-string).
    pub fn object<T: 'static>(&self, key: &str) -> Option<&T> {
        self.get(key).and_then(PropValue::as_object::<T>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commas_split_and_deprecated_keys_translate() {
        let mut p = ExtProperties::new();
        p.set_property("resource.loader", "file, string".into());
        assert_eq!(p.string_array("resource.loaders"), vec!["file", "string"]);
        p.set_property("file.resource.loader.path", "a\\,b".into());
        assert_eq!(p.string("resource.loader.file.path").unwrap(), "a,b");
        p.set_property("foreach.provide.scope.control", "true".into());
        assert!(p.boolean("context.scope_control.foreach", false));
        assert_eq!(translate_key("string.resource.loader.class"), "resource.loader.string.class");
    }

    #[test]
    fn interpolation_and_loading() {
        let mut p = ExtProperties::new();
        p.load("# comment\nbase = /x\npath = ${base}/y\nlong = a\\\n  b\nempty =\n", None);
        assert_eq!(p.string("path").unwrap(), "/x/y");
        assert_eq!(p.string("long").unwrap(), "ab");
        assert!(!p.contains_key("empty"));
        let sub = {
            let mut q = ExtProperties::new();
            q.set_property("context.scope_control.macro", "true".into());
            q.subset("context.scope_control").unwrap()
        };
        assert_eq!(sub.keys(), ["macro"]);
    }
}
