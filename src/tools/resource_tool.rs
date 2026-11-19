//! `org.apache.velocity.tools.generic.ResourceTool` (`$text`): keys of resource bundles, composed
//! (`$text.hello.whoever`), localized, and formatted with inserted arguments (`MessageFormat`).

use std::sync::{Arc, Weak};

use java_compat::lang::{JError, JList, JMap, JObject, JValue, ListKind};
use java_compat::reflect::{ClassDef, JType};
use java_compat::text::Locale;
use parking_lot::Mutex;

use super::classes::{Annotation, JavaClass};
use super::classpath;
use super::safe::{self, HasLocale, LocaleConfig, SafeConfig, Tool};
use super::values::ValueParser;
use super::{c, identity, o, s, tool_object, val};

/// The class name of `org.apache.velocity.tools.generic.ResourceTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.ResourceTool";
/// The class name of `ResourceTool.Key`.
pub const KEY: &str = "org.apache.velocity.tools.generic.ResourceTool$Key";
/// `ResourceTool.BUNDLES_KEY`: the configuration key listing the bundles' base names.
pub const BUNDLES_KEY: &str = "bundles";

/// Port of `org.apache.velocity.tools.generic.ResourceTool` (`$text`): messages from properties resource bundles,
/// formatted with `MessageFormat`.
pub struct ResourceTool {
    locale: LocaleConfig,
    bundles: Mutex<Vec<String>>,
    me: Weak<ResourceTool>,
}

impl ResourceTool {
    /// `new ResourceTool()`: the bundle `resources` in the default locale.
    pub fn new() -> Arc<ResourceTool> {
        Arc::new_cyclic(|me| ResourceTool { locale: LocaleConfig::default(), bundles: Mutex::new(vec!["resources".into()]), me: me.clone() })
    }

    fn this(&self) -> Arc<ResourceTool> {
        self.me.upgrade().expect("alive")
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        JValue::Object(self.this() as Arc<dyn JObject>)
    }

    /// `SafeConfig.configure(Map)`: reads `bundles` and the locale; nothing once locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    /// `getLocale()`: the locale bundles are read in unless a key names one.
    pub fn get_locale(&self) -> Locale {
        self.locale.locale()
    }

    /// `setDefaultBundle(bundle)`: replaces the bundle list with that one base name.
    pub fn set_default_bundle(&self, bundle: &str) {
        *self.bundles.lock() = vec![bundle.to_string()];
    }

    /// `getDefaultBundle()`: the first configured bundle.
    pub fn get_default_bundle(&self) -> String {
        self.bundles.lock()[0].clone()
    }

    fn bundles(&self) -> Vec<String> {
        self.bundles.lock().clone()
    }

    fn key(&self, key: Option<String>, bundles: Vec<String>, locale: JValue, args: Option<Vec<JValue>>) -> Key {
        Key { tool: self.this(), key, bundles, locale, args, raw: Arc::new(Mutex::new(None)) }
    }

    /// `get(key)`.
    pub fn get(&self, key: Option<&str>) -> Key {
        self.key(key.map(str::to_string), self.bundles(), self.get_locale().value(), None)
    }

    /// `getKeys()`: every key of the bundles.
    pub fn get_keys(&self) -> Result<Option<Vec<String>>, JError> {
        self.keys_of(None, &self.bundles(), &self.get_locale().value())
    }

    /// `bundle(bundle)`: an empty key that reads only from that bundle.
    pub fn bundle(&self, bundle: &str) -> Key {
        self.key(None, vec![bundle.to_string()], self.get_locale().value(), None)
    }

    /// `locale(locale)`: a `Locale` or anything `toLocale` reads.
    pub fn locale(&self, locale: JValue) -> Key {
        self.key(None, self.bundles(), locale, None)
    }

    /// `insert(args...)`: an empty key carrying `MessageFormat` arguments.
    pub fn insert(&self, args: Vec<JValue>) -> Key {
        self.key(None, self.bundles(), self.get_locale().value(), Some(args))
    }

    fn bundle_for(&self, base: &str, loc: &JValue) -> Result<Option<Arc<classpath::Bundle>>, JError> {
        let locale = if loc.is_null() { Some(self.get_locale()) } else { safe::to_locale(loc) };
        match locale {
            None => Ok(None),
            Some(l) => classpath::get_bundle(base, &l).map(Some),
        }
    }

    /// `get(key, baseName, locale)`: `MissingResourceException` when the bundle does not exist.
    pub fn get_in(&self, key: Option<&str>, base: &str, loc: &JValue) -> Result<JValue, JError> {
        let bundle = self.bundle_for(base, loc)?;
        Ok(match (key, bundle) {
            (Some(k), Some(b)) => b.get_string(k).map(JValue::str).unwrap_or(JValue::Null),
            _ => JValue::Null,
        })
    }

    /// `get(key, bundles, locale)`: the first bundle having the key.
    pub fn get_from(&self, key: Option<&str>, bundles: &[String], loc: &JValue) -> Result<JValue, JError> {
        for b in bundles {
            let r = self.get_in(key, b, loc)?;
            if !r.is_null() {
                return Ok(r);
            }
        }
        Ok(JValue::Null)
    }

    /// `getKeys(prefix, baseName, locale)`: the keys under `prefix.` (without it).
    pub fn keys_in(&self, prefix: Option<&str>, base: &str, loc: &JValue) -> Result<Option<Vec<String>>, JError> {
        let Some(bundle) = self.bundle_for(base, loc)? else { return Ok(None) };
        let mut list = Vec::new();
        for key in bundle.keys() {
            match prefix {
                None => list.push(key),
                Some(p) if key.starts_with(p) => {
                    let rest = &key[p.len()..];
                    if rest.is_empty() {
                        return Err(JError::new("java.lang.StringIndexOutOfBoundsException", "Index 0 out of bounds for length 0"));
                    }
                    list.push(rest.strip_prefix('.').unwrap_or(rest).to_string());
                }
                Some(_) => {}
            }
        }
        Ok(Some(list))
    }

    /// `getKeys(prefix, bundles, locale)`.
    pub fn keys_of(&self, prefix: Option<&str>, bundles: &[String], loc: &JValue) -> Result<Option<Vec<String>>, JError> {
        let locale = if loc.is_null() { Some(self.get_locale()) } else { safe::to_locale(loc) };
        let Some(locale) = locale else { return Ok(None) };
        if bundles.is_empty() {
            return Ok(None);
        }
        let mut master = Vec::new();
        for b in bundles {
            if let Some(sub) = self.keys_in(prefix, b, &locale.value())? {
                master.extend(sub);
            }
        }
        Ok(Some(master))
    }

    /// `render(resource, args)`: `MessageFormat.format` in the default locale.
    pub fn render(&self, resource: &JValue, args: Option<&[JValue]>) -> Result<String, JError> {
        java_compat::text::message_format(&resource.to_java_string(), args.unwrap_or(&[]), &super::default_locale())
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for ResourceTool {
    fn safe(&self) -> &SafeConfig {
        &self.locale.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        if let Some(b) = values.get_strings(BUNDLES_KEY) {
            *self.bundles.lock() = b.into_iter().map(|x| x.unwrap_or_else(|| "null".into())).collect();
        }
        self.locale.configure(values);
    }
}

impl HasLocale for ResourceTool {
    fn locale_config(&self) -> &LocaleConfig {
        &self.locale
    }
}

/// `ResourceTool.Key`: a key (possibly partial), the bundles and locale to read it from, and arguments.
#[derive(Clone)]
pub struct Key {
    tool: Arc<ResourceTool>,
    key: Option<String>,
    bundles: Vec<String>,
    locale: JValue,
    args: Option<Vec<JValue>>,
    raw: Arc<Mutex<Option<JValue>>>,
}

impl Key {
    fn with(&self, key: Option<String>, bundles: Vec<String>, locale: JValue, args: Option<Vec<JValue>>) -> Key {
        Key { tool: self.tool.clone(), key, bundles, locale, args, raw: Arc::new(Mutex::new(None)) }
    }

    /// `get(key)`: this key, a dot, and `key`.
    pub fn get(&self, key: &str) -> Key {
        let k = match &self.key {
            None => key.to_string(),
            Some(p) => format!("{p}.{key}"),
        };
        self.with(Some(k), self.bundles.clone(), self.locale.clone(), self.args.clone())
    }

    /// `bundle(bundle)`: this key, read only from that bundle.
    pub fn bundle(&self, bundle: &str) -> Key {
        self.with(self.key.clone(), vec![bundle.to_string()], self.locale.clone(), self.args.clone())
    }

    /// `locale(locale)`: this key, read in that locale (a `Locale` or anything `toLocale` reads).
    pub fn locale(&self, locale: JValue) -> Key {
        self.with(self.key.clone(), self.bundles.clone(), locale, self.args.clone())
    }

    /// `insert(args)`: after the arguments inserted before.
    pub fn insert(&self, args: Vec<JValue>) -> Key {
        let new = match &self.args {
            None => args,
            Some(a) => a.iter().cloned().chain(args).collect(),
        };
        self.with(self.key.clone(), self.bundles.clone(), self.locale.clone(), Some(new))
    }

    /// `getExists()`: whether a bundle has this key.
    pub fn get_exists(&self) -> Result<bool, JError> {
        Ok(!self.get_raw()?.is_null())
    }

    /// `getRaw()`: the resource, looked up once.
    pub fn get_raw(&self) -> Result<JValue, JError> {
        let mut raw = self.raw.lock();
        if raw.is_none() {
            *raw = Some(self.tool.get_from(self.key.as_deref(), &self.bundles, &self.locale)?);
        }
        Ok(raw.clone().unwrap_or(JValue::Null))
    }

    /// `getKeys()`: the bundles' keys under this one (all of them for an empty key); `None` when the locale is invalid.
    pub fn get_keys(&self) -> Result<Option<Vec<String>>, JError> {
        self.tool.keys_of(self.key.as_deref(), &self.bundles, &self.locale)
    }

    /// `toString()`: the formatted resource, `???key???` when missing, `""` without a key.
    pub fn render(&self) -> Result<String, JError> {
        let Some(key) = &self.key else { return Ok(String::new()) };
        if !self.get_exists()? {
            super::log::warn(CLASS, &format!("missing key: {key}"));
            return Ok(format!("???{key}???"));
        }
        self.tool.render(&self.get_raw()?, self.args.as_deref())
    }

    /// This key as a template value.
    pub fn value(self) -> JValue {
        JValue::Object(Arc::new(self) as Arc<dyn JObject>)
    }

    fn java_string(&self) -> String {
        self.render().unwrap_or_else(|e| e.to_string())
    }
}

fn keys_value(k: Option<Vec<String>>) -> JValue {
    k.map(|k| JValue::List(JList::new(ListKind::ArrayList, k.into_iter().map(JValue::str).collect()))).unwrap_or(JValue::Null)
}

fn opt_key(v: &JValue) -> Option<String> {
    (!v.is_null()).then(|| v.to_java_string())
}

fn args_of(v: &JValue) -> Vec<JValue> {
    match v {
        JValue::Array(a) => a.items(),
        JValue::List(l) => l.items(),
        _ => Vec::new(),
    }
}

fn key_def() -> Arc<ClassDef> {
    type K = Key;
    let me = || c(KEY);
    let def = ClassDef::new(KEY)
        .method::<K>("get", &[s()], Some(me()), |k, _, a| val(k.get(&a[0].to_java_string()).value()))
        .method::<K>("get", &[o()], Some(me()), |k, _, a| val(k.get(&a[0].to_java_string()).value()))
        .method::<K>("bundle", &[s()], Some(me()), |k, _, a| val(k.bundle(&a[0].to_java_string()).value()))
        .method::<K>("locale", &[o()], Some(me()), |k, _, a| val(k.locale(a[0].clone()).value()))
        .method::<K>("insert", &[JType::array(o())], Some(me()), |k, _, a| val(k.insert(args_of(&a[0])).value()))
        .method::<K>("insert", &[c("java.util.List")], Some(me()), |k, _, a| val(k.insert(args_of(&a[0])).value()))
        .method::<K>("insert", &[o()], Some(me()), |k, _, a| val(k.insert(vec![a[0].clone()]).value()))
        .method::<K>("insert", &[o(), o()], Some(me()), |k, _, a| val(k.insert(vec![a[0].clone(), a[1].clone()]).value()))
        .method::<K>("getExists", &[], Some(JType::Boolean), |k, _, _| val(JValue::Bool(k.get_exists()?)))
        .method::<K>("getRaw", &[], Some(o()), |k, _, _| val(k.get_raw()?))
        .method::<K>("getKeys", &[], Some(c("java.util.List")), |k, _, _| val(keys_value(k.get_keys()?)))
        .method::<K>("toString", &[], Some(s()), |k, _, _| val(JValue::str(k.render()?)));
    super::object_methods::<K>(def).register()
}

tool_object!(Key, key_def);

fn def() -> Arc<ClassDef> {
    type R = ResourceTool;
    let key = || c(KEY);
    let def = ClassDef::new(CLASS)
        .extends(&[safe::LOCALE_CONFIG, safe::SAFE_CONFIG])
        .implements(&["java.io.Serializable"])
        .method::<R>("get", &[o()], Some(key()), |t, _, a| val(t.get(opt_key(&a[0]).as_deref()).value()))
        .method::<R>("get", &[s()], Some(key()), |t, _, a| val(t.get(a[0].as_str()).value()))
        .method::<R>("getKeys", &[], Some(c("java.util.List")), |t, _, _| val(keys_value(t.get_keys()?)))
        .method::<R>("bundle", &[s()], Some(key()), |t, _, a| val(t.bundle(&a[0].to_java_string()).value()))
        .method::<R>("locale", &[o()], Some(key()), |t, _, a| val(t.locale(a[0].clone()).value()))
        .method::<R>("insert", &[JType::array(o())], Some(key()), |t, _, a| val(t.insert(args_of(&a[0])).value()))
        .method::<R>("insert", &[c("java.util.List")], Some(key()), |t, _, a| val(t.insert(args_of(&a[0])).value()))
        .method::<R>("insert", &[o()], Some(key()), |t, _, a| val(t.insert(vec![a[0].clone()]).value()))
        .method::<R>("insert", &[o(), o()], Some(key()), |t, _, a| val(t.insert(vec![a[0].clone(), a[1].clone()]).value()))
        .method::<R>("get", &[o(), s(), o()], Some(o()), |t, _, a| match a[1].as_str() {
            Some(b) => val(t.get_in(opt_key(&a[0]).as_deref(), b, &a[2])?),
            None => val(JValue::Null),
        })
        .method::<R>("get", &[o(), JType::array(s()), o()], Some(o()), |t, _, a| {
            let bundles: Vec<String> = args_of(&a[1]).iter().map(|b| b.to_java_string()).collect();
            val(t.get_from(opt_key(&a[0]).as_deref(), &bundles, &a[2])?)
        })
        .method::<R>("getKeys", &[s(), s(), o()], Some(c("java.util.List")), |t, _, a| match a[1].as_str() {
            Some(b) => val(keys_value(t.keys_in(a[0].as_str(), b, &a[2])?)),
            None => val(JValue::Null),
        })
        .method::<R>("getKeys", &[s(), JType::array(s()), o()], Some(c("java.util.List")), |t, _, a| {
            let bundles: Vec<String> = args_of(&a[1]).iter().map(|b| b.to_java_string()).collect();
            val(keys_value(t.keys_of(a[0].as_str(), &bundles, &a[2])?))
        })
        .method::<R>("render", &[o(), JType::array(o())], Some(s()), |t, _, a| val(JValue::str(t.render(&a[0], (!a[1].is_null()).then(|| args_of(&a[1])).as_deref())?)));
    super::object_methods::<R>(safe::locale_config_methods::<R>(def)).register()
}

tool_object!(ResourceTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || ResourceTool::new().value()).annotated(Annotation::default_key("text"))
}
