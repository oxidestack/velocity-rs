//! `org.apache.velocity.tools.generic.DisplayTool` (`$display`): lists, truncation, cells, capitalisation,
//! alternates, tag stripping, plurals, measurements, `MessageFormat` and `printf` formatting.

use std::sync::{Arc, Weak};

use java_compat::lang::{JError, JMap, JObject, JValue};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;

use super::bean::get_property;
use super::classes::{Annotation, JavaClass};
use super::safe::{self, HasLocale, LocaleConfig, SafeConfig, Tool};
use super::values::ValueParser;
use super::{Ret, identity, o, object_methods, s, string_of, text, tool_object, val};

/// The Java class name of the tool.
pub const CLASS: &str = "org.apache.velocity.tools.generic.DisplayTool";
/// The configuration key of the list delimiter (default `, `).
pub const LIST_DELIM_KEY: &str = "listDelim";
/// The configuration key of the delimiter before a list's last item (default ` and `).
pub const LIST_FINAL_DELIM_KEY: &str = "listFinalDelim";
/// The configuration key of the default truncation length (default 30).
pub const TRUNCATE_LENGTH_KEY: &str = "truncateLength";
/// The configuration key of the suffix truncation appends (default `...`).
pub const TRUNCATE_SUFFIX_KEY: &str = "truncateSuffix";
/// The configuration key of whether truncation cuts at a word boundary.
pub const TRUNCATE_AT_WORD_KEY: &str = "truncateAtWord";
/// The configuration key of the default cell width (default 30).
pub const CELL_LENGTH_KEY: &str = "cellLength";
/// The configuration key of the suffix of a truncated cell (default `...`).
pub const CELL_SUFFIX_KEY: &str = "cellSuffix";
/// The configuration key of what `alt` gives for `null` (default `null`).
pub const DEFAULT_ALTERNATE_KEY: &str = "defaultAlternate";
/// The configuration key of the tags `stripTags` keeps.
pub const ALLOWED_TAGS_KEY: &str = "allowedTags";

#[derive(Clone)]
struct Settings {
    delim: Option<String>,
    final_delim: Option<String>,
    truncate_length: i32,
    truncate_suffix: Option<String>,
    truncate_at_word: bool,
    cell_length: i32,
    cell_suffix: Option<String>,
    default_alternate: Option<String>,
    allowed_tags: Option<Vec<Option<String>>>,
}

/// Port of `org.apache.velocity.tools.generic.DisplayTool`: display helpers with configurable defaults.
pub struct DisplayTool {
    locale: LocaleConfig,
    st: Mutex<Settings>,
    me: Weak<DisplayTool>,
}

impl DisplayTool {
    /// A tool with the upstream defaults.
    pub fn new() -> Arc<DisplayTool> {
        Arc::new_cyclic(|me| DisplayTool {
            locale: LocaleConfig::default(),
            st: Mutex::new(Settings {
                delim: Some(", ".into()),
                final_delim: Some(" and ".into()),
                truncate_length: 30,
                truncate_suffix: Some("...".into()),
                truncate_at_word: false,
                cell_length: 30,
                cell_suffix: Some("...".into()),
                default_alternate: Some("null".into()),
                allowed_tags: None,
            }),
            me: me.clone(),
        })
    }

    /// This tool as a template value, `Null` once it is dropped.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `configure(params)`: the locale, safe mode and the defaults the configuration keys set.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    fn settings(&self) -> Settings {
        self.st.lock().clone()
    }

    /// `getLocale()`: the configured locale.
    pub fn get_locale(&self) -> java_compat::text::Locale {
        self.locale.locale()
    }

    /// `getListDelimiter()`.
    pub fn get_list_delimiter(&self) -> Option<String> {
        self.settings().delim
    }

    /// `setListDelimiter(delim)`.
    pub fn set_list_delimiter(&self, v: Option<&str>) {
        self.st.lock().delim = v.map(str::to_string);
    }

    /// `getListFinalDelimiter()`.
    pub fn get_list_final_delimiter(&self) -> Option<String> {
        self.settings().final_delim
    }

    /// `setListFinalDelimiter(delim)`.
    pub fn set_list_final_delimiter(&self, v: Option<&str>) {
        self.st.lock().final_delim = v.map(str::to_string);
    }

    /// `getTruncateLength()`.
    pub fn get_truncate_length(&self) -> i32 {
        self.settings().truncate_length
    }

    /// `setTruncateLength(length)`.
    pub fn set_truncate_length(&self, v: i32) {
        self.st.lock().truncate_length = v;
    }

    /// `getTruncateSuffix()`.
    pub fn get_truncate_suffix(&self) -> Option<String> {
        self.settings().truncate_suffix
    }

    /// `setTruncateSuffix(suffix)`.
    pub fn set_truncate_suffix(&self, v: Option<&str>) {
        self.st.lock().truncate_suffix = v.map(str::to_string);
    }

    /// `getTruncateAtWord()`.
    pub fn get_truncate_at_word(&self) -> bool {
        self.settings().truncate_at_word
    }

    /// `setTruncateAtWord(atWord)`.
    pub fn set_truncate_at_word(&self, v: bool) {
        self.st.lock().truncate_at_word = v;
    }

    /// `getCellSuffix()`.
    pub fn get_cell_suffix(&self) -> Option<String> {
        self.settings().cell_suffix
    }

    /// `setCellSuffix(suffix)`.
    pub fn set_cell_suffix(&self, v: Option<&str>) {
        self.st.lock().cell_suffix = v.map(str::to_string);
    }

    /// `getCellLength()`.
    pub fn get_cell_length(&self) -> i32 {
        self.settings().cell_length
    }

    /// `setCellLength(length)`.
    pub fn set_cell_length(&self, v: i32) {
        self.st.lock().cell_length = v;
    }

    /// `getDefaultAlternate()`.
    pub fn get_default_alternate(&self) -> Option<String> {
        self.settings().default_alternate
    }

    /// `setDefaultAlternate(alternate)`.
    pub fn set_default_alternate(&self, v: Option<&str>) {
        self.st.lock().default_alternate = v.map(str::to_string);
    }

    /// `getAllowedTags()`: `None` when unset.
    pub fn get_allowed_tags(&self) -> Option<Vec<Option<String>>> {
        self.settings().allowed_tags
    }

    /// `setAllowedTags(tags)`.
    pub fn set_allowed_tags(&self, v: Option<Vec<Option<String>>>) {
        self.st.lock().allowed_tags = v;
    }

    /// `list(list)`: with the default delimiters.
    pub fn list(&self, l: &JValue) -> Result<Option<String>, JError> {
        let st = self.settings();
        list(l, st.delim.as_deref(), st.final_delim.as_deref(), None)
    }

    /// `list(list, delim)`: the same delimiter everywhere.
    pub fn list_delimited(&self, l: &JValue, delim: Option<&str>) -> Result<Option<String>, JError> {
        list(l, delim, delim, None)
    }

    /// `list(list, delim, finaldelim)`.
    pub fn list_with(&self, l: &JValue, delim: Option<&str>, final_delim: Option<&str>) -> Result<Option<String>, JError> {
        list(l, delim, final_delim, None)
    }

    /// `list(list, delim, finaldelim, property)`: each item's `property` listed instead of the item.
    pub fn list_property(&self, l: &JValue, delim: Option<&str>, final_delim: Option<&str>, property: Option<&str>) -> Result<Option<String>, JError> {
        list(l, delim, final_delim, property)
    }

    /// `message(format, args...)`: `MessageFormat.format` in the default locale; a lone collection argument
    /// stands for its elements.
    pub fn message(&self, format: Option<&str>, args: Option<&[JValue]>) -> Result<Option<String>, JError> {
        let Some(format) = format else { return Ok(None) };
        let Some(args) = spread(args) else { return Ok(Some(format.to_string())) };
        java_compat::text::message_format(format, &args, &super::default_locale()).map(Some)
    }

    /// `printf(format, args...)`: `String.format` in the tool's locale.
    pub fn printf(&self, format: Option<&str>, args: Option<&[JValue]>) -> Result<Option<String>, JError> {
        let Some(format) = format else { return Ok(None) };
        let Some(args) = spread(args) else { return Ok(Some(format.to_string())) };
        java_compat::text::string_format(&self.get_locale(), format, &args).map(Some)
    }

    /// `truncate(truncateMe)`: with the default length, suffix and word mode.
    pub fn truncate(&self, v: &JValue) -> Option<String> {
        let st = self.settings();
        truncate(v, st.truncate_length, st.truncate_suffix.as_deref(), st.truncate_at_word)
    }

    /// `truncate(truncateMe, maxLength)`.
    pub fn truncate_to(&self, v: &JValue, max: i32) -> Option<String> {
        let st = self.settings();
        truncate(v, max, st.truncate_suffix.as_deref(), st.truncate_at_word)
    }

    /// `truncate(truncateMe, suffix)`.
    pub fn truncate_suffixed(&self, v: &JValue, suffix: Option<&str>) -> Option<String> {
        let st = self.settings();
        truncate(v, st.truncate_length, suffix, st.truncate_at_word)
    }

    /// `truncate(truncateMe, maxLength, suffix)`.
    pub fn truncate_with(&self, v: &JValue, max: i32, suffix: Option<&str>) -> Option<String> {
        truncate(v, max, suffix, self.settings().truncate_at_word)
    }

    /// `truncate(truncateMe, maxLength, suffix, atWord)`.
    pub fn truncate_full(&self, v: &JValue, max: i32, suffix: Option<&str>, at_word: bool) -> Option<String> {
        truncate(v, max, suffix, at_word)
    }

    /// `space(length)`: that many spaces, `None` for a negative length.
    pub fn space(&self, length: i32) -> Option<String> {
        space(length)
    }

    /// `cell(obj)`: padded or truncated to the default cell length.
    pub fn cell(&self, v: &JValue) -> Option<String> {
        let st = self.settings();
        self.cell_with(v, st.cell_length, st.cell_suffix.as_deref())
    }

    /// `cell(obj, cellsize)`.
    pub fn cell_sized(&self, v: &JValue, size: i32) -> Option<String> {
        self.cell_with(v, size, self.settings().cell_suffix.as_deref())
    }

    /// `cell(obj, suffix)`.
    pub fn cell_suffixed(&self, v: &JValue, suffix: Option<&str>) -> Option<String> {
        self.cell_with(v, self.settings().cell_length, suffix)
    }

    /// `cell(obj, cellsize, suffix)`: padded with spaces or truncated (with the default truncation word mode).
    pub fn cell_with(&self, v: &JValue, size: i32, suffix: Option<&str>) -> Option<String> {
        cell(v, size, suffix, self.settings().truncate_at_word)
    }

    /// `capitalize(obj)`: the first character upper-cased; `None` for `null`.
    pub fn capitalize(&self, v: &JValue) -> Option<String> {
        recase_first(v, true)
    }

    /// `uncapitalize(obj)`: the first character lower-cased; `None` for `null`.
    pub fn uncapitalize(&self, v: &JValue) -> Option<String> {
        recase_first(v, false)
    }

    /// `alt(checkMe)`: the default alternate for `null`.
    pub fn alt(&self, v: &JValue) -> JValue {
        if v.is_null() { self.get_default_alternate().map(JValue::str).unwrap_or(JValue::Null) } else { v.clone() }
    }

    /// `alt(checkMe, alternate)`: `alternate` for `null`.
    pub fn alt_or(&self, v: &JValue, alternate: &JValue) -> JValue {
        if v.is_null() { alternate.clone() } else { v.clone() }
    }

    /// `br(obj)`: each newline preceded by `<br />`; `None` for `null`.
    pub fn br(&self, v: &JValue) -> Option<String> {
        string_of(v).map(|s| s.replace('\n', "<br />\n"))
    }

    /// `stripTags(obj)`: every tag but the configured allowed ones removed.
    pub fn strip_tags(&self, v: &JValue) -> Result<Option<String>, JError> {
        strip_tags(v, self.get_allowed_tags().as_deref())
    }

    /// `stripTags(obj, allowedTags...)`.
    pub fn strip_tags_allowing(&self, v: &JValue, tags: Option<&[Option<String>]>) -> Result<Option<String>, JError> {
        strip_tags(v, tags)
    }

    /// `plural(value, singular)`: the singular for 1 and -1, else an English plural of it.
    pub fn plural(&self, value: i32, singular: Option<&str>) -> Option<String> {
        plural(value, singular.map(str::to_string), None)
    }

    /// `plural(value, singular, plural)`: the singular for 1 and -1, else `p` (or an English plural when `None`).
    pub fn plural_with(&self, value: i32, singular: Option<&str>, p: Option<&str>) -> Option<String> {
        plural(value, singular.map(str::to_string), p.map(str::to_string))
    }

    /// `measure(obj)`: the text's line count and widest line; `None` for `null`.
    pub fn measure(&self, v: &JValue) -> Option<Measurements> {
        string_of(v).map(|s| Measurements::new(&s))
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

/// A varargs array: `None` when there is nothing to format with; a lone collection spread.
fn spread(args: Option<&[JValue]>) -> Option<Vec<JValue>> {
    let args = args.filter(|a| !a.is_empty())?;
    if args.len() == 1 && is_collection(&args[0]) {
        let items = args[0].iterate().unwrap_or_default();
        return (!items.is_empty()).then_some(items);
    }
    Some(args.to_vec())
}

fn is_collection(v: &JValue) -> bool {
    match v {
        JValue::List(_) => true,
        JValue::Object(_) => java_compat::reflect::class_def(v).is_some_and(|d| d.interfaces.iter().any(|i| &**i == "java.util.Collection")),
        _ => false,
    }
}

impl Tool for DisplayTool {
    fn safe(&self) -> &SafeConfig {
        &self.locale.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        self.locale.configure(values);
        if let Some(v) = values.get_string(LIST_DELIM_KEY) {
            self.set_list_delimiter(Some(&v));
        }
        if let Some(v) = values.get_string(LIST_FINAL_DELIM_KEY) {
            self.set_list_final_delimiter(Some(&v));
        }
        if let Some(v) = values.get_integer(TRUNCATE_LENGTH_KEY) {
            self.set_truncate_length(v);
        }
        if let Some(v) = values.get_string(TRUNCATE_SUFFIX_KEY) {
            self.set_truncate_suffix(Some(&v));
        }
        if let Some(v) = values.get_boolean(TRUNCATE_AT_WORD_KEY) {
            self.set_truncate_at_word(v);
        }
        if let Some(v) = values.get_integer(CELL_LENGTH_KEY) {
            self.set_cell_length(v);
        }
        if let Some(v) = values.get_string(CELL_SUFFIX_KEY) {
            self.set_cell_suffix(Some(&v));
        }
        if let Some(v) = values.get_string(DEFAULT_ALTERNATE_KEY) {
            self.set_default_alternate(Some(&v));
        }
        if let Some(v) = values.get_strings(ALLOWED_TAGS_KEY) {
            self.set_allowed_tags(Some(v));
        }
    }
}

impl HasLocale for DisplayTool {
    fn locale_config(&self) -> &LocaleConfig {
        &self.locale
    }
}

/// A Java string as UTF-16 code units, for `length()` / `substring` arithmetic.
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn from_units(u: &[u16]) -> String {
    String::from_utf16_lossy(u)
}

fn int_arg(v: &JValue) -> i32 {
    match v {
        JValue::Int(i) => *i,
        other => crate::math::int_value(other),
    }
}

/// `truncate(truncateMe, maxLength, suffix, atWord)`.
pub fn truncate(value: &JValue, max: i32, suffix: Option<&str>, at_word: bool) -> Option<String> {
    if value.is_null() || max <= 0 {
        return None;
    }
    let string = value.to_java_string();
    let u = units(&string);
    let max = max as usize;
    if u.len() <= max {
        return Some(string);
    }
    let suffix_len = suffix.map(|s| units(s).len()).unwrap_or(0);
    let Some(suffix) = suffix.filter(|_| max as i64 - suffix_len as i64 > 0) else { return Some(from_units(&u[..max])) };
    if at_word {
        let head = &u[..max - suffix_len + 1];
        if let Some(last_space) = head.iter().rposition(|c| *c == b' ' as u16) {
            if last_space > suffix_len {
                return Some(from_units(&u[..last_space]) + suffix);
            }
        }
    }
    Some(from_units(&u[..max - suffix_len]) + suffix)
}

/// `space(length)`: that many spaces, `None` for a negative length.
pub fn space(length: i32) -> Option<String> {
    (length >= 0).then(|| " ".repeat(length as usize))
}

/// `cell(obj, cellsize, suffix)`: padded with spaces to `size`, or truncated; `None` for `null` or a size below 1.
pub fn cell(value: &JValue, size: i32, suffix: Option<&str>, at_word: bool) -> Option<String> {
    if value.is_null() || size <= 0 {
        return None;
    }
    let v = value.to_java_string();
    let len = units(&v).len() as i32;
    if len == size {
        Some(v)
    } else if len > size {
        truncate(&JValue::str(v), size, suffix, at_word)
    } else {
        Some(v + &space(size - len).unwrap_or_default())
    }
}

/// `substring(0,1).toUpperCase() + substring(1)` (or lower case).
pub fn recase_first(value: &JValue, upper: bool) -> Option<String> {
    let string = string_of(value)?;
    let u = units(&string);
    let change = |s: String| if upper { s.to_uppercase() } else { s.to_lowercase() };
    Some(match u.len() {
        0 => string,
        1 => change(string),
        _ => change(from_units(&u[..1])) + &from_units(&u[1..]),
    })
}

fn items(list: &JValue) -> Vec<JValue> {
    match list {
        JValue::List(l) => l.items(),
        JValue::Array(a) => a.items(),
        JValue::Object(o) if java_compat::reflect::class_def(list).is_some_and(|d| d.interfaces.iter().any(|i| &**i == "java.util.Collection")) => o.iterate().unwrap_or_default(),
        other => vec![other.clone()],
    }
}

/// `list(list, delim, finaldelim, property)`: `null` for `null`; an item's property failing throws.
pub fn list(list: &JValue, delim: Option<&str>, final_delim: Option<&str>, property: Option<&str>) -> Result<Option<String>, JError> {
    if list.is_null() {
        return Ok(None);
    }
    let items = items(list);
    let size = items.len();
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        match property.filter(|p| !p.is_empty()) {
            Some(p) => {
                let v = get_property(item, p).map_err(|e| JError::new("java.lang.IllegalArgumentException", format!("Could not retrieve '{p}' from {}: {e}", item.to_java_string())))?;
                out.push_str(&v.to_java_string());
            }
            None => out.push_str(&item.to_java_string()),
        }
        if i + 2 < size {
            out.push_str(delim.unwrap_or("null"));
        } else if i + 1 < size {
            out.push_str(final_delim.unwrap_or("null"));
        }
    }
    Ok(Some(out))
}

/// `stripTags(obj, allowedTags...)`: every tag but the allowed ones removed.
pub fn strip_tags(value: &JValue, allowed: Option<&[Option<String>]>) -> Result<Option<String>, JError> {
    let Some(v) = string_of(value) else { return Ok(None) };
    let tags: Vec<String> = allowed
        .unwrap_or(&[])
        .iter()
        .flatten()
        .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric()))
        .cloned()
        .collect();
    let rule = if tags.is_empty() { "(?i)<[^>]*?>".to_string() } else { format!("(?i)<(?!/?({})[\\s>/])[^>]*?>", tags.join("|")) };
    let re = java_compat::methods::java_regex(&rule)?;
    Ok(Some(re.replace_all(&v, "").into_owned()))
}

/// `plural(value, singular, plural)`: the singular for 1 and -1, else `plural`, else the singular with `s`, `es` or `ies` (upper case after a capital last letter).
pub fn plural(value: i32, singular: Option<String>, plural: Option<String>) -> Option<String> {
    if value == 1 || value == -1 {
        return singular;
    }
    if plural.is_some() {
        return plural;
    }
    let singular = singular?;
    if singular.is_empty() {
        return Some(singular);
    }
    let last = singular.chars().last().expect("not empty").to_string();
    let is_capital = last.to_lowercase() != last;
    let word = singular.to_lowercase();
    if word.ends_with('x') || word.ends_with("sh") || word.ends_with("ch") || word.ends_with('s') {
        return Some(singular + if is_capital { "ES" } else { "es" });
    }
    let wu = units(&word);
    if wu.len() > 1 && word.ends_with('y') && !matches!(from_units(&wu[wu.len() - 2..wu.len() - 1]).as_str(), "a" | "e" | "i" | "o" | "u") {
        let su = units(&singular);
        return Some(from_units(&su[..su.len() - 1]) + if is_capital { "IES" } else { "ies" });
    }
    Some(singular + if is_capital { "S" } else { "s" })
}

/// `DisplayTool.Measurements`.
pub struct Measurements {
    height: i32,
    width: i32,
}

impl Measurements {
    /// `getHeight()`: the number of lines.
    pub fn get_height(&self) -> i32 {
        self.height
    }

    /// `getWidth()`: the length of the longest line in UTF-16 units.
    pub fn get_width(&self) -> i32 {
        self.width
    }

    /// The measurements of `s`.
    pub fn new(s: &str) -> Measurements {
        let lines = java_compat::methods::split(s, "\n", 0).unwrap_or_default();
        let width = lines.iter().map(|l| units(l).len() as i32).max().unwrap_or(0).max(0);
        Measurements { height: lines.len() as i32, width }
    }

    fn java_string(&self) -> String {
        format!("{{ height: {}, width: {} }}", self.height, self.width)
    }
}

fn measurements_def() -> Arc<ClassDef> {
    let def = ClassDef::new("org.apache.velocity.tools.generic.DisplayTool$Measurements")
        .method::<Measurements>("getHeight", &[], Some(JType::Int), |m, _, _| val(JValue::Int(m.height)))
        .method::<Measurements>("getWidth", &[], Some(JType::Int), |m, _, _| val(JValue::Int(m.width)))
        .method::<Measurements>("toString", &[], Some(s()), |m, _, _| val(JValue::str(m.java_string())));
    object_methods::<Measurements>(def).register()
}

tool_object!(Measurements, measurements_def);

/// A `String[]` value.
pub(crate) fn strings_value(v: Vec<Option<String>>) -> JValue {
    JValue::Array(Arc::new(java_compat::lang::JArray::new(s(), v.into_iter().map(|x| x.map(JValue::str).unwrap_or(JValue::Null)).collect())))
}

fn opt_text(v: Option<String>) -> Ret {
    text(v)
}

fn strings_arg(v: &JValue) -> Option<Vec<Option<String>>> {
    match v {
        JValue::Array(a) => Some(a.items().iter().map(|t| t.as_str().map(str::to_string)).collect()),
        _ => None,
    }
}

fn varargs(v: &JValue) -> Option<Vec<JValue>> {
    match v {
        JValue::Array(a) => Some(a.items()),
        JValue::Null => None,
        other => Some(vec![other.clone()]),
    }
}

fn def() -> Arc<ClassDef> {
    type D = DisplayTool;
    let int = || JType::Int;
    let def = ClassDef::new(CLASS)
        .extends(&[safe::LOCALE_CONFIG, safe::SAFE_CONFIG])
        .implements(&["java.io.Serializable"])
        .method::<D>("getListDelimiter", &[], Some(s()), |t, _, _| opt_text(t.get_list_delimiter()))
        .method::<D>("getListFinalDelimiter", &[], Some(s()), |t, _, _| opt_text(t.get_list_final_delimiter()))
        .method::<D>("getTruncateLength", &[], Some(int()), |t, _, _| val(JValue::Int(t.get_truncate_length())))
        .method::<D>("getTruncateSuffix", &[], Some(s()), |t, _, _| opt_text(t.get_truncate_suffix()))
        .method::<D>("getTruncateAtWord", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.get_truncate_at_word())))
        .method::<D>("getCellSuffix", &[], Some(s()), |t, _, _| opt_text(t.get_cell_suffix()))
        .method::<D>("getCellLength", &[], Some(int()), |t, _, _| val(JValue::Int(t.get_cell_length())))
        .method::<D>("getDefaultAlternate", &[], Some(s()), |t, _, _| opt_text(t.get_default_alternate()))
        .method::<D>("getAllowedTags", &[], Some(JType::array(s())), |t, _, _| {
            val(t.get_allowed_tags().map(strings_value).unwrap_or(JValue::Null))
        })
        .method::<D>("list", &[o()], Some(s()), |t, _, a| text(t.list(&a[0])?))
        .method::<D>("list", &[o(), s()], Some(s()), |t, _, a| text(t.list_delimited(&a[0], a[1].as_str())?))
        .method::<D>("list", &[o(), s(), s()], Some(s()), |t, _, a| text(t.list_with(&a[0], a[1].as_str(), a[2].as_str())?))
        .method::<D>("list", &[o(), s(), s(), s()], Some(s()), |t, _, a| text(t.list_property(&a[0], a[1].as_str(), a[2].as_str(), a[3].as_str())?))
        .method::<D>("message", &[s(), JType::array(o())], Some(s()), |t, _, a| text(t.message(a[0].as_str(), varargs(&a[1]).as_deref())?))
        .method::<D>("printf", &[s(), JType::array(o())], Some(s()), |t, _, a| text(t.printf(a[0].as_str(), varargs(&a[1]).as_deref())?))
        .method::<D>("truncate", &[o()], Some(s()), |t, _, a| text(t.truncate(&a[0])))
        .method::<D>("truncate", &[o(), int()], Some(s()), |t, _, a| text(t.truncate_to(&a[0], int_arg(&a[1]))))
        .method::<D>("truncate", &[o(), s()], Some(s()), |t, _, a| text(t.truncate_suffixed(&a[0], a[1].as_str())))
        .method::<D>("truncate", &[o(), int(), s()], Some(s()), |t, _, a| text(t.truncate_with(&a[0], int_arg(&a[1]), a[2].as_str())))
        .method::<D>("truncate", &[o(), int(), s(), JType::Boolean], Some(s()), |t, _, a| text(t.truncate_full(&a[0], int_arg(&a[1]), a[2].as_str(), matches!(a[3], JValue::Bool(true)))))
        .method::<D>("space", &[int()], Some(s()), |t, _, a| text(t.space(int_arg(&a[0]))))
        .method::<D>("cell", &[o()], Some(s()), |t, _, a| text(t.cell(&a[0])))
        .method::<D>("cell", &[o(), int()], Some(s()), |t, _, a| text(t.cell_sized(&a[0], int_arg(&a[1]))))
        .method::<D>("cell", &[o(), s()], Some(s()), |t, _, a| text(t.cell_suffixed(&a[0], a[1].as_str())))
        .method::<D>("cell", &[o(), int(), s()], Some(s()), |t, _, a| text(t.cell_with(&a[0], int_arg(&a[1]), a[2].as_str())))
        .method::<D>("capitalize", &[o()], Some(s()), |t, _, a| text(t.capitalize(&a[0])))
        .method::<D>("uncapitalize", &[o()], Some(s()), |t, _, a| text(t.uncapitalize(&a[0])))
        .method::<D>("alt", &[o()], Some(o()), |t, _, a| val(t.alt(&a[0])))
        .method::<D>("alt", &[o(), o()], Some(o()), |t, _, a| val(t.alt_or(&a[0], &a[1])))
        .method::<D>("br", &[o()], Some(s()), |t, _, a| text(t.br(&a[0])))
        .method::<D>("stripTags", &[o()], Some(s()), |t, _, a| text(t.strip_tags(&a[0])?))
        .method::<D>("stripTags", &[o(), JType::array(s())], Some(s()), |t, _, a| text(t.strip_tags_allowing(&a[0], strings_arg(&a[1]).as_deref())?))
        .method::<D>("plural", &[int(), s()], Some(s()), |t, _, a| text(t.plural(int_arg(&a[0]), a[1].as_str())))
        .method::<D>("plural", &[int(), s(), s()], Some(s()), |t, _, a| text(t.plural_with(int_arg(&a[0]), a[1].as_str(), a[2].as_str())))
        .method::<D>("measure", &[o()], Some(JType::class("org.apache.velocity.tools.generic.DisplayTool$Measurements")), |t, _, a| {
            val(match t.measure(&a[0]) {
                Some(m) => JValue::Object(Arc::new(m)),
                None => JValue::Null,
            })
        });
    object_methods::<D>(safe::locale_config_methods::<D>(def)).register()
}

tool_object!(DisplayTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || DisplayTool::new().value()).annotated(Annotation::default_key("display")).annotated(Annotation::valid_scope(&[super::scope::APPLICATION]))
}
