//! `org.apache.velocity.tools.generic.EscapeTool` (`$esc`).

use std::sync::{Arc, Weak};

use java_compat::lang::{JError, JMap, JObject, JValue};
use java_compat::reflect::ClassDef;
use parking_lot::Mutex;

use super::classes::{Annotation, JavaClass};
use super::safe::{self, SafeConfig, Tool};
use super::values::ValueParser;
use super::{Ret, identity, o, object_methods, s, string_of, text, tool_object, val};

/// The class name of `org.apache.velocity.tools.generic.EscapeTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.EscapeTool";
/// `EscapeTool.DEFAULT_KEY`: the key `velocity(obj)` refers to the tool by.
pub const DEFAULT_KEY: &str = "esc";

/// Port of `org.apache.velocity.tools.generic.EscapeTool` (`$esc`): strings escaped for Java, JavaScript, HTML, XML,
/// URLs, SQL, properties and VTL; `None` for `null` input.
pub struct EscapeTool {
    safe: SafeConfig,
    key: Mutex<String>,
    me: Weak<EscapeTool>,
}

impl EscapeTool {
    /// `new EscapeTool()`: known under the key `esc`.
    pub fn new() -> Arc<EscapeTool> {
        Arc::new_cyclic(|me| EscapeTool { safe: SafeConfig::default(), key: Mutex::new(DEFAULT_KEY.into()), me: me.clone() })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: reads `key`; nothing once locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    /// `getKey()`: the key `velocity(obj)` refers to the tool by.
    pub fn get_key(&self) -> String {
        self.key.lock().clone()
    }

    /// `setKey(key)`.
    pub fn set_key(&self, key: &str) {
        *self.key.lock() = key.to_string();
    }

    /// `velocity(obj)`: `$` and `#` as references to this tool (`${esc.d}`, `${esc.h}`).
    pub fn velocity(&self, v: &JValue) -> Option<String> {
        let key = self.get_key();
        string_of(v).map(|s| s.replace('$', &format!("${{{key}.d}}")).replace('#', &format!("${{{key}.h}}")))
    }

    /// `java(obj)`: `StringEscapeUtils.escapeJava` of its string.
    pub fn java(&self, v: &JValue) -> Option<String> {
        string_of(v).map(|s| java_compat::escape_java(&s))
    }

    /// `propertyKey(obj)`: escaped as a properties-file key (spaces escaped too).
    pub fn property_key(&self, v: &JValue) -> Option<String> {
        string_of(v).map(|s| dump_string(&s, true))
    }

    /// `propertyValue(obj)`: escaped as a properties-file value (of its spaces, only a leading one escaped).
    pub fn property_value(&self, v: &JValue) -> Option<String> {
        string_of(v).map(|s| dump_string(&s, false))
    }

    /// `javascript(obj)`: `StringEscapeUtils.escapeEcmaScript` of its string.
    pub fn javascript(&self, v: &JValue) -> Option<String> {
        string_of(v).map(|s| java_compat::escape_ecma_script(&s))
    }

    /// `html(obj)`: `StringEscapeUtils.escapeHtml4` of its string.
    pub fn html(&self, v: &JValue) -> Option<String> {
        string_of(v).map(|s| java_compat::escape_html4(&s))
    }

    /// `url(obj)`: `URLEncoder.encode` in UTF-8.
    pub fn url(&self, v: &JValue) -> Option<String> {
        string_of(v).map(|s| java_compat::url_encode(&s))
    }

    /// `xml(obj)`: `StringEscapeUtils.escapeXml10` of its string.
    pub fn xml(&self, v: &JValue) -> Option<String> {
        string_of(v).map(|s| java_compat::escape_xml10(&s))
    }

    /// `sql(obj)`: single quotes doubled.
    pub fn sql(&self, v: &JValue) -> Option<String> {
        string_of(v).map(|s| s.replace('\'', "''"))
    }

    /// `unicode(code)`: the character with this hexadecimal code point (`\u` optional).
    pub fn unicode(&self, v: &JValue) -> Result<Option<String>, JError> {
        Ok(unicode(v)?.and_then(|v| v.as_str().map(str::to_string)))
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for EscapeTool {
    fn safe(&self) -> &SafeConfig {
        &self.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        if let Some(k) = values.get_string("key") {
            self.set_key(&k);
        }
    }
}

/// `dumpString`: `Properties.store`'s escaping of a key or value.
fn dump_string(string: &str, key: bool) -> String {
    let units: Vec<u16> = string.encode_utf16().collect();
    let mut out = String::new();
    let mut i = 0;
    if !key && units.first() == Some(&(b' ' as u16)) {
        out.push_str("\\ ");
        i = 1;
    }
    for &ch in &units[i..] {
        match ch {
            0x09 => out.push_str("\\t"),
            0x0a => out.push_str("\\n"),
            0x0c => out.push_str("\\f"),
            0x0d => out.push_str("\\r"),
            _ => {
                if "\\#!=:".encode_utf16().any(|c| c == ch) || (key && ch == b' ' as u16) {
                    out.push('\\');
                }
                if (0x20..=0x7e).contains(&ch) {
                    out.push(ch as u8 as char);
                } else {
                    out.push_str(&format!("\\u{ch:04x}"));
                }
            }
        }
    }
    out
}

/// `unicode(code)`: `Integer.valueOf(s, 16)` then `Character.toChars`.
fn unicode(code: &JValue) -> Ret {
    let Some(mut s) = string_of(code) else { return val(JValue::Null) };
    if s.starts_with("\\u") {
        s = s[2..].to_string();
    }
    let digits = s.strip_prefix(['+', '-']).unwrap_or(&s);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        let message = if s.is_empty() { "For input string: \"\"".to_string() } else { format!("For input string: \"{s}\" under radix 16") };
        return Err(JError::new("java.lang.NumberFormatException", message));
    }
    let cp = i64::from_str_radix(&s, 16).ok().filter(|v| i32::try_from(*v).is_ok());
    let Some(cp) = cp else { return Err(JError::new("java.lang.NumberFormatException", format!("For input string: \"{s}\" under radix 16"))) };
    match u32::try_from(cp).ok().filter(|c| *c <= 0x10ffff) {
        Some(c) => {
            // A lone surrogate is a valid `char` in Java; Rust strings cannot hold one.
            let ch = char::from_u32(c).unwrap_or('\u{fffd}');
            text(Some(ch.to_string()))
        }
        None => Err(JError::new("java.lang.IllegalArgumentException", format!("Not a valid Unicode code point: 0x{:X}", cp as i32 as u32))),
    }
}

fn escaper(f: fn(&str) -> String) -> impl Fn(&EscapeTool, &JValue, &[JValue]) -> Ret + Send + Sync {
    move |_, _, a| text(string_of(&a[0]).map(|s| f(&s)))
}

fn constant(v: &'static str) -> impl Fn(&EscapeTool, &JValue, &[JValue]) -> Ret + Send + Sync {
    move |_, _, _| val(JValue::str(v))
}

fn def() -> Arc<ClassDef> {
    let mut def = ClassDef::new(CLASS)
        .extends(&[safe::SAFE_CONFIG])
        .implements(&["java.io.Serializable"])
        .method::<EscapeTool>("getKey", &[], Some(s()), |t, _, _| val(JValue::str(t.get_key())))
        .method::<EscapeTool>("velocity", &[o()], Some(s()), |t, _, a| text(t.velocity(&a[0])))
        .method::<EscapeTool>("java", &[o()], Some(s()), escaper(java_compat::escape_java))
        .method::<EscapeTool>("propertyKey", &[o()], Some(s()), |_, _, a| text(string_of(&a[0]).map(|s| dump_string(&s, true))))
        .method::<EscapeTool>("propertyValue", &[o()], Some(s()), |_, _, a| text(string_of(&a[0]).map(|s| dump_string(&s, false))))
        .method::<EscapeTool>("javascript", &[o()], Some(s()), escaper(java_compat::escape_ecma_script))
        .method::<EscapeTool>("html", &[o()], Some(s()), escaper(java_compat::escape_html4))
        .method::<EscapeTool>("url", &[o()], Some(s()), escaper(java_compat::url_encode))
        .method::<EscapeTool>("unurl", &[o()], Some(s()), |_, _, a| match string_of(&a[0]) {
            None => val(JValue::Null),
            Some(s) => java_compat::url_decode(&s).map(|d| Some(JValue::str(d))).map_err(|m| JError::new("java.lang.IllegalArgumentException", m)),
        })
        .method::<EscapeTool>("xml", &[o()], Some(s()), escaper(java_compat::escape_xml10))
        .method::<EscapeTool>("sql", &[o()], Some(s()), escaper(|s| s.replace('\'', "''")))
        .method::<EscapeTool>("unicode", &[o()], Some(s()), |_, _, a| unicode(&a[0]));
    for (names, v) in [
        (["getDollar", "getD"], "$"),
        (["getHash", "getH"], "#"),
        (["getBackslash", "getB"], "\\"),
        (["getQuote", "getQ"], "\""),
        (["getSingleQuote", "getS"], "'"),
        (["getNewline", "getN"], "\n"),
        (["getExclamation", "getE"], "!"),
    ] {
        for n in names {
            def = def.method::<EscapeTool>(n, &[], Some(s()), constant(v));
        }
    }
    object_methods::<EscapeTool>(safe::safe_config_methods::<EscapeTool>(def)).register()
}

tool_object!(EscapeTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || EscapeTool::new().value()).annotated(Annotation::default_key("esc")).annotated(Annotation::valid_scope(&[super::scope::APPLICATION]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_escaping() {
        assert_eq!(dump_string(" a b=c:d#é\n", false), "\\ a b\\=c\\:d\\#\\u00e9\\n");
        assert_eq!(dump_string("a b", true), "a\\ b");
    }
}
