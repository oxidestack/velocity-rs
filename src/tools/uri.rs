//! What `LinkTool` needs of `java.net.URI`: parsing a URI reference (RFC 2396 with the JDK's deviations and
//! error messages), building one from components (quoting what each component does not allow), and the decoded
//! component getters.

use std::sync::Arc;

use java_compat::lang::{JError, JObject, JValue};
use java_compat::reflect::{ClassDef, JType};

use super::{s, val};

fn in_set(c: char, set: &str) -> bool {
    set.contains(c)
}

const MARK: &str = "-_.!~*'()";
const RESERVED: &str = ";/?:@&=+$,[]";

fn alphanum(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

fn unreserved(c: char) -> bool {
    alphanum(c) || in_set(c, MARK)
}

/// A character class of the grammar, with `%` escapes (and non-ASCII "other" characters) allowed or not.
#[derive(Clone, Copy)]
enum Class {
    Uric,
    Path,
    UserInfo,
    RegName,
    Server,
    Scheme,
    Digit,
}

impl Class {
    fn matches(self, c: char) -> bool {
        match self {
            Class::Uric => unreserved(c) || in_set(c, RESERVED),
            Class::Path => unreserved(c) || in_set(c, ":@&=+$,;/"),
            Class::UserInfo => unreserved(c) || in_set(c, ";:&=+$,"),
            Class::RegName => unreserved(c) || in_set(c, "$,;:@&=+"),
            Class::Server => unreserved(c) || in_set(c, ";:&=+$,") || in_set(c, "-.:@[]"),
            Class::Scheme => alphanum(c) || in_set(c, "+-."),
            Class::Digit => c.is_ascii_digit(),
        }
    }

    fn escapes(self) -> bool {
        !matches!(self, Class::Scheme | Class::Digit)
    }
}

/// `Character.isSpaceChar(c) || Character.isISOControl(c)`.
fn space_or_control(c: char) -> bool {
    c.is_control() || matches!(c, ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

fn syntax(input: &str, reason: &str, index: usize) -> JError {
    JError::new("java.net.URISyntaxException", format!("{reason} at index {index}: {input}"))
}

struct Parser<'a> {
    input: &'a str,
    chars: Vec<char>,
    uri: Uri,
}

impl<'a> Parser<'a> {
    fn at(&self, p: usize, c: char) -> bool {
        self.chars.get(p) == Some(&c)
    }

    fn fail(&self, reason: &str, p: usize) -> JError {
        syntax(self.input, reason, p)
    }

    fn expecting(&self, what: &str, p: usize) -> JError {
        self.fail(&format!("Expected {what}"), p)
    }

    /// `scan(start, end, err, stop)`: the first stop character; `None` at an error character.
    fn scan_stop(&self, start: usize, end: usize, err: &str, stop: &str) -> Option<usize> {
        let mut p = start;
        while p < end {
            let c = self.chars[p];
            if err.contains(c) {
                return None;
            }
            if stop.contains(c) {
                break;
            }
            p += 1;
        }
        Some(p)
    }

    /// `scanEscape`: past a valid escape (or non-ASCII "other" character).
    fn scan_escape(&self, p: usize, n: usize, c: char) -> Result<usize, JError> {
        if c == '%' {
            if p + 3 <= n && self.chars[p + 1].is_ascii_hexdigit() && self.chars[p + 2].is_ascii_hexdigit() {
                return Ok(p + 3);
            }
            return Err(self.fail("Malformed escape pair", p));
        }
        if (c as u32) >= 0x80 && !space_or_control(c) {
            return Ok(p + 1);
        }
        Ok(p)
    }

    /// `scan(start, n, lowMask, highMask)`: past the characters of the class.
    fn scan_class(&self, start: usize, n: usize, class: Class) -> Result<usize, JError> {
        let mut p = start;
        while p < n {
            let c = self.chars[p];
            if (c as u32) < 0x80 && class.matches(c) {
                p += 1;
                continue;
            }
            if class.escapes() {
                let q = self.scan_escape(p, n, c)?;
                if q > p {
                    p = q;
                    continue;
                }
            }
            break;
        }
        Ok(p)
    }

    fn check_chars(&self, start: usize, end: usize, class: Class, what: &str) -> Result<(), JError> {
        let p = self.scan_class(start, end, class)?;
        if p < end {
            return Err(self.fail(&format!("Illegal character in {what}"), p));
        }
        Ok(())
    }

    fn sub(&self, a: usize, b: usize) -> String {
        self.chars[a..b].iter().collect()
    }

    fn parse(&mut self, require_server: bool) -> Result<(), JError> {
        let n = self.chars.len();
        let ssp;
        let mut p;
        match self.scan_stop(0, n, "/?#", ":") {
            Some(q) if self.at(q, ':') => {
                if q == 0 {
                    return Err(self.expecting("scheme name", 0));
                }
                if !self.chars[0].is_ascii_alphabetic() {
                    return Err(self.fail("Illegal character in scheme name", 0));
                }
                self.check_chars(1, q, Class::Scheme, "scheme name")?;
                self.uri.scheme = Some(self.sub(0, q));
                p = q + 1;
                ssp = p;
                if self.at(p, '/') {
                    p = self.parse_hierarchical(p, n, require_server)?;
                } else {
                    let q = self.scan_stop(p, n, "", "#").unwrap_or(n);
                    if q <= p {
                        return Err(self.expecting("scheme-specific part", p));
                    }
                    self.check_chars(p, q, Class::Uric, "opaque part")?;
                    p = q;
                }
            }
            _ => {
                ssp = 0;
                p = self.parse_hierarchical(0, n, require_server)?;
            }
        }
        self.uri.ssp = self.sub(ssp, p);
        if self.at(p, '#') {
            self.check_chars(p + 1, n, Class::Uric, "fragment")?;
            self.uri.fragment = Some(self.sub(p + 1, n));
            p = n;
        }
        if p < n {
            return Err(self.fail("Expected end of URI", p));
        }
        Ok(())
    }

    fn parse_hierarchical(&mut self, start: usize, n: usize, require_server: bool) -> Result<usize, JError> {
        let mut p = start;
        if self.at(p, '/') && self.at(p + 1, '/') {
            p += 2;
            let q = self.scan_stop(p, n, "", "/?#").unwrap_or(n);
            if q > p {
                p = self.parse_authority(p, q, require_server)?;
            } else if q >= n {
                return Err(self.expecting("authority", p));
            }
        }
        let q = self.scan_stop(p, n, "", "?#").unwrap_or(n);
        self.check_chars(p, q, Class::Path, "path")?;
        self.uri.path = Some(self.sub(p, q));
        p = q;
        if self.at(p, '?') {
            p += 1;
            let q = self.scan_stop(p, n, "", "#").unwrap_or(n);
            self.check_chars(p, q, Class::Uric, "query")?;
            self.uri.query = Some(self.sub(p, q));
            p = q;
        }
        Ok(p)
    }

    fn parse_authority(&mut self, start: usize, n: usize, require_server: bool) -> Result<usize, JError> {
        let p = start;
        let server_chars = self.scan_class(p, n, Class::Server)? == n;
        let reg_chars = self.scan_class(p, n, Class::RegName)? == n;
        if reg_chars && !server_chars {
            self.uri.authority = Some(self.sub(p, n));
            return Ok(n);
        }
        let mut q = p;
        let mut ex = None;
        if server_chars {
            match self.parse_server(p, n) {
                Ok(end) => {
                    if end < n {
                        return Err(self.expecting("end of authority", end));
                    }
                    self.uri.authority = Some(self.sub(p, n));
                    q = end;
                }
                Err(e) => {
                    self.uri.user_info = None;
                    self.uri.host = None;
                    self.uri.port = -1;
                    if require_server {
                        return Err(e);
                    }
                    ex = Some(e);
                    q = p;
                }
            }
        }
        if q < n {
            if reg_chars {
                self.uri.authority = Some(self.sub(p, n));
            } else if let Some(e) = ex {
                return Err(e);
            } else {
                return Err(self.fail("Illegal character in authority", q));
            }
        }
        Ok(n)
    }

    fn parse_server(&mut self, start: usize, n: usize) -> Result<usize, JError> {
        let mut p = start;
        if let Some(q) = self.scan_stop(p, n, "/?#", "@") {
            if self.at(q, '@') {
                self.check_chars(p, q, Class::UserInfo, "user info")?;
                self.uri.user_info = Some(self.sub(p, q));
                p = q + 1;
            }
        }
        if self.at(p, '[') {
            let close = (p..n).find(|i| self.chars[*i] == ']').ok_or_else(|| self.fail("Expected closing bracket for IPv6 address", n))?;
            let inner = self.sub(p + 1, close);
            if inner.is_empty() || !inner.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.') {
                return Err(self.fail("Malformed IPv6 address", p + 1));
            }
            self.uri.host = Some(self.sub(p, close + 1));
            p = close + 1;
        } else {
            let q = self.parse_ipv4(p, n);
            p = match q {
                Some(q) => {
                    self.uri.host = Some(self.sub(p, q));
                    q
                }
                None => self.parse_hostname(p, n)?,
            };
        }
        if self.at(p, ':') {
            p += 1;
            let q = self.scan_stop(p, n, "", "/").unwrap_or(n);
            if q > p {
                self.check_chars(p, q, Class::Digit, "port number")?;
                self.uri.port = self.sub(p, q).parse().map_err(|_| self.fail("Malformed port number", p))?;
            }
            p = q;
        }
        if p < n {
            return Err(self.expecting("port number", p));
        }
        Ok(p)
    }

    /// A dotted IPv4 address followed by the end or a port.
    fn parse_ipv4(&self, start: usize, n: usize) -> Option<usize> {
        let mut p = start;
        for i in 0..4 {
            let d = (p..n).take_while(|j| self.chars[*j].is_ascii_digit()).count();
            if d == 0 || d > 3 || self.sub(p, p + d).parse::<u32>().ok()? > 255 {
                return None;
            }
            p += d;
            if i < 3 {
                if !self.at(p, '.') {
                    return None;
                }
                p += 1;
            }
        }
        (p == n || self.at(p, ':')).then_some(p)
    }

    fn parse_hostname(&mut self, start: usize, n: usize) -> Result<usize, JError> {
        let mut p = start;
        let mut last_label: Option<usize> = None;
        loop {
            let q = (p..n).take_while(|j| alphanum(self.chars[*j])).count() + p;
            if q <= p {
                break;
            }
            last_label = Some(p);
            p = q;
            let q = (p..n).take_while(|j| alphanum(self.chars[*j]) || self.chars[*j] == '-').count() + p;
            if q > p {
                if self.chars[q - 1] == '-' {
                    return Err(self.fail("Illegal character in hostname", q - 1));
                }
                p = q;
            }
            if !self.at(p, '.') {
                break;
            }
            p += 1;
            if p >= n {
                break;
            }
        }
        if p < n && !self.at(p, ':') {
            return Err(self.fail("Illegal character in hostname", p));
        }
        let Some(l) = last_label else { return Err(self.expecting("hostname", start)) };
        if l > start && !self.chars[l].is_ascii_alphabetic() {
            return Err(self.fail("Illegal character in hostname", l));
        }
        self.uri.host = Some(self.sub(start, p));
        Ok(p)
    }
}

/// A `java.net.URI`: its string and components (raw, as they appear in the string).
#[derive(Clone, Debug, Default)]
pub struct Uri {
    string: String,
    scheme: Option<String>,
    ssp: String,
    authority: Option<String>,
    user_info: Option<String>,
    host: Option<String>,
    port: i32,
    path: Option<String>,
    query: Option<String>,
    fragment: Option<String>,
}

impl PartialEq for Uri {
    fn eq(&self, other: &Uri) -> bool {
        self.string == other.string
    }
}

/// `%XX` escapes decoded as UTF-8.
fn decode(s: &Option<String>) -> Option<String> {
    let s = s.as_ref()?;
    if !s.contains('%') {
        return Some(s.clone());
    }
    let mut bytes = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            let hex = |x: u8| (x as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                bytes.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        bytes.push(b[i]);
        i += 1;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// `URI.quote(s, mask)`: what the class does not allow escaped, space and control characters as UTF-8.
fn quote(s: &str, class: Class, out: &mut String) {
    for c in s.chars() {
        if (c as u32) < 0x80 {
            if class.matches(c) {
                out.push(c);
            } else {
                out.push_str(&format!("%{:02X}", c as u32));
            }
        } else if space_or_control(c) {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(c);
        }
    }
}

impl Uri {
    /// `new URI(str)`.
    pub fn parse(s: &str) -> Result<Uri, JError> {
        let mut p = Parser { input: s, chars: s.chars().collect(), uri: Uri { string: s.to_string(), port: -1, ..Uri::default() } };
        p.parse(false)?;
        Ok(p.uri)
    }

    fn parse_server(s: &str) -> Result<Uri, JError> {
        let mut p = Parser { input: s, chars: s.chars().collect(), uri: Uri { string: s.to_string(), port: -1, ..Uri::default() } };
        p.parse(true)?;
        Ok(p.uri)
    }

    /// `new URI(scheme, userInfo, host, port, path, query, fragment)`.
    pub fn from_components(scheme: Option<&str>, user_info: Option<&str>, host: Option<&str>, port: i32, path: Option<&str>, query: Option<&str>, fragment: Option<&str>) -> Result<Uri, JError> {
        let mut out = String::new();
        if let Some(sc) = scheme {
            out.push_str(sc);
            out.push(':');
        }
        if let Some(h) = host {
            out.push_str("//");
            if let Some(u) = user_info {
                quote(u, Class::UserInfo, &mut out);
                out.push('@');
            }
            let bracket = h.contains(':') && !h.starts_with('[') && !h.ends_with(']');
            if bracket {
                out.push('[');
            }
            out.push_str(h);
            if bracket {
                out.push(']');
            }
            if port != -1 {
                out.push_str(&format!(":{port}"));
            }
        }
        if let Some(pth) = path {
            quote(pth, Class::Path, &mut out);
        }
        if let Some(q) = query {
            out.push('?');
            quote(q, Class::Uric, &mut out);
        }
        if let Some(f) = fragment {
            out.push('#');
            quote(f, Class::Uric, &mut out);
        }
        if scheme.is_some() && path.is_some_and(|p| !p.is_empty() && !p.starts_with('/')) {
            return Err(syntax(&out, "Relative path in absolute URI", 0));
        }
        Uri::parse_server(&out)
    }

    /// `new URI(scheme, ssp, fragment)`.
    pub fn opaque(scheme: Option<&str>, ssp: Option<&str>, fragment: Option<&str>) -> Result<Uri, JError> {
        let mut out = String::new();
        if let Some(sc) = scheme {
            out.push_str(sc);
            out.push(':');
        }
        if let Some(sp) = ssp {
            quote(sp, Class::Uric, &mut out);
        }
        if let Some(f) = fragment {
            out.push('#');
            quote(f, Class::Uric, &mut out);
        }
        Uri::parse(&out)
    }

    /// `getScheme()`: `None` for a relative URI.
    pub fn get_scheme(&self) -> Option<String> {
        self.scheme.clone()
    }

    /// `isOpaque()`: `mailto:x`, not `http://x`.
    pub fn is_opaque(&self) -> bool {
        self.path.is_none()
    }

    /// `getSchemeSpecificPart()`: decoded.
    pub fn get_scheme_specific_part(&self) -> Option<String> {
        decode(&Some(self.ssp.clone()))
    }

    /// `getUserInfo()`: decoded, `None` when absent.
    pub fn get_user_info(&self) -> Option<String> {
        decode(&self.user_info)
    }

    /// `getHost()`: `None` when absent or not server-based.
    pub fn get_host(&self) -> Option<String> {
        self.host.clone()
    }

    /// `getPort()`: `-1` when undefined.
    pub fn get_port(&self) -> i32 {
        self.port
    }

    /// `getAuthority()`: decoded, `None` when absent.
    pub fn get_authority(&self) -> Option<String> {
        decode(&self.authority)
    }

    /// `getPath()`: decoded; `None` for an opaque URI.
    pub fn get_path(&self) -> Option<String> {
        decode(&self.path)
    }

    /// `getQuery()`: decoded, `None` when absent.
    pub fn get_query(&self) -> Option<String> {
        decode(&self.query)
    }

    /// `getFragment()`: decoded, `None` when absent.
    pub fn get_fragment(&self) -> Option<String> {
        decode(&self.fragment)
    }

    /// `getRawQuery()`: as written, still percent-encoded.
    pub fn get_raw_query(&self) -> Option<String> {
        self.query.clone()
    }

    /// `isAbsolute()`: whether the URI has a scheme.
    pub fn is_absolute(&self) -> bool {
        self.scheme.is_some()
    }

    /// `toString()`: the URI as written (quoted where the constructor quoted it).
    pub fn java_string(&self) -> String {
        self.string.clone()
    }

    /// This URI as a `java.net.URI` template value.
    pub fn value(self) -> JValue {
        JValue::Object(Arc::new(UriObject(Arc::new(self))) as Arc<dyn JObject>)
    }
}

/// A `java.net.URI` value.
pub struct UriObject(pub Arc<Uri>);

/// The class name of `java.net.URI`.
pub const URI: &str = "java.net.URI";

/// The URI behind a `java.net.URI` value.
pub fn as_uri(v: &JValue) -> Option<Arc<Uri>> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<UriObject>().map(|u| u.0.clone()),
        _ => None,
    }
}

fn text(v: Option<String>) -> super::Ret {
    super::text(v)
}

fn uri_def() -> Arc<ClassDef> {
    type U = UriObject;
    ClassDef::new(URI)
        .implements(&["java.lang.Comparable", "java.io.Serializable"])
        .method::<U>("getScheme", &[], Some(s()), |u, _, _| text(u.0.get_scheme()))
        .method::<U>("isOpaque", &[], Some(JType::Boolean), |u, _, _| val(JValue::Bool(u.0.is_opaque())))
        .method::<U>("isAbsolute", &[], Some(JType::Boolean), |u, _, _| val(JValue::Bool(u.0.is_absolute())))
        .method::<U>("getSchemeSpecificPart", &[], Some(s()), |u, _, _| text(u.0.get_scheme_specific_part()))
        .method::<U>("getAuthority", &[], Some(s()), |u, _, _| text(u.0.get_authority()))
        .method::<U>("getUserInfo", &[], Some(s()), |u, _, _| text(u.0.get_user_info()))
        .method::<U>("getHost", &[], Some(s()), |u, _, _| text(u.0.get_host()))
        .method::<U>("getPort", &[], Some(JType::Int), |u, _, _| val(JValue::Int(u.0.get_port())))
        .method::<U>("getPath", &[], Some(s()), |u, _, _| text(u.0.get_path()))
        .method::<U>("getQuery", &[], Some(s()), |u, _, _| text(u.0.get_query()))
        .method::<U>("getRawQuery", &[], Some(s()), |u, _, _| text(u.0.get_raw_query()))
        .method::<U>("getFragment", &[], Some(s()), |u, _, _| text(u.0.get_fragment()))
        .method::<U>("toString", &[], Some(s()), |u, _, _| val(JValue::str(u.0.java_string())))
        .method::<U>("equals", &[super::o()], Some(JType::Boolean), |u, _, a| val(JValue::Bool(as_uri(&a[0]).is_some_and(|o| *o == *u.0))))
        .method::<U>("hashCode", &[], Some(JType::Int), |u, _, _| val(JValue::Int(java_compat::lang::string_hash(&u.0.string))))
        .register()
}

impl JObject for UriObject {
    fn class_name(&self) -> String {
        URI.into()
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<super::Ret> {
        None
    }

    fn to_java_string(&self) -> String {
        self.0.java_string()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<ClassDef>> {
        static DEF: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(uri_def);
        Some(DEF.get())
    }
}
