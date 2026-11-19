//! `org.apache.velocity.tools.generic.LinkTool` (`$link`): URIs built piece by piece, each change returning a
//! copy (`$link.host('x').path('y').param('a', 1)`), printed with an XHTML-safe query.

use std::sync::{Arc, Weak};

use java_compat::lang::{JArray, JError, JList, JMap, JObject, JValue, MapKind};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;

use super::classes::{Annotation, JavaClass};
use super::safe::{self, SafeConfig, Tool};
use super::uri::{self, Uri};
use super::values::ValueParser;
use super::{c, o, s, tool_object, val};

/// The Java class name of the tool.
pub const CLASS: &str = "org.apache.velocity.tools.generic.LinkTool";
/// The query delimiter in HTML mode.
pub const HTML_QUERY_DELIMITER: &str = "&";
/// The query delimiter in XHTML mode, the default.
pub const XHTML_QUERY_DELIMITER: &str = "&amp;";
/// The configuration key of whether `param` appends to an existing parameter rather than replacing it.
pub const APPEND_PARAMS_KEY: &str = "appendParameters";
/// The configuration key of whether links print without scheme, user, host and port.
pub const FORCE_RELATIVE_KEY: &str = "forceRelative";
/// The default character encoding of parameters.
pub const DEFAULT_CHARSET: &str = "UTF-8";
/// The scheme a link gets when it is made absolute.
pub const DEFAULT_SCHEME: &str = "http";
/// The scheme of a secure link.
pub const SECURE_SCHEME: &str = "https";
/// The configuration key of a whole URI to start from.
pub const URI_KEY: &str = "uri";
/// The configuration key of the scheme.
pub const SCHEME_KEY: &str = "scheme";
/// The configuration key of the user info.
pub const USER_KEY: &str = "user";
/// The configuration key of the host.
pub const HOST_KEY: &str = "host";
/// The configuration key of the port.
pub const PORT_KEY: &str = "port";
/// The configuration key of the path.
pub const PATH_KEY: &str = super::toolbox::PATH_KEY;
/// The configuration key of the query parameters.
pub const QUERY_KEY: &str = "params";
/// The configuration key of the fragment.
pub const FRAGMENT_KEY: &str = "anchor";
/// The configuration key of the character encoding.
pub const CHARSET_KEY: &str = "charset";
/// The configuration key of whether the query uses the XHTML delimiter `&amp;`.
pub const XHTML_MODE_KEY: &str = "xhtml";
pub use safe::{LOCK_CONFIG_KEY, SAFE_MODE_KEY};

#[derive(Clone)]
struct State {
    scheme: Option<String>,
    user: Option<String>,
    host: Option<String>,
    port: i32,
    path: Option<String>,
    /// A `LinkedHashMap` shared by shallow copies, as in Java.
    query: Option<JMap>,
    fragment: Option<String>,
    charset: String,
    query_delim: String,
    append_params: bool,
    force_relative: bool,
    opaque: bool,
}

/// Port of `org.apache.velocity.tools.generic.LinkTool`: a URI under construction; every builder method returns a changed copy and leaves this one alone.
pub struct LinkTool {
    safe: SafeConfig,
    st: Mutex<State>,
    /// The tool every copy was made from (`self`); `None` for that tool itself.
    origin: Option<Arc<LinkTool>>,
    me: Weak<LinkTool>,
}

const LOG: &str = CLASS;

fn string_of(v: &JValue) -> Option<String> {
    (!v.is_null()).then(|| v.to_java_string())
}

impl LinkTool {
    /// A link with nothing set: no scheme, host, path or query, `UTF-8`, XHTML mode, appending parameters.
    pub fn new() -> Arc<LinkTool> {
        Arc::new_cyclic(|me| LinkTool {
            safe: SafeConfig::default(),
            st: Mutex::new(State {
                scheme: None,
                user: None,
                host: None,
                port: -1,
                path: None,
                query: None,
                fragment: None,
                charset: DEFAULT_CHARSET.into(),
                query_delim: XHTML_QUERY_DELIMITER.into(),
                append_params: true,
                force_relative: false,
                opaque: false,
            }),
            origin: None,
            me: me.clone(),
        })
    }

    fn this(&self) -> Arc<LinkTool> {
        self.me.upgrade().expect("alive")
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        JValue::Object(self.this() as Arc<dyn JObject>)
    }

    /// `configure(params)`: the safe-mode settings and the link parts the configuration gives.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    /// `isSafeMode()`: whether `getUri()` is withheld from templates.
    pub fn is_safe_mode(&self) -> bool {
        self.safe.is_safe_mode()
    }

    /// `duplicate(deep)`: a copy sharing the query map, or with its own copy of it.
    pub fn duplicate(&self, deep: bool) -> Arc<LinkTool> {
        let mut st = self.st.lock().clone();
        if deep {
            if let Some(q) = &st.query {
                st.query = Some(JMap::from_entries(MapKind::Linked, q.entries()));
            }
        }
        let origin = Some(self.origin.clone().unwrap_or_else(|| self.this()));
        let copy = Arc::new_cyclic(|me| LinkTool { safe: SafeConfig::default(), st: Mutex::new(st), origin, me: me.clone() });
        copy.safe.set_safe_mode(self.safe.is_safe_mode());
        copy.safe.set_lock_config(self.safe.is_config_locked());
        copy
    }

    /// The query map itself (for identity checks).
    pub fn query_map(&self) -> Option<JMap> {
        self.st.lock().query.clone()
    }

    /// `setCharacterEncoding(charset)`.
    pub fn set_character_encoding(&self, charset: &str) {
        self.st.lock().charset = charset.to_string();
    }

    /// `setXHTML(xhtml)`: the query delimiter, `&amp;` when true, else `&`.
    pub fn set_xhtml(&self, xhtml: bool) {
        self.st.lock().query_delim = if xhtml { XHTML_QUERY_DELIMITER } else { HTML_QUERY_DELIMITER }.into();
    }

    /// `setAppendParams(append)`: whether `param` adds to a parameter's values rather than replacing them.
    pub fn set_append_params(&self, a: bool) {
        self.st.lock().append_params = a;
    }

    /// `setForceRelative(force)`: whether the link prints without scheme, user, host and port.
    pub fn set_force_relative(&self, f: bool) {
        self.st.lock().force_relative = f;
    }

    /// `setScheme(obj)`: without a trailing colon; `null` for empty.
    pub fn set_scheme(&self, obj: &JValue) {
        let mut st = self.st.lock();
        st.scheme = string_of(obj).filter(|s| !s.is_empty()).map(|s| s.strip_suffix(':').map(str::to_string).unwrap_or(s));
    }

    /// `setUserInfo(obj)`: its string form, `None` for `null`.
    pub fn set_user_info(&self, obj: &JValue) {
        self.st.lock().user = string_of(obj);
    }

    /// `setHost(obj)`: its string form, `None` for `null`.
    pub fn set_host(&self, obj: &JValue) {
        self.st.lock().host = string_of(obj);
    }

    /// `setPort(obj)`: `-2` (an error) for something that is not a number.
    pub fn set_port(&self, obj: &JValue) {
        let port = match obj {
            JValue::Null => -1,
            n if super::num::is_number(n) => super::num::int_value(n),
            other => match crate::uberspect::parse_integral(&other.to_java_string(), 32) {
                Ok(p) => p as i32,
                Err(e) => {
                    super::log::error(LOG, &format!("Could not convert '{}' to int: {e}", other.to_java_string()));
                    -2
                }
            },
        };
        self.st.lock().port = port;
    }

    /// `setPath(obj)`: with a leading slash unless opaque.
    pub fn set_path(&self, obj: &JValue) {
        let mut st = self.st.lock();
        st.path = string_of(obj).map(|p| if !st.opaque && !p.starts_with('/') { format!("/{p}") } else { p });
    }

    /// `appendPath(obj)`: added to the path with one slash between; nothing for `null` or an opaque link.
    pub fn append_path(&self, obj: &JValue) {
        if obj.is_null() || self.is_opaque() {
            return;
        }
        let combined = combine_path(self.get_path().as_deref(), Some(&obj.to_java_string()));
        self.set_path(&combined.map(JValue::str).unwrap_or(JValue::Null));
    }

    /// `combinePath(start, end)`: joined with exactly one slash.
    pub fn combine_path(&self, start: Option<&str>, end: Option<&str>) -> Option<String> {
        combine_path(start, end)
    }

    /// `setQuery(obj)`: a map (array values spread), else a query string.
    pub fn set_query(&self, obj: &JValue) {
        let query = match obj {
            JValue::Null => None,
            JValue::Map(m) => {
                let q = JMap::new(MapKind::Linked);
                for (k, v) in m.entries() {
                    let key = JValue::str(k.to_java_string());
                    match &v {
                        JValue::Array(a) => match a.len() {
                            0 => {}
                            1 => {
                                q.put(key, a.get(0).unwrap_or(JValue::Null));
                            }
                            _ => {
                                q.put(key, JValue::List(JList::array_list(a.items())));
                            }
                        },
                        JValue::Null => {
                            // `values.getClass()` of null throws in Java.
                            q.put(key, JValue::Null);
                        }
                        _ => {
                            q.put(key, v.clone());
                        }
                    }
                }
                Some(q)
            }
            other => {
                let qs = self.normalize_query(&other.to_java_string());
                self.parse_query_with(&qs, &self.st.lock().query_delim.clone())
            }
        };
        self.st.lock().query = query;
    }

    /// `normalizeQuery(qs)`: every `&` or `&amp;` as the current delimiter.
    pub fn normalize_query(&self, qs: &str) -> String {
        if !qs.contains('&') {
            return qs.to_string();
        }
        let delim = self.st.lock().query_delim.clone();
        qs.replace("&amp;", "&").replace('&', &delim)
    }

    /// `toQuery(parameters)`.
    pub fn to_query_map(&self, parameters: Option<&JMap>) -> Option<String> {
        let parameters = parameters?;
        let delim = self.st.lock().query_delim.clone();
        let mut out = String::new();
        for (k, v) in parameters.entries() {
            if !out.is_empty() {
                out.push_str(&delim);
            }
            out.push_str(&self.to_query(&k, &v));
        }
        Some(out)
    }

    /// `appendQuery(obj)`: a query string added to the current query.
    pub fn append_query(&self, obj: &JValue) {
        if !obj.is_null() {
            let combined = self.combine_query(self.get_query().as_deref(), Some(&obj.to_java_string()));
            self.set_query(&combined.map(JValue::str).unwrap_or(JValue::Null));
        }
    }

    /// `setParam(key, value, append)`.
    pub fn set_param(&self, key: &JValue, value: &JValue, append: bool) {
        let key = JValue::str(key.to_java_string());
        let existing = self.st.lock().query.clone();
        match existing {
            None => {
                let q = JMap::new(MapKind::Linked);
                self.st.lock().query = Some(q.clone());
                put_param(&q, key, value.clone());
            }
            Some(q) if append => append_param(&q, key, value.clone()),
            Some(q) => put_param(&q, key, value.clone()),
        }
    }

    /// `setParams(obj, append)`.
    pub fn set_params(&self, obj: &JValue, append: bool) {
        if !append {
            self.set_query(obj);
            return;
        }
        if obj.is_null() {
            return;
        }
        let map = match obj {
            JValue::Map(m) => Some(m.clone()),
            other => self.parse_query(&other.to_java_string()),
        };
        if let Some(m) = map {
            let q = {
                let mut st = self.st.lock();
                st.query.get_or_insert_with(|| JMap::new(MapKind::Linked)).clone()
            };
            for (k, v) in m.entries() {
                append_param(&q, JValue::str(k.to_java_string()), v);
            }
        }
    }

    /// `removeParam(key)`: the removed value, `Null` when there was none.
    pub fn remove_param(&self, key: &JValue) -> JValue {
        match self.st.lock().query.clone() {
            Some(q) => q.remove(&JValue::str(key.to_java_string())),
            None => JValue::Null,
        }
    }

    /// `handleParamsBoolean(keep)`.
    pub fn handle_params_boolean(&self, keep: bool) {
        if !keep {
            self.set_query(&JValue::Null);
        }
    }

    /// `combineQuery(current, add)`.
    pub fn combine_query(&self, current: Option<&str>, add: Option<&str>) -> Option<String> {
        let delim = self.st.lock().query_delim.clone();
        let add = match add {
            None | Some("") => return current.map(str::to_string),
            Some(a) => a.strip_prefix('?').unwrap_or(a),
        };
        let current = match current {
            None | Some("") => return Some(add.to_string()),
            Some(c) => c,
        };
        let current = if let Some(c) = current.strip_suffix(delim.as_str()) { c } else { current.strip_suffix('&').unwrap_or(current) };
        if add.starts_with(delim.as_str()) {
            return Some(format!("{current}{add}"));
        }
        let add = add.strip_prefix('&').unwrap_or(add);
        Some(format!("{current}{delim}{add}"))
    }

    /// `toQuery(key, value)`: `key=value`, each element of a list or array a pair of its own.
    pub fn to_query(&self, key: &JValue, value: &JValue) -> String {
        let delim = self.st.lock().query_delim.clone();
        let enc = |v: &JValue| self.encode(v).unwrap_or_else(|| "null".into());
        let items = match value {
            JValue::Null => return format!("{}=", enc(key)),
            JValue::List(l) => l.items(),
            JValue::Array(a) if !a.component.is_primitive() => a.items(),
            other => return format!("{}={}", enc(key), enc(other)),
        };
        let k = enc(key);
        items.iter().map(|v| format!("{k}={}", if v.is_null() { String::new() } else { enc(v) })).collect::<Vec<_>>().join(&delim)
    }

    /// `parseQuery(query)`: normalized first.
    pub fn parse_query(&self, query: &str) -> Option<JMap> {
        let delim = self.st.lock().query_delim.clone();
        self.parse_query_with(&self.normalize_query(query), &delim)
    }

    /// `parseQuery(query, delim)`: repeated keys collect their values in a list.
    pub fn parse_query_with(&self, query: &str, delim: &str) -> Option<JMap> {
        let query = query.strip_prefix('?').unwrap_or(query);
        let pairs = java_compat::methods::split(query, delim, 0).unwrap_or_default();
        if pairs.is_empty() {
            return None;
        }
        let params = JMap::new(MapKind::Linked);
        for pair in pairs {
            let kv = java_compat::methods::split(&pair, "=", 0).unwrap_or_default();
            let key = JValue::str(kv.first().cloned().unwrap_or_default());
            let mut value = kv.get(1).map(|v| JValue::str(v.as_str())).unwrap_or(JValue::Null);
            if params.contains_key(&key) {
                let old = params.get(&key).unwrap_or(JValue::Null);
                value = match old {
                    JValue::List(l) => {
                        l.push(value);
                        JValue::List(l)
                    }
                    other => JValue::List(JList::array_list(vec![other, value])),
                };
            }
            params.put(key, value);
        }
        Some(params)
    }

    /// `setFragment(obj)`: `null` for empty.
    pub fn set_fragment(&self, obj: &JValue) {
        self.st.lock().fragment = string_of(obj).filter(|f| !f.is_empty());
    }

    /// `setFromURI(obj)`: every component from a URI (or its string); `false` when it does not parse.
    pub fn set_from_uri(&self, obj: &JValue) -> bool {
        if obj.is_null() {
            for f in [Self::set_scheme, Self::set_user_info, Self::set_host, Self::set_port, Self::set_path, Self::set_query, Self::set_fragment] {
                f(self, &JValue::Null);
            }
            return true;
        }
        let Some(uri) = self.to_uri(obj) else { return false };
        let opt = |v: Option<String>| v.map(JValue::str).unwrap_or(JValue::Null);
        if let Some(sc) = uri.get_scheme() {
            self.set_scheme(&JValue::str(sc));
        }
        if uri.is_opaque() {
            self.st.lock().opaque = true;
            if let Some(ssp) = uri.get_scheme_specific_part() {
                self.set_path(&JValue::str(ssp));
            }
        } else {
            if let Some(u) = uri.get_user_info() {
                self.set_user_info(&JValue::str(u));
            }
            if let Some(h) = uri.get_host() {
                self.set_host(&JValue::str(h));
            }
            if uri.get_port() > -1 {
                self.set_port(&JValue::Int(uri.get_port()));
            }
            if let Some(p) = uri.get_path() {
                let p = if p == "/" || p.is_empty() { None } else { Some(p) };
                self.set_path(&opt(p));
            }
        }
        if let Some(q) = uri.get_query() {
            self.set_query(&JValue::str(q));
        }
        if let Some(f) = uri.get_fragment() {
            self.set_fragment(&JValue::str(f));
        }
        true
    }

    /// `toURI(obj)`: a URI as is, else parsed from the string (logged and `null` when it does not parse).
    pub fn to_uri(&self, obj: &JValue) -> Option<Arc<Uri>> {
        if let Some(u) = uri::as_uri(obj) {
            return Some(u);
        }
        match Uri::parse(&obj.to_java_string()) {
            Ok(u) => Some(Arc::new(u)),
            Err(e) => {
                super::log::error(LOG, &format!("Could not convert '{}' to URI: {e}", obj.to_java_string()));
                None
            }
        }
    }

    /// `createURI()`: `null` for an invalid port, nothing to show, or a URI that does not build.
    pub fn create_uri(&self) -> Option<Uri> {
        let st = self.st.lock().clone();
        if st.port <= -2 {
            return None;
        }
        let query = self.to_query_map(st.query.as_ref());
        let r = if st.opaque {
            if st.query.as_ref().is_some_and(|q| !q.is_empty()) {
                super::log::warn(LOG, &format!("Ignoring given parameters in opaque URI: {}", query.clone().unwrap_or_default()));
            }
            Uri::opaque(st.scheme.as_deref(), st.path.as_deref(), st.fragment.as_deref())
        } else if st.force_relative {
            if st.path.is_none() && st.query.is_none() && st.fragment.is_none() {
                return None;
            }
            Uri::from_components(None, None, None, -1, st.path.as_deref(), query.as_deref(), st.fragment.as_deref())
        } else {
            if st.scheme.is_none() && st.user.is_none() && st.host.is_none() && st.path.is_none() && st.query.is_none() && st.fragment.is_none() {
                return None;
            }
            Uri::from_components(st.scheme.as_deref(), st.user.as_deref(), st.host.as_deref(), st.port, st.path.as_deref(), query.as_deref(), st.fragment.as_deref())
        };
        match r {
            Ok(u) => Some(u),
            Err(e) => {
                super::log::error(LOG, &format!("Could not create URI: {e}"));
                None
            }
        }
    }

    /// `getCharacterEncoding()`.
    pub fn get_character_encoding(&self) -> String {
        self.st.lock().charset.clone()
    }

    /// `isXHTML()`: whether the query delimiter is `&amp;`.
    pub fn is_xhtml(&self) -> bool {
        self.st.lock().query_delim == XHTML_QUERY_DELIMITER
    }

    /// `getAppendParams()`.
    pub fn get_append_params(&self) -> bool {
        self.st.lock().append_params
    }

    fn copy_with(&self, f: impl FnOnce(&LinkTool)) -> Arc<LinkTool> {
        let copy = self.duplicate(false);
        f(&copy);
        copy
    }

    /// `scheme(scheme)`: a copy with this scheme.
    pub fn scheme(&self, scheme: &JValue) -> Arc<LinkTool> {
        self.copy_with(|c| c.set_scheme(scheme))
    }

    /// `secure()`: a copy with the `https` scheme.
    pub fn secure(&self) -> Arc<LinkTool> {
        self.scheme(&JValue::str(SECURE_SCHEME))
    }

    /// `insecure()`: a copy with the `http` scheme.
    pub fn insecure(&self) -> Arc<LinkTool> {
        self.scheme(&JValue::str(DEFAULT_SCHEME))
    }

    /// `getScheme()`: without the colon, `None` when unset.
    pub fn get_scheme(&self) -> Option<String> {
        self.st.lock().scheme.clone()
    }

    /// `isSecure()`: whether the scheme is `https`, ignoring case.
    pub fn is_secure(&self) -> bool {
        self.get_scheme().is_some_and(|s| s.eq_ignore_ascii_case(SECURE_SCHEME))
    }

    /// `isOpaque()`: whether the link is an opaque URI such as `mailto:x@y`.
    pub fn is_opaque(&self) -> bool {
        self.st.lock().opaque
    }

    /// `user(info)`: a copy with this user info.
    pub fn user(&self, info: &JValue) -> Arc<LinkTool> {
        self.copy_with(|c| c.set_user_info(info))
    }

    /// `getUser()`: the user info, `None` when unset.
    pub fn get_user(&self) -> Option<String> {
        self.st.lock().user.clone()
    }

    /// `host(host)`: a relative link becomes absolute (`http`).
    pub fn host(&self, host: &JValue) -> Arc<LinkTool> {
        self.copy_with(|c| {
            c.set_host(host);
            if c.get_host().is_some() && !c.is_absolute() {
                c.set_scheme(&JValue::str(DEFAULT_SCHEME));
            }
        })
    }

    /// `getHost()`: `None` when unset.
    pub fn get_host(&self) -> Option<String> {
        self.st.lock().host.clone()
    }

    /// `port(port)`: a copy with this port.
    pub fn port(&self, port: &JValue) -> Arc<LinkTool> {
        self.copy_with(|c| c.set_port(port))
    }

    /// `getPort()`: `null` when unset or invalid.
    pub fn get_port(&self) -> Option<i32> {
        let p = self.st.lock().port;
        (p >= 0).then_some(p)
    }

    /// `path(path)`: a copy with this path.
    pub fn path(&self, path: &JValue) -> Arc<LinkTool> {
        self.copy_with(|c| c.set_path(path))
    }

    /// `getPath()`: `None` when unset.
    pub fn get_path(&self) -> Option<String> {
        self.st.lock().path.clone()
    }

    /// `append(path)`.
    pub fn append(&self, path: &JValue) -> Arc<LinkTool> {
        self.copy_with(|c| c.append_path(path))
    }

    /// `getDirectory()`: the path up to its last slash.
    pub fn get_directory(&self) -> Option<String> {
        let st = self.st.lock();
        if st.opaque {
            return None;
        }
        let p = st.path.as_ref()?;
        Some(match p.rfind('/') {
            None => String::new(),
            Some(i) => p[..=i].to_string(),
        })
    }

    /// `getFile()`: the path after its last slash; `None` without a path or for an opaque link.
    pub fn get_file(&self) -> Option<String> {
        let st = self.st.lock();
        if st.opaque {
            return None;
        }
        let p = st.path.as_ref()?;
        Some(match p.rfind('/') {
            None => p.clone(),
            Some(i) => p[i + 1..].to_string(),
        })
    }

    /// `getRoot()`: the string form of `root()`.
    pub fn get_root(&self) -> Option<String> {
        self.root().and_then(|r| r.java_string_opt())
    }

    /// `root()`: scheme, user, host and port only; `null` without a host.
    pub fn root(&self) -> Option<Arc<LinkTool>> {
        {
            let st = self.st.lock();
            if st.host.is_none() || st.opaque || st.port == -2 {
                return None;
            }
        }
        let copy = self.absolute();
        copy.set_path(&JValue::Null);
        copy.set_query(&JValue::Null);
        copy.set_fragment(&JValue::Null);
        Some(copy)
    }

    /// `directory()`: a copy reduced to the root and this link's directory, without query or fragment.
    pub fn directory(&self) -> Arc<LinkTool> {
        let copy = match self.root() {
            Some(c) => c,
            None => {
                let c = self.duplicate(false);
                c.set_query(&JValue::Null);
                c.set_fragment(&JValue::Null);
                c
            }
        };
        copy.set_path(&self.get_directory().map(JValue::str).unwrap_or(JValue::Null));
        copy
    }

    /// `isRelative()`: whether the link prints without scheme and host (no scheme, or forced relative).
    pub fn is_relative(&self) -> bool {
        let st = self.st.lock();
        st.force_relative || st.scheme.is_none()
    }

    /// `relative()`: a copy that prints relative.
    pub fn relative(&self) -> Arc<LinkTool> {
        self.copy_with(|c| c.set_force_relative(true))
    }

    /// `relative(obj)`: a path relative to this link's directory.
    pub fn relative_to(&self, obj: &JValue) -> Option<Arc<LinkTool>> {
        let copy = self.relative();
        let pth = match string_of(obj) {
            None => self.get_context_path(),
            Some(o) => combine_path(self.get_context_path().as_deref(), Some(&o)),
        };
        copy.set_from_uri(&pth.map(JValue::str).unwrap_or(JValue::Null)).then_some(copy)
    }

    /// `getContextPath()`: here the directory of the path.
    pub fn get_context_path(&self) -> Option<String> {
        self.get_directory()
    }

    /// `isAbsolute()`: whether the link has a scheme and is not forced relative.
    pub fn is_absolute(&self) -> bool {
        let st = self.st.lock();
        st.scheme.is_some() && !st.force_relative
    }

    /// `absolute()`: a copy that prints absolute, `http` when it has no scheme.
    pub fn absolute(&self) -> Arc<LinkTool> {
        self.copy_with(|c| {
            c.set_force_relative(false);
            if c.get_scheme().is_none() {
                c.set_scheme(&JValue::str(DEFAULT_SCHEME));
            }
        })
    }

    /// `absolute(obj)`: a path (relative to this link's directory unless it starts with `http` or `/`).
    pub fn absolute_to(&self, obj: &JValue) -> Option<Arc<LinkTool>> {
        let copy = self.absolute();
        match string_of(obj) {
            None => copy.set_path(&self.get_directory().map(JValue::str).unwrap_or(JValue::Null)),
            Some(p) => {
                let p = if !p.starts_with(DEFAULT_SCHEME) && !p.starts_with('/') { combine_path(self.get_directory().as_deref(), Some(&p)).unwrap_or_default() } else { p };
                if !copy.set_from_uri(&JValue::str(p)) {
                    return None;
                }
            }
        }
        Some(copy)
    }

    /// `uri(uri)`: a copy set from this URI; `None` when it does not parse.
    pub fn uri(&self, uri: &JValue) -> Option<Arc<LinkTool>> {
        let copy = self.duplicate(false);
        copy.set_from_uri(uri).then_some(copy)
    }

    /// `getUri()`: `null` in safe mode.
    pub fn get_uri(&self) -> Option<Uri> {
        if !self.is_safe_mode() { self.create_uri() } else { None }
    }

    /// `getBaseRef()`: the link without query and fragment.
    pub fn get_base_ref(&self) -> Option<String> {
        let copy = self.duplicate(false);
        copy.set_query(&JValue::Null);
        copy.set_fragment(&JValue::Null);
        copy.java_string_opt()
    }

    /// `query(query)`: a copy with this query, a map or a query string.
    pub fn query(&self, query: &JValue) -> Arc<LinkTool> {
        self.copy_with(|c| c.set_query(query))
    }

    /// `getQuery()`: the parameters as a query string, `None` without any.
    pub fn get_query(&self) -> Option<String> {
        let q = self.st.lock().query.clone();
        self.to_query_map(q.as_ref())
    }

    /// `param(key, value)`: a copy with the parameter added, or replaced when parameters are not appended.
    pub fn param(&self, key: &JValue, value: &JValue) -> Arc<LinkTool> {
        let copy = self.duplicate(true);
        copy.set_param(key, value, self.get_append_params());
        copy
    }

    /// `append(key, value)`: a copy with the value added to the parameter's values.
    pub fn append_param_copy(&self, key: &JValue, value: &JValue) -> Arc<LinkTool> {
        let copy = self.duplicate(true);
        copy.set_param(key, value, true);
        copy
    }

    /// `set(key, value)`: a copy with the parameter replaced.
    pub fn set(&self, key: &JValue, value: &JValue) -> Arc<LinkTool> {
        let copy = self.duplicate(true);
        copy.set_param(key, value, false);
        copy
    }

    /// `remove(key)`: a copy without the parameter.
    pub fn remove(&self, key: &JValue) -> Arc<LinkTool> {
        let copy = self.duplicate(true);
        copy.remove_param(key);
        copy
    }

    /// `params(parameters)`: `true`/`false` keeps or drops them, a map or query string adds (or sets) them.
    pub fn params(&self, parameters: &JValue) -> Arc<LinkTool> {
        match parameters {
            JValue::Null => self.this(),
            JValue::Bool(keep) => {
                let copy = self.duplicate(true);
                copy.handle_params_boolean(*keep);
                copy
            }
            JValue::Map(m) if m.is_empty() => self.duplicate(false),
            other => {
                let append = self.get_append_params();
                let copy = self.duplicate(append);
                copy.set_params(other, append);
                copy
            }
        }
    }

    /// `getParams()`: `null` when there are none.
    pub fn get_params(&self) -> Option<JMap> {
        self.st.lock().query.clone().filter(|q| !q.is_empty())
    }

    /// `anchor(anchor)`: a copy with this fragment.
    pub fn anchor(&self, anchor: &JValue) -> Arc<LinkTool> {
        self.copy_with(|c| c.set_fragment(anchor))
    }

    /// `getAnchor()`: the fragment, `None` when unset.
    pub fn get_anchor(&self) -> Option<String> {
        self.st.lock().fragment.clone()
    }

    /// `getSelf()`: the tool every copy came from.
    pub fn get_self(&self) -> Arc<LinkTool> {
        self.origin.clone().unwrap_or_else(|| self.this())
    }

    /// `toString()`: the URI, with the percent escapes of the query's own escapes undone; `null` for nothing.
    pub fn java_string_opt(&self) -> Option<String> {
        let uri = self.create_uri()?;
        if self.st.lock().query.is_some() {
            return Some(decode_query_percents(&uri.java_string()));
        }
        Some(uri.java_string())
    }

    fn java_string(&self) -> String {
        self.java_string_opt().unwrap_or_else(|| "null".into())
    }

    /// `equals(obj)`: the same string.
    pub fn equals(&self, other: &LinkTool) -> bool {
        self.java_string_opt() == other.java_string_opt()
    }

    /// `encode(obj)`: `URLEncoder.encode` in the tool's charset.
    pub fn encode(&self, obj: &JValue) -> Option<String> {
        string_of(obj).map(|s| java_compat::url_encode(&s))
    }

    /// `decode(obj)`: `URLDecoder.decode`; `None` for `null`, `IllegalArgumentException` for a malformed escape.
    pub fn decode(&self, obj: &JValue) -> Result<Option<String>, JError> {
        match string_of(obj) {
            None => Ok(None),
            Some(s) => java_compat::url_decode(&s).map(Some).map_err(|m| JError::new("java.lang.IllegalArgumentException", m)),
        }
    }
}

fn combine_path(start: Option<&str>, end: Option<&str>) -> Option<String> {
    let Some(end) = end else { return start.map(str::to_string) };
    let Some(start) = start else { return Some(end.to_string()) };
    let (se, es) = (start.ends_with('/'), end.starts_with('/'));
    Some(if se ^ es {
        format!("{start}{end}")
    } else if se && es {
        format!("{start}{}", &end[1..])
    } else {
        format!("{start}/{end}")
    })
}

/// `putParam`: an `Object[]` stored as a list.
fn put_param(q: &JMap, key: JValue, value: JValue) {
    let value = match &value {
        JValue::Array(a) if !a.component.is_primitive() => JValue::List(JList::array_list(a.items())),
        _ => value,
    };
    q.put(key, value);
}

/// `appendParam`: a second value turns the entry into a list.
fn append_param(q: &JMap, key: JValue, value: JValue) {
    if !q.contains_key(&key) {
        put_param(q, key, value);
        return;
    }
    let add = |l: &JList, v: &JValue| match v {
        JValue::List(x) => x.items().into_iter().for_each(|i| l.push(i)),
        JValue::Array(a) if !a.component.is_primitive() => a.items().into_iter().for_each(|i| l.push(i)),
        other => l.push(other.clone()),
    };
    match q.get(&key).unwrap_or(JValue::Null) {
        JValue::List(l) => add(&l, &value),
        cur => {
            let l = JList::array_list(vec![cur]);
            add(&l, &value);
            put_param(q, key, JValue::List(l));
        }
    }
}

/// `decodeQueryPercents(url)`: in the query, `%25` back to `%` (other escapes kept).
fn decode_query_percents(url: &str) -> String {
    let mut out = String::new();
    let (mut in_query, mut have_percent, mut have_two) = (false, false, false);
    for c in url.chars() {
        if in_query {
            if have_percent {
                if have_two {
                    out.push('%');
                    if c != '5' {
                        out.push('2');
                        out.push(c);
                    }
                    have_percent = false;
                    have_two = false;
                } else if c == '2' {
                    have_two = true;
                } else {
                    out.push('%');
                    out.push(c);
                    have_percent = false;
                }
            } else if c == '%' {
                have_percent = true;
            } else {
                out.push(c);
            }
            if c == '#' {
                in_query = false;
            }
        } else {
            out.push(c);
            if c == '?' {
                in_query = true;
            }
        }
    }
    if have_percent {
        out.push('%');
        if have_two {
            out.push('2');
        }
    }
    out
}

impl Tool for LinkTool {
    fn safe(&self) -> &SafeConfig {
        &self.safe
    }

    fn configure_values(&self, props: &ValueParser) {
        let opt = |v: Option<String>| v.map(JValue::str);
        if let Some(l) = opt(props.get_string(URI_KEY)) {
            self.set_from_uri(&l);
        }
        if let Some(v) = opt(props.get_string(SCHEME_KEY)) {
            self.set_scheme(&v);
        }
        if let Some(v) = opt(props.get_string(USER_KEY)) {
            self.set_user_info(&v);
        }
        if let Some(v) = opt(props.get_string(HOST_KEY)) {
            self.set_host(&v);
        }
        if let Some(p) = props.get_integer(PORT_KEY) {
            self.set_port(&JValue::Int(p));
        }
        if let Some(v) = opt(props.get_string(PATH_KEY)) {
            self.set_path(&v);
        }
        if let Some(v) = opt(props.get_string(QUERY_KEY)) {
            self.set_query(&v);
        }
        if let Some(v) = opt(props.get_string(FRAGMENT_KEY)) {
            self.set_fragment(&v);
        }
        if let Some(c) = props.get_string(CHARSET_KEY) {
            self.set_character_encoding(&c);
        }
        if let Some(x) = props.get_boolean(XHTML_MODE_KEY) {
            self.set_xhtml(x);
        }
        if let Some(a) = props.get_boolean(APPEND_PARAMS_KEY) {
            self.set_append_params(a);
        }
        if let Some(f) = props.get_boolean(FORCE_RELATIVE_KEY) {
            self.set_force_relative(f);
        }
    }
}

fn link(t: Option<Arc<LinkTool>>) -> super::Ret {
    val(t.map(|t| t.value()).unwrap_or(JValue::Null))
}

fn def() -> Arc<ClassDef> {
    type L = LinkTool;
    let me = || c(CLASS);
    let b = || JType::Boolean;
    let def = ClassDef::new(CLASS)
        .extends(&[safe::SAFE_CONFIG])
        .implements(&["java.lang.Cloneable", "java.io.Serializable"])
        .method::<L>("setCharacterEncoding", &[s()], None, |t, _, a| {
            t.set_character_encoding(&a[0].to_java_string());
            Ok(None)
        })
        .method::<L>("setXHTML", &[b()], None, |t, _, a| {
            t.set_xhtml(matches!(a[0], JValue::Bool(true)));
            Ok(None)
        })
        .method::<L>("setAppendParams", &[b()], None, |t, _, a| {
            t.set_append_params(matches!(a[0], JValue::Bool(true)));
            Ok(None)
        })
        .method::<L>("setForceRelative", &[b()], None, |t, _, a| {
            t.set_force_relative(matches!(a[0], JValue::Bool(true)));
            Ok(None)
        })
        .method::<L>("setScheme", &[o()], None, |t, _, a| {
            t.set_scheme(&a[0]);
            Ok(None)
        })
        .method::<L>("setUserInfo", &[o()], None, |t, _, a| {
            t.set_user_info(&a[0]);
            Ok(None)
        })
        .method::<L>("setHost", &[o()], None, |t, _, a| {
            t.set_host(&a[0]);
            Ok(None)
        })
        .method::<L>("setPort", &[o()], None, |t, _, a| {
            t.set_port(&a[0]);
            Ok(None)
        })
        .method::<L>("setPath", &[o()], None, |t, _, a| {
            t.set_path(&a[0]);
            Ok(None)
        })
        .method::<L>("appendPath", &[o()], None, |t, _, a| {
            t.append_path(&a[0]);
            Ok(None)
        })
        .method::<L>("setQuery", &[o()], None, |t, _, a| {
            t.set_query(&a[0]);
            Ok(None)
        })
        .method::<L>("toQuery", &[c("java.util.Map")], Some(s()), |t, _, a| super::text(t.to_query_map(match &a[0] {
            JValue::Map(m) => Some(m),
            _ => None,
        })))
        .method::<L>("appendQuery", &[o()], None, |t, _, a| {
            t.append_query(&a[0]);
            Ok(None)
        })
        .method::<L>("setParam", &[o(), o(), b()], None, |t, _, a| {
            t.set_param(&a[0], &a[1], matches!(a[2], JValue::Bool(true)));
            Ok(None)
        })
        .method::<L>("setParams", &[o(), b()], None, |t, _, a| {
            t.set_params(&a[0], matches!(a[1], JValue::Bool(true)));
            Ok(None)
        })
        .method::<L>("removeParam", &[o()], Some(o()), |t, _, a| val(t.remove_param(&a[0])))
        .method::<L>("setFragment", &[o()], None, |t, _, a| {
            t.set_fragment(&a[0]);
            Ok(None)
        })
        .method::<L>("getCharacterEncoding", &[], Some(s()), |t, _, _| val(JValue::str(t.get_character_encoding())))
        .method::<L>("isXHTML", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_xhtml())))
        .method::<L>("getAppendParams", &[], Some(b()), |t, _, _| val(JValue::Bool(t.get_append_params())))
        .method::<L>("scheme", &[o()], Some(me()), |t, _, a| link(Some(t.scheme(&a[0]))))
        .method::<L>("secure", &[], Some(me()), |t, _, _| link(Some(t.secure())))
        .method::<L>("insecure", &[], Some(me()), |t, _, _| link(Some(t.insecure())))
        .method::<L>("getScheme", &[], Some(s()), |t, _, _| super::text(t.get_scheme()))
        .method::<L>("isSecure", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_secure())))
        .method::<L>("isOpaque", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_opaque())))
        .method::<L>("user", &[o()], Some(me()), |t, _, a| link(Some(t.user(&a[0]))))
        .method::<L>("getUser", &[], Some(s()), |t, _, _| super::text(t.get_user()))
        .method::<L>("host", &[o()], Some(me()), |t, _, a| link(Some(t.host(&a[0]))))
        .method::<L>("getHost", &[], Some(s()), |t, _, _| super::text(t.get_host()))
        .method::<L>("port", &[o()], Some(me()), |t, _, a| link(Some(t.port(&a[0]))))
        .method::<L>("getPort", &[], Some(c("java.lang.Integer")), |t, _, _| val(t.get_port().map(JValue::Int).unwrap_or(JValue::Null)))
        .method::<L>("path", &[o()], Some(me()), |t, _, a| link(Some(t.path(&a[0]))))
        .method::<L>("getPath", &[], Some(s()), |t, _, _| super::text(t.get_path()))
        .method::<L>("append", &[o()], Some(me()), |t, _, a| link(Some(t.append(&a[0]))))
        .method::<L>("getDirectory", &[], Some(s()), |t, _, _| super::text(t.get_directory()))
        .method::<L>("getFile", &[], Some(s()), |t, _, _| super::text(t.get_file()))
        .method::<L>("getRoot", &[], Some(s()), |t, _, _| super::text(t.get_root()))
        .method::<L>("root", &[], Some(me()), |t, _, _| link(t.root()))
        .method::<L>("directory", &[], Some(me()), |t, _, _| link(Some(t.directory())))
        .method::<L>("isRelative", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_relative())))
        .method::<L>("relative", &[], Some(me()), |t, _, _| link(Some(t.relative())))
        .method::<L>("relative", &[o()], Some(me()), |t, _, a| link(t.relative_to(&a[0])))
        .method::<L>("getContextPath", &[], Some(s()), |t, _, _| super::text(t.get_context_path()))
        .method::<L>("isAbsolute", &[], Some(b()), |t, _, _| val(JValue::Bool(t.is_absolute())))
        .method::<L>("absolute", &[], Some(me()), |t, _, _| link(Some(t.absolute())))
        .method::<L>("absolute", &[o()], Some(me()), |t, _, a| link(t.absolute_to(&a[0])))
        .method::<L>("uri", &[o()], Some(me()), |t, _, a| link(t.uri(&a[0])))
        .method::<L>("getUri", &[], Some(c(uri::URI)), |t, _, _| val(t.get_uri().map(Uri::value).unwrap_or(JValue::Null)))
        .method::<L>("getBaseRef", &[], Some(s()), |t, _, _| super::text(t.get_base_ref()))
        .method::<L>("query", &[o()], Some(me()), |t, _, a| link(Some(t.query(&a[0]))))
        .method::<L>("getQuery", &[], Some(s()), |t, _, _| super::text(t.get_query()))
        .method::<L>("param", &[o(), o()], Some(me()), |t, _, a| link(Some(t.param(&a[0], &a[1]))))
        .method::<L>("append", &[o(), o()], Some(me()), |t, _, a| link(Some(t.append_param_copy(&a[0], &a[1]))))
        .method::<L>("set", &[o(), o()], Some(me()), |t, _, a| link(Some(t.set(&a[0], &a[1]))))
        .method::<L>("remove", &[o()], Some(me()), |t, _, a| link(Some(t.remove(&a[0]))))
        .method::<L>("params", &[o()], Some(me()), |t, _, a| link(Some(t.params(&a[0]))))
        .method::<L>("getParams", &[], Some(c("java.util.Map")), |t, _, _| val(t.get_params().map(JValue::Map).unwrap_or(JValue::Null)))
        .method::<L>("anchor", &[o()], Some(me()), |t, _, a| link(Some(t.anchor(&a[0]))))
        .method::<L>("getAnchor", &[], Some(s()), |t, _, _| super::text(t.get_anchor()))
        .method::<L>("getSelf", &[], Some(me()), |t, _, _| link(Some(t.get_self())))
        .method::<L>("toString", &[], Some(s()), |t, _, _| super::text(t.java_string_opt()))
        .method::<L>("equals", &[o()], Some(b()), |t, _, a| {
            val(JValue::Bool(match &a[0] {
                JValue::Object(x) => x.as_any().downcast_ref::<LinkTool>().is_some_and(|other| t.equals(other)),
                _ => false,
            }))
        })
        .method::<L>("hashCode", &[], Some(JType::Int), |t, _, _| val(JValue::Int(t.java_string_opt().map(|s| java_compat::lang::string_hash(&s)).unwrap_or(-1))))
        .method::<L>("encode", &[o()], Some(s()), |t, _, a| super::text(t.encode(&a[0])))
        .method::<L>("decode", &[o()], Some(s()), |t, _, a| super::text(t.decode(&a[0])?));
    super::object_methods::<L>(safe::safe_config_methods::<L>(def)).register()
}

tool_object!(LinkTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || LinkTool::new().value())
        .annotated(Annotation::default_key("link"))
        .annotated(Annotation::skip_setters())
        .annotated(Annotation::valid_scope(&[super::scope::REQUEST]))
}

/// A `Boolean[]` for building query arrays.
pub fn booleans(values: &[bool]) -> JValue {
    JValue::Array(Arc::new(JArray::new(c("java.lang.Boolean"), values.iter().map(|b| JValue::Bool(*b)).collect())))
}
