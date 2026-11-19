//! `org.apache.velocity.tools.generic.ValueParser`: typed access to a map of values (the tools' configuration,
//! request parameters), with sub-keys (`a.b` read as `$parser.a.b`).

use std::collections::BTreeSet;
use std::sync::{Arc, Weak};

use java_compat::lang::{JArray, JError, JList, JMap, JObject, JValue, ListKind, MapKind};
use java_compat::reflect::{ClassDef, JType};
use java_compat::text::Locale;
use parking_lot::Mutex;

use super::conversion;
use super::num;
use super::safe::{self, FormatConfig, HasFormat, HasLocale, SafeConfig, Tool};
use super::{c, o, s, tool_object, val};

/// `ValueParser.STRINGS_DELIMITER_FORMAT_KEY`: the configuration key for the delimiter `getValues` splits strings
/// around.
pub const STRINGS_DELIMITER_FORMAT_KEY: &str = "stringsDelimiter";
/// `ValueParser.DEFAULT_STRINGS_DELIMITER`: `,`.
pub const DEFAULT_STRINGS_DELIMITER: &str = ",";
/// `ValueParser.ALLOWSUBKEYS_KEY`: the configuration key for treating `a.b` keys as sub-keys of `a`.
pub const ALLOWSUBKEYS_KEY: &str = "allowSubkeys";
/// `ValueParser.READONLY_KEY`: the configuration key for refusing `put`/`remove`.
pub const READONLY_KEY: &str = "readOnly";
/// The class name of `org.apache.velocity.tools.generic.ValueParser`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.ValueParser";

struct State {
    delimiter: String,
    source: Option<JMap>,
    allow_subkeys: bool,
    has_subkeys: Option<bool>,
    read_only: bool,
}

/// Port of `org.apache.velocity.tools.generic.ValueParser`: typed access to a map's values (strings, booleans, numbers,
/// locales and their arrays), with dotted sub-keys.
pub struct ValueParser {
    config: FormatConfig,
    state: Mutex<State>,
    me: Weak<ValueParser>,
}

impl ValueParser {
    /// `new ValueParser()` / `new ValueParser(source)`: the map itself, not a copy.
    pub fn new(source: Option<JMap>) -> Arc<ValueParser> {
        Arc::new_cyclic(|me| ValueParser {
            config: FormatConfig::default(),
            state: Mutex::new(State { delimiter: DEFAULT_STRINGS_DELIMITER.into(), source, allow_subkeys: true, has_subkeys: None, read_only: true }),
            me: me.clone(),
        })
    }

    /// A parser over a copy of `entries`.
    pub fn of(entries: &[(&str, JValue)]) -> Arc<ValueParser> {
        let m = JMap::new(MapKind::Hash);
        for (k, v) in entries {
            m.put_str(k, v.clone());
        }
        ValueParser::new(Some(m))
    }

    /// This parser as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `getSource()`: created empty on first use.
    pub fn source_map(&self) -> JMap {
        self.state.lock().source.get_or_insert_with(|| JMap::new(MapKind::Hash)).clone()
    }

    fn source_opt(&self) -> Option<JMap> {
        self.state.lock().source.clone()
    }

    /// `setSource(source)`: the map itself, not a copy.
    pub fn set_source(&self, source: Option<JMap>) {
        self.state.lock().source = source;
    }

    /// `getAllowSubkeys()`: whether `get` falls back to sub-keys, `true` unless configured.
    pub fn allow_subkeys(&self) -> bool {
        self.state.lock().allow_subkeys
    }

    /// `setAllowSubkeys(allow)`.
    pub fn set_allow_subkeys(&self, allow: bool) {
        self.state.lock().allow_subkeys = allow;
    }

    /// `getReadOnly()`: whether `put`/`remove` throw `UnsupportedOperationException`, `true` unless configured.
    pub fn read_only(&self) -> bool {
        self.state.lock().read_only
    }

    /// `setReadOnly(ro)`.
    pub fn set_read_only(&self, ro: bool) {
        self.state.lock().read_only = ro;
    }

    /// `setStringsDelimiter(d)`: the regular expression `getValues` splits strings around.
    pub fn set_strings_delimiter(&self, d: &str) {
        self.state.lock().delimiter = d.to_string();
    }

    /// `getFormat()`: the default number format names parse with.
    pub fn format(&self) -> String {
        self.config.format()
    }

    /// `getLocale()`: the locale numbers parse with.
    pub fn locale(&self) -> Locale {
        self.config.locale()
    }

    /// The parser's `FormatConfig` state (format and locale).
    pub fn config(&self) -> &FormatConfig {
        &self.config
    }

    /// `exists(key)`.
    pub fn exists(&self, key: &str) -> bool {
        !self.get_value(key).is_null()
    }

    /// `get(key)`: the value, else the sub-keys under `key.` as a new parser.
    pub fn get(&self, key: &str) -> JValue {
        let v = self.get_value(key);
        if v.is_null() && self.allow_subkeys() {
            if let Some(sub) = self.get_subkey(Some(key)) {
                return sub.value();
            }
        }
        v
    }

    /// `getValue(key)`.
    pub fn get_value(&self, key: &str) -> JValue {
        self.source_map().get_str(key).unwrap_or(JValue::Null)
    }

    /// `getValue(key, alternate)`: `alternate` when the key has no value or a `null` one.
    pub fn get_value_or(&self, key: &str, alternate: JValue) -> JValue {
        let v = self.get_value(key);
        if v.is_null() { alternate } else { v }
    }

    /// `parseStringList`: split around the delimiter (a regular expression), unless it does not occur.
    fn parse_string_list(&self, value: &str) -> Vec<String> {
        let d = self.state.lock().delimiter.clone();
        if d.is_empty() || !value.contains(&d) {
            return vec![value.to_string()];
        }
        java_compat::methods::split(value, &d, 0).unwrap_or_else(|_| vec![value.to_string()])
    }

    /// `getValues(key)`: a string split into a `String[]`, an `Object[]` as is, anything else wrapped.
    pub fn get_values(&self, key: &str) -> Option<JValue> {
        let v = self.get_value(key);
        match &v {
            JValue::Null => None,
            JValue::Str(s) => Some(JArray::strings(self.parse_string_list(s))),
            JValue::Array(a) if !a.component.is_primitive() => Some(v.clone()),
            other => Some(JArray::objects(vec![other.clone()])),
        }
    }

    fn values_items(&self, key: &str) -> Option<(JType, Vec<JValue>)> {
        match self.get_values(key)? {
            JValue::Array(a) => Some((a.component.clone(), a.items())),
            _ => None,
        }
    }

    /// `getString(key)`: the value converted to a string, `None` when absent.
    pub fn get_string(&self, key: &str) -> Option<String> {
        conversion::to_string(&self.get_value(key))
    }

    /// `getString(key, alternate)`: `alternate` when absent or not convertible.
    pub fn get_string_or(&self, key: &str, alternate: Option<&str>) -> Option<String> {
        self.get_string(key).or_else(|| alternate.map(str::to_string))
    }

    /// `getBoolean(key)`: `true` for a `true` value or the string `true` in any case, `false` for anything else, `None`
    /// when absent.
    pub fn get_boolean(&self, key: &str) -> Option<bool> {
        conversion::to_boolean(&self.get_value(key))
    }

    /// `getBoolean(key, alternate)`: `alternate` when absent.
    pub fn get_boolean_or(&self, key: &str, alternate: bool) -> bool {
        self.get_boolean(key).unwrap_or(alternate)
    }

    fn number_of(&self, v: &JValue) -> Option<JValue> {
        conversion::to_number_in(v, &self.format(), &self.locale())
    }

    /// `getInteger(key)`: the value parsed as a number in the configured format and locale and narrowed to `int`;
    /// `None` when absent or unparseable.
    pub fn get_integer(&self, key: &str) -> Option<i32> {
        let v = self.get_value(key);
        if v.is_null() {
            return None;
        }
        self.number_of(&v).map(|n| num::int_value(&n))
    }

    /// `getLong(key)`: as `get_integer`, narrowed to `long`.
    pub fn get_long(&self, key: &str) -> Option<i64> {
        let v = self.get_value(key);
        if v.is_null() {
            return None;
        }
        self.number_of(&v).map(|n| num::long_value(&n))
    }

    /// `getDouble(key)`: as `get_integer`, as a `double`.
    pub fn get_double(&self, key: &str) -> Option<f64> {
        let v = self.get_value(key);
        if v.is_null() {
            return None;
        }
        self.number_of(&v).map(|n| num::double_value(&n))
    }

    /// `getNumber(key)`.
    pub fn get_number(&self, key: &str) -> Option<JValue> {
        let v = self.get_value(key);
        if v.is_null() {
            return None;
        }
        self.number_of(&v)
    }

    /// `getInt(key, alternate)`: `alternate` when absent or not a number.
    pub fn get_int(&self, key: &str, alternate: i32) -> i32 {
        self.get_number(key).map(|n| num::int_value(&n)).unwrap_or(alternate)
    }

    /// `getLocale(key)`: a `Locale` value as is, a string such as `fr_CA` converted; `None` when absent or invalid.
    pub fn get_locale(&self, key: &str) -> Option<Locale> {
        safe::to_locale(&self.get_value(key))
    }

    /// `getStrings(key)`.
    pub fn get_strings(&self, key: &str) -> Option<Vec<Option<String>>> {
        let (component, items) = self.values_items(key)?;
        if component == JType::string() {
            return Some(items.iter().map(|v| v.as_str().map(str::to_string)).collect());
        }
        Some(items.iter().map(conversion::to_string).collect())
    }

    /// `getBooleans(key)`: each of `getValues(key)` converted as `get_boolean` converts, `None` for a `null` element.
    pub fn get_booleans(&self, key: &str) -> Option<Vec<Option<bool>>> {
        let (_, items) = self.values_items(key)?;
        Some(items.iter().map(conversion::to_boolean).collect())
    }

    /// `getNumbers(key)`: each of `getValues(key)` as a number (parsed when not one already), `None` for those that do
    /// not parse.
    pub fn get_numbers(&self, key: &str) -> Option<Vec<Option<JValue>>> {
        let (_, items) = self.values_items(key)?;
        Some(items.iter().map(|v| if num::is_number(v) { Some(v.clone()) } else { self.number_of(v) }).collect())
    }

    /// `getInts(key)`: `NullPointerException` for an element that is not a number.
    pub fn get_ints(&self, key: &str) -> Result<Option<Vec<i32>>, JError> {
        let Some((_, items)) = self.values_items(key) else { return Ok(None) };
        items.iter().map(|v| self.number_of(v).map(|n| num::int_value(&n)).ok_or_else(super::npe)).collect::<Result<Vec<_>, _>>().map(Some)
    }

    /// `getDoubles(key)`: `NullPointerException` for an element that is not a number.
    pub fn get_doubles(&self, key: &str) -> Result<Option<Vec<f64>>, JError> {
        let Some((_, items)) = self.values_items(key) else { return Ok(None) };
        items.iter().map(|v| self.number_of(v).map(|n| num::double_value(&n)).ok_or_else(super::npe)).collect::<Result<Vec<_>, _>>().map(Some)
    }

    /// `getLocales(key)`: each of `getValues(key)` as a locale, `None` for those that do not convert.
    pub fn get_locales(&self, key: &str) -> Option<Vec<Option<Locale>>> {
        let (_, items) = self.values_items(key)?;
        Some(items.iter().map(|v| java_compat::text::objects::as_locale(v).or_else(|| conversion::to_locale(&v.to_java_string()))).collect())
    }

    /// `hasSubkeys()`.
    pub fn has_subkeys(&self) -> bool {
        if !self.allow_subkeys() {
            return false;
        }
        let source = self.source_map();
        let mut st = self.state.lock();
        if st.has_subkeys.is_none() {
            let found = source.entries().iter().any(|(k, _)| k.as_str().and_then(|k| k.find('.')).is_some_and(|dot| dot > 0));
            st.has_subkeys = Some(found);
        }
        st.has_subkeys.unwrap_or(false)
    }

    /// `getSubkeys()`: the distinct prefixes before a dot, sorted.
    pub fn get_subkeys(&self) -> Vec<String> {
        let keys = self.key_set();
        if self.source_opt().is_none() || !self.allow_subkeys() {
            return keys;
        }
        let set: BTreeSet<String> = keys.iter().filter_map(|k| k.find('.').filter(|d| *d > 0).map(|d| k[..d].to_string())).collect();
        set.into_iter().collect()
    }

    /// `getSubkey(subkey)`.
    pub fn get_subkey(&self, subkey: Option<&str>) -> Option<Arc<ValueParser>> {
        let subkey = subkey.filter(|s| !s.is_empty())?;
        if !self.has_subkeys() {
            return None;
        }
        let prefix = format!("{subkey}.");
        let mut values: Option<JMap> = None;
        for (k, v) in self.source_map().entries() {
            let Some(k) = k.as_str() else { continue };
            if k.starts_with(&prefix) && k.len() > prefix.len() {
                values.get_or_insert_with(|| JMap::new(MapKind::Hash)).put_str(&k[prefix.len()..], v);
            }
        }
        let parser = ValueParser::new(Some(values?));
        parser.set_read_only(self.read_only());
        Some(parser)
    }

    /// `size()`: the number of entries in the source map.
    pub fn size(&self) -> usize {
        self.source_map().len()
    }

    /// `isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.source_map().is_empty()
    }

    /// `containsKey(key)`.
    pub fn contains_key(&self, key: &JValue) -> bool {
        self.source_map().contains_key(key)
    }

    /// `put(key, value)`: the previous value; `UnsupportedOperationException` when read-only.
    pub fn put(&self, key: &str, value: JValue) -> Result<JValue, JError> {
        if self.read_only() {
            return Err(JError::new("java.lang.UnsupportedOperationException", format!("Cannot put({key},{}); {CLASS} is read-only", value.to_java_string())));
        }
        let source = self.source_map();
        {
            let mut st = self.state.lock();
            if st.has_subkeys == Some(false) && key.contains('.') {
                st.has_subkeys = Some(true);
            }
        }
        Ok(source.put(JValue::str(key), value))
    }

    /// `remove(key)`: the removed value; `UnsupportedOperationException` when read-only.
    pub fn remove(&self, key: &JValue) -> Result<JValue, JError> {
        if self.read_only() {
            return Err(JError::new("java.lang.UnsupportedOperationException", format!("Cannot remove({}); {CLASS} is read-only", key.to_java_string())));
        }
        {
            let mut st = self.state.lock();
            if st.has_subkeys == Some(true) && key.as_str().is_some_and(|k| k.contains('.')) {
                st.has_subkeys = None;
            }
        }
        Ok(self.source_map().remove(key))
    }

    /// `keySet()`: the source map's keys as strings, in its iteration order.
    pub fn key_set(&self) -> Vec<String> {
        self.source_map().entries().iter().map(|(k, _)| k.to_java_string()).collect()
    }

    fn java_string(&self) -> String {
        let items: Vec<String> = self.source_map().entries().iter().map(|(k, v)| format!("{}={}", k.to_java_string(), v.to_java_string())).collect();
        format!("{{{}}}", items.join(", "))
    }
}

impl Tool for ValueParser {
    fn safe(&self) -> &SafeConfig {
        &self.config.locale.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        self.config.configure(values);
        if let Some(d) = values.get_string(STRINGS_DELIMITER_FORMAT_KEY) {
            self.set_strings_delimiter(&d);
        }
        if let Some(a) = values.get_boolean(ALLOWSUBKEYS_KEY) {
            self.set_allow_subkeys(a);
        }
        if let Some(r) = values.get_boolean(READONLY_KEY) {
            self.set_read_only(r);
        }
    }
}

impl HasLocale for ValueParser {
    fn locale_config(&self) -> &safe::LocaleConfig {
        &self.config.locale
    }
}

impl HasFormat for ValueParser {
    fn format_config(&self) -> &FormatConfig {
        &self.config
    }
}

/// The parser behind a `ValueParser` value.
pub fn as_value_parser(v: &JValue) -> Option<Arc<ValueParser>> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<ValueParser>().and_then(|p| p.me.upgrade()),
        _ => None,
    }
}

fn opt_str(v: Option<String>) -> JValue {
    v.map(JValue::str).unwrap_or(JValue::Null)
}

fn key(a: &JValue) -> String {
    a.as_str().map(str::to_string).unwrap_or_else(|| a.to_java_string())
}

fn def() -> Arc<ClassDef> {
    type P = ValueParser;
    let set = || c("java.util.Set");
    let number = || c("java.lang.Number");
    let locale = || c("java.util.Locale");
    let boolean_box = || c("java.lang.Boolean");
    let def = ClassDef::new(CLASS)
        .extends(&[safe::FORMAT_CONFIG, safe::LOCALE_CONFIG, safe::SAFE_CONFIG])
        .implements(&["java.util.Map"])
        .method::<P>("exists", &[s()], Some(JType::Boolean), |p, _, a| val(JValue::Bool(p.exists(&key(&a[0])))))
        .method::<P>("get", &[s()], Some(o()), |p, _, a| val(p.get(&key(&a[0]))))
        .method::<P>("get", &[o()], Some(o()), |p, _, a| val(p.get(&a[0].to_java_string())))
        .method::<P>("getValue", &[s()], Some(o()), |p, _, a| val(p.get_value(&key(&a[0]))))
        .method::<P>("getValue", &[s(), o()], Some(o()), |p, _, a| val(p.get_value_or(&key(&a[0]), a[1].clone())))
        .method::<P>("getValues", &[s()], Some(JType::array(o())), |p, _, a| val(p.get_values(&key(&a[0])).unwrap_or(JValue::Null)))
        .method::<P>("getString", &[s()], Some(s()), |p, _, a| val(opt_str(p.get_string(&key(&a[0])))))
        .method::<P>("getString", &[s(), s()], Some(s()), |p, _, a| val(opt_str(p.get_string_or(&key(&a[0]), a[1].as_str()))))
        .method::<P>("getBoolean", &[s()], Some(boolean_box()), |p, _, a| val(p.get_boolean(&key(&a[0])).map(JValue::Bool).unwrap_or(JValue::Null)))
        .method::<P>("getBoolean", &[s(), JType::Boolean], Some(JType::Boolean), |p, _, a| val(JValue::Bool(p.get_boolean_or(&key(&a[0]), matches!(a[1], JValue::Bool(true))))))
        .method::<P>("getBoolean", &[s(), boolean_box()], Some(boolean_box()), |p, _, a| val(p.get_boolean(&key(&a[0])).map(JValue::Bool).unwrap_or_else(|| a[1].clone())))
        .method::<P>("getInteger", &[s()], Some(c("java.lang.Integer")), |p, _, a| val(p.get_integer(&key(&a[0])).map(JValue::Int).unwrap_or(JValue::Null)))
        .method::<P>("getInteger", &[s(), c("java.lang.Integer")], Some(c("java.lang.Integer")), |p, _, a| val(p.get_integer(&key(&a[0])).map(JValue::Int).unwrap_or_else(|| a[1].clone())))
        .method::<P>("getLong", &[s()], Some(c("java.lang.Long")), |p, _, a| val(p.get_long(&key(&a[0])).map(JValue::Long).unwrap_or(JValue::Null)))
        .method::<P>("getLong", &[s(), c("java.lang.Long")], Some(c("java.lang.Long")), |p, _, a| val(p.get_long(&key(&a[0])).map(JValue::Long).unwrap_or_else(|| a[1].clone())))
        .method::<P>("getDouble", &[s()], Some(c("java.lang.Double")), |p, _, a| val(p.get_double(&key(&a[0])).map(JValue::Double).unwrap_or(JValue::Null)))
        .method::<P>("getDouble", &[s(), c("java.lang.Double")], Some(c("java.lang.Double")), |p, _, a| val(p.get_double(&key(&a[0])).map(JValue::Double).unwrap_or_else(|| a[1].clone())))
        .method::<P>("getNumber", &[s()], Some(number()), |p, _, a| val(p.get_number(&key(&a[0])).unwrap_or(JValue::Null)))
        .method::<P>("getLocale", &[s()], Some(locale()), |p, _, a| val(p.get_locale(&key(&a[0])).map(|l| l.value()).unwrap_or(JValue::Null)))
        .method::<P>("getNumber", &[s(), number()], Some(number()), |p, _, a| val(p.get_number(&key(&a[0])).unwrap_or_else(|| a[1].clone())))
        .method::<P>("getInt", &[s(), JType::Int], Some(JType::Int), |p, _, a| val(JValue::Int(p.get_int(&key(&a[0]), num::int_value(&a[1])))))
        .method::<P>("getDouble", &[s(), JType::Double], Some(JType::Double), |p, _, a| {
            val(JValue::Double(p.get_number(&key(&a[0])).map(|n| num::double_value(&n)).unwrap_or(num::double_value(&a[1]))))
        })
        .method::<P>("getLocale", &[s(), locale()], Some(locale()), |p, _, a| val(p.get_locale(&key(&a[0])).map(|l| l.value()).unwrap_or_else(|| a[1].clone())))
        .method::<P>("getStrings", &[s()], Some(JType::array(s())), |p, _, a| {
            val(p.get_strings(&key(&a[0])).map(|v| JValue::Array(Arc::new(JArray::new(s(), v.into_iter().map(opt_str).collect())))).unwrap_or(JValue::Null))
        })
        .method::<P>("getBooleans", &[s()], Some(JType::array(boolean_box())), |p, _, a| {
            val(p.get_booleans(&key(&a[0])).map(|v| JValue::Array(Arc::new(JArray::new(c("java.lang.Boolean"), v.into_iter().map(|b| b.map(JValue::Bool).unwrap_or(JValue::Null)).collect())))).unwrap_or(JValue::Null))
        })
        .method::<P>("getNumbers", &[s()], Some(JType::array(number())), |p, _, a| {
            val(p.get_numbers(&key(&a[0])).map(|v| JValue::Array(Arc::new(JArray::new(c("java.lang.Number"), v.into_iter().map(|n| n.unwrap_or(JValue::Null)).collect())))).unwrap_or(JValue::Null))
        })
        .method::<P>("getInts", &[s()], Some(JType::array(JType::Int)), |p, _, a| {
            val(p.get_ints(&key(&a[0]))?.map(|v| JValue::Array(Arc::new(JArray::new(JType::Int, v.into_iter().map(JValue::Int).collect())))).unwrap_or(JValue::Null))
        })
        .method::<P>("getDoubles", &[s()], Some(JType::array(JType::Double)), |p, _, a| {
            val(p.get_doubles(&key(&a[0]))?.map(|v| JValue::Array(Arc::new(JArray::new(JType::Double, v.into_iter().map(JValue::Double).collect())))).unwrap_or(JValue::Null))
        })
        .method::<P>("getLocales", &[s()], Some(JType::array(locale())), |p, _, a| {
            val(p.get_locales(&key(&a[0])).map(|v| JValue::Array(Arc::new(JArray::new(c("java.util.Locale"), v.into_iter().map(|l| l.map(|l| l.value()).unwrap_or(JValue::Null)).collect())))).unwrap_or(JValue::Null))
        })
        .method::<P>("hasSubkeys", &[], Some(JType::Boolean), |p, _, _| val(JValue::Bool(p.has_subkeys())))
        .method::<P>("getSubkeys", &[], Some(set()), |p, _, _| val(JValue::List(JList::new(ListKind::Set, p.get_subkeys().into_iter().map(JValue::str).collect()))))
        .method::<P>("getSubkey", &[s()], Some(c(CLASS)), |p, _, a| val(p.get_subkey(a[0].as_str()).map(|v| v.value()).unwrap_or(JValue::Null)))
        .method::<P>("size", &[], Some(JType::Int), |p, _, _| val(JValue::Int(p.size() as i32)))
        .method::<P>("isEmpty", &[], Some(JType::Boolean), |p, _, _| val(JValue::Bool(p.is_empty())))
        .method::<P>("containsKey", &[o()], Some(JType::Boolean), |p, _, a| val(JValue::Bool(p.contains_key(&a[0]))))
        .method::<P>("containsValue", &[o()], Some(JType::Boolean), |p, _, a| {
            val(JValue::Bool(p.source_map().entries().iter().any(|(_, v)| java_compat::lang::java_equals(v, &a[0]))))
        })
        .method::<P>("put", &[s(), o()], Some(o()), |p, _, a| val(p.put(&key(&a[0]), a[1].clone())?))
        .method::<P>("remove", &[o()], Some(o()), |p, _, a| val(p.remove(&a[0])?))
        .method::<P>("putAll", &[c("java.util.Map")], None, |p, _, a| {
            if p.read_only() {
                return Err(JError::new("java.lang.UnsupportedOperationException", format!("Cannot putAll({}); {CLASS} is read-only", a[0].to_java_string())));
            }
            p.state.lock().has_subkeys = None;
            let source = p.source_map();
            if let JValue::Map(m) = &a[0] {
                for (k, v) in m.entries() {
                    source.put(k, v);
                }
            }
            Ok(None)
        })
        .method::<P>("clear", &[], None, |p, _, _| {
            if p.read_only() {
                return Err(JError::new("java.lang.UnsupportedOperationException", format!("Cannot clear(); {CLASS} is read-only")));
            }
            p.state.lock().has_subkeys = Some(false);
            let source = p.source_map();
            for (k, _) in source.entries() {
                source.remove(&k);
            }
            Ok(None)
        })
        .method::<P>("keySet", &[], Some(set()), |p, _, _| val(JValue::List(JList::new(ListKind::Set, p.key_set().into_iter().map(JValue::str).collect()))))
        .method::<P>("values", &[], Some(c("java.util.Collection")), |p, _, _| {
            val(JValue::List(JList::new(ListKind::Collection, p.source_map().entries().into_iter().map(|(_, v)| v).collect())))
        })
        .method::<P>("entrySet", &[], Some(set()), |p, _, _| {
            let entries = p.source_map().entries().into_iter().map(|(key, value)| JValue::Object(Arc::new(java_compat::lang::MapEntry { key, value }) as Arc<dyn JObject>)).collect();
            val(JValue::List(JList::new(ListKind::Set, entries)))
        })
        .method::<P>("toString", &[], Some(s()), |p, _, _| val(JValue::str(p.java_string())));
    super::object_methods::<P>(safe::format_config_methods::<P>(def)).register()
}

tool_object!(ValueParser, def);

pub(crate) fn class() -> super::classes::JavaClass {
    use super::classes::Annotation;
    super::classes::JavaClass::tool(def(), || ValueParser::new(None).value())
        .annotated(Annotation::default_key("parser"))
        .annotated(Annotation::invalid_scope(&[super::scope::SESSION]))
        .annotated(Annotation::skip_setters())
}
