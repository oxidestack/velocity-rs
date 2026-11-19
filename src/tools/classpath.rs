//! The Java class path as the tools search it (`ClassLoader.getResource`, `Class.getResource`), with
//! `java.util.Properties` files and `ResourceBundle.getBundle`'s lookup over them.
//!
//! The class path holds the resources velocity-tools' jar carries (its default `tools.xml` and the
//! `times` bundle) and any directories an application adds with [`add_directory`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use java_compat::lang::JError;
use java_compat::text::Locale;
use parking_lot::RwLock;

const JAR: &str = "jar:file:/velocity-tools-generic-3.1.jar!/";

static BUILTIN: &[(&str, &str)] = &[
    ("org/apache/velocity/tools/generic/tools.xml", include_str!("resources/tools.xml")),
    ("org/apache/velocity/tools/generic/times.properties", include_str!("resources/times.properties")),
];

static DIRECTORIES: LazyLock<RwLock<Vec<PathBuf>>> = LazyLock::new(|| RwLock::new(Vec::new()));

/// Adds a directory to the class path (after the ones already there).
pub fn add_directory(dir: impl AsRef<Path>) {
    let d = dir.as_ref().to_path_buf();
    let mut dirs = DIRECTORIES.write();
    if !dirs.contains(&d) {
        dirs.push(d);
    }
}

/// A resource found on the class path: its URL and content.
#[derive(Clone, Debug)]
pub struct Resource {
    /// The resource's URL (`file:` or `jar:file:...!/...`).
    pub url: String,
    /// The resource's raw content.
    pub bytes: Arc<[u8]>,
}

impl Resource {
    /// The content decoded as UTF-8 (an `InputStreamReader`'s replacement of malformed input).
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

/// `ClassLoader.getResources(name)`: every class path entry holding `name` (no leading slash allowed).
pub fn loader_resources(name: &str) -> Vec<Resource> {
    if name.starts_with('/') || name.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for dir in DIRECTORIES.read().iter() {
        let p = dir.join(name);
        if p.is_file() {
            if let Ok(bytes) = std::fs::read(&p) {
                let abs = std::fs::canonicalize(&p).unwrap_or(p);
                out.push(Resource { url: format!("file:{}", abs.display()), bytes: bytes.into() });
            }
        }
    }
    if let Some((n, content)) = BUILTIN.iter().find(|(n, _)| *n == name) {
        out.push(Resource { url: format!("{JAR}{n}"), bytes: content.as_bytes().into() });
    }
    out
}

/// `ClassLoader.getResource(name)`.
pub fn loader_resource(name: &str) -> Option<Resource> {
    loader_resources(name).into_iter().next()
}

/// `Class.getResource(name)` for a class: absolute with a leading slash, else relative to its package.
pub fn class_resource(name: &str, class: &str) -> Option<Resource> {
    match name.strip_prefix('/') {
        Some(abs) => loader_resource(abs),
        None => {
            let package = class.rfind('.').map(|i| class[..i].replace('.', "/")).unwrap_or_default();
            if package.is_empty() { loader_resource(name) } else { loader_resource(&format!("{package}/{name}")) }
        }
    }
}

const CLASS_UTILS: &str = "org.apache.velocity.tools.ClassUtils";

/// `ClassUtils.getResource(name, caller)`: the class loaders, then `ClassUtils`' and the caller's class.
pub fn get_resource(name: &str, caller: &str) -> Option<Resource> {
    loader_resource(name).or_else(|| class_resource(name, CLASS_UTILS)).or_else(|| class_resource(name, caller))
}

/// `ClassUtils.getResources(name, caller)`: distinct URLs from the loaders and the classes, retried with a
/// leading slash when nothing is found.
pub fn get_resources(name: &str, caller: &str) -> Vec<Resource> {
    let mut found: Vec<Resource> = Vec::new();
    let add = |r: Resource, found: &mut Vec<Resource>| {
        if !found.iter().any(|f| f.url == r.url) {
            found.push(r);
        }
    };
    for r in loader_resources(name) {
        add(r, &mut found);
    }
    // The class loader of ClassUtils (the same class path); then `ClassUtils.class.getResource` if it found none.
    let from_loader = loader_resources(name);
    if from_loader.is_empty() {
        if let Some(r) = class_resource(name, CLASS_UTILS) {
            add(r, &mut found);
        }
    }
    if from_loader.is_empty() {
        if let Some(r) = class_resource(name, caller) {
            add(r, &mut found);
        }
    }
    if found.is_empty() && !name.starts_with('/') {
        return get_resources(&format!("/{name}"), caller);
    }
    found
}

/// `java.util.Properties.load(Reader)`: key/value pairs in file order (later duplicates replace earlier ones).
pub fn load_properties(text: &str) -> Result<Vec<(String, String)>, JError> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut lines = text.split_inclusive(['\n', '\r']).peekable();
    let mut pending = String::new();
    let mut continuing = false;
    let is_ws = |c: char| matches!(c, ' ' | '\t' | '\u{c}');
    let mut logical: Vec<String> = Vec::new();
    while let Some(raw) = lines.next() {
        // `\r\n` is one line terminator.
        let mut line = raw.trim_end_matches(['\n', '\r']).to_string();
        if raw.ends_with('\r') && lines.peek() == Some(&"\n") {
            lines.next();
        }
        if continuing {
            line = line.trim_start_matches(is_ws).to_string();
        } else {
            let t = line.trim_start_matches(is_ws);
            if t.is_empty() || t.starts_with('#') || t.starts_with('!') {
                continue;
            }
            line = t.to_string();
        }
        let trailing = line.chars().rev().take_while(|c| *c == '\\').count();
        if trailing % 2 == 1 {
            line.pop();
            pending.push_str(&line);
            continuing = true;
            continue;
        }
        pending.push_str(&line);
        logical.push(std::mem::take(&mut pending));
        continuing = false;
    }
    if !pending.is_empty() {
        logical.push(pending);
    }
    for l in logical {
        let chars: Vec<char> = l.chars().collect();
        let limit = chars.len();
        let (mut key_len, mut value_start, mut has_sep, mut preceding_backslash) = (0, limit, false, false);
        while key_len < limit {
            let c = chars[key_len];
            if (c == '=' || c == ':') && !preceding_backslash {
                value_start = key_len + 1;
                has_sep = true;
                break;
            } else if is_ws(c) && !preceding_backslash {
                value_start = key_len + 1;
                break;
            }
            preceding_backslash = c == '\\' && !preceding_backslash;
            key_len += 1;
        }
        while value_start < limit {
            let c = chars[value_start];
            if !is_ws(c) {
                if !has_sep && (c == '=' || c == ':') {
                    has_sep = true;
                } else {
                    break;
                }
            }
            value_start += 1;
        }
        let key: String = chars[..key_len].iter().collect();
        let value: String = chars[value_start.min(limit)..].iter().collect();
        let (k, v) = (unescape(&key)?, unescape(&value)?);
        match out.iter_mut().find(|(ok, _)| *ok == k) {
            Some(slot) => slot.1 = v,
            None => out.push((k, v)),
        }
    }
    Ok(out)
}

fn unescape(s: &str) -> Result<String, JError> {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('f') => out.push('\u{c}'),
            Some('u') => {
                let hex: String = it.by_ref().take(4).collect();
                let v = (hex.len() == 4).then(|| u32::from_str_radix(&hex, 16).ok()).flatten();
                match v {
                    Some(v) => out.push(char::from_u32(v).unwrap_or('\u{fffd}')),
                    None => return Err(JError::new("java.lang.IllegalArgumentException", "Malformed \\uxxxx encoding.")),
                }
            }
            Some(o) => out.push(o),
            None => {}
        }
    }
    Ok(out)
}

/// A `PropertyResourceBundle` and its parents.
#[derive(Debug, Clone)]
pub struct Bundle {
    /// The locale this bundle's properties file is for (`Locale.ROOT` for the base bundle).
    pub locale: Locale,
    entries: Vec<(String, String)>,
    /// The bundle consulted for keys this one lacks, `None` at the base bundle.
    pub parent: Option<Arc<Bundle>>,
}

impl Bundle {
    /// `getString(key)` (`handleGetObject` up the parent chain).
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).or_else(|| self.parent.as_ref().and_then(|p| p.get_string(key)))
    }

    /// `getKeys()`: this bundle's keys and its parents'.
    pub fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.entries.iter().map(|(k, _)| k.clone()).collect();
        if let Some(p) = &self.parent {
            for k in p.keys() {
                if !keys.contains(&k) {
                    keys.push(k);
                }
            }
        }
        keys
    }
}

fn bundle_suffix(l: &Locale) -> String {
    let s = l.to_string();
    if s.is_empty() { String::new() } else { format!("_{s}") }
}

fn candidates(l: &Locale) -> Vec<Locale> {
    let mut out = Vec::new();
    if !l.variant().is_empty() {
        out.push(Locale::new(l.language(), l.country(), l.variant()));
    }
    if !l.country().is_empty() {
        out.push(Locale::new(l.language(), l.country(), ""));
    }
    if !l.language().is_empty() {
        out.push(Locale::new(l.language(), "", ""));
    }
    out.push(Locale::root());
    out
}

fn load_bundle(base: &str, l: &Locale) -> Option<(Locale, Vec<(String, String)>)> {
    let name = format!("{}{}.properties", base.replace('.', "/"), bundle_suffix(l));
    let r = loader_resource(&name)?;
    load_properties(&r.text()).ok().map(|e| (l.clone(), e))
}

/// `ResourceBundle.getBundle(baseName, locale)` with the default control: the locale's candidates, then the
/// default locale's; `MissingResourceException` when nothing exists.
pub fn get_bundle(base: &str, locale: &Locale) -> Result<Arc<Bundle>, JError> {
    /// The bundles loaded, by base name and locale.
    type Bundles = HashMap<(String, String), Arc<Bundle>>;
    static CACHE: LazyLock<RwLock<Bundles>> = LazyLock::new(|| RwLock::new(HashMap::new()));
    let key = (base.to_string(), locale.to_string());
    if let Some(b) = CACHE.read().get(&key) {
        return Ok(b.clone());
    }
    let find = |cands: &[Locale]| -> Option<Arc<Bundle>> {
        let mut chain: Option<Arc<Bundle>> = None;
        for c in cands.iter().rev() {
            if let Some((l, entries)) = load_bundle(base, c) {
                chain = Some(Arc::new(Bundle { locale: l, entries, parent: chain }));
            }
        }
        chain
    };
    let mut target = Some(locale.clone());
    let mut bundle: Option<Arc<Bundle>> = None;
    let mut base_bundle: Option<Arc<Bundle>> = None;
    let default = crate::tools::default_locale();
    while let Some(t) = target {
        let cands = candidates(&t);
        bundle = find(&cands);
        if let Some(b) = &bundle {
            let is_base = b.locale == Locale::root();
            if !is_base || b.locale == *locale || (cands.len() == 1 && b.locale == cands[0]) {
                break;
            }
            if is_base && base_bundle.is_none() {
                base_bundle = Some(b.clone());
            }
        }
        target = if t == default { None } else { Some(default.clone()) };
    }
    let bundle = match bundle.or(base_bundle) {
        Some(b) => b,
        None => {
            return Err(JError::new(
                "java.util.MissingResourceException",
                format!("Can't find bundle for base name {base}, locale {locale}"),
            ));
        }
    };
    CACHE.write().insert(key, bundle.clone());
    Ok(bundle)
}
