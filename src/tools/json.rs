//! `org.apache.velocity.tools.generic.JsonTool` (`$json`) and `JsonContent`, over json-simple 3.0.2's
//! `Jsoner.deserialize`: its JFlex lexer (`jsonstrict.lex`) and its lenient state machine (commas and colons
//! optional, a trailing comma allowed), numbers as `BigDecimal`, objects as `HashMap`s.

use std::sync::{Arc, Weak};

use java_compat::lang::{JError, JList, JMap, JObject, JValue, ListKind, MapKind};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;

use super::{c, o, object_methods, s, tool_object, val};
use crate::math;

#[derive(Debug, Clone)]
enum Token {
    Datum(JValue),
    LeftBrace,
    RightBrace,
    LeftSquare,
    RightSquare,
    Comma,
    Colon,
    End,
}

/// `JsonException` (position, problem).
fn json_error(position: usize, problem: &str) -> JError {
    JError::new("com.github.cliftonlabs.json_simple.JsonException", format!("{problem} at position {position}. Fix the parsable string and try again."))
}

struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    /// `yychar`: where the last token started.
    start: usize,
    _src: &'a str,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Lexer<'a> {
        Lexer { chars: src.chars().collect(), pos: 0, start: 0, _src: src }
    }

    fn peek(&self, off: usize) -> Option<char> {
        self.chars.get(self.pos + off).copied()
    }

    /// `DOUBLE = [-]?[0-9]+((\.[0-9]+)?([eE][-+]?[0-9]+)?)`, longest match.
    fn number_len(&self) -> usize {
        let mut i = 0;
        if self.peek(0) == Some('-') {
            i = 1;
        }
        let digits = |from: usize| (from..).take_while(|j| self.peek(*j).is_some_and(|c| c.is_ascii_digit())).count();
        let d = digits(i);
        if d == 0 {
            return 0;
        }
        i += d;
        if self.peek(i) == Some('.') {
            let f = digits(i + 1);
            if f > 0 {
                i += 1 + f;
            }
        }
        if matches!(self.peek(i), Some('e' | 'E')) {
            let mut j = i + 1;
            if matches!(self.peek(j), Some('+' | '-')) {
                j += 1;
            }
            let e = digits(j);
            if e > 0 {
                i = j + e;
            }
        }
        i
    }

    fn next(&mut self) -> Result<Token, JError> {
        loop {
            let Some(c) = self.peek(0) else { return Ok(Token::End) };
            let start = self.pos;
            self.start = start;
            match c {
                ' ' | '\t' | '\r' | '\n' => {
                    self.pos += 1;
                    continue;
                }
                '"' => {
                    self.pos += 1;
                    return self.string();
                }
                '{' | '}' | '[' | ']' | ',' | ':' => {
                    self.pos += 1;
                    return Ok(match c {
                        '{' => Token::LeftBrace,
                        '}' => Token::RightBrace,
                        '[' => Token::LeftSquare,
                        ']' => Token::RightSquare,
                        ',' => Token::Comma,
                        _ => Token::Colon,
                    });
                }
                _ => {}
            }
            let n = self.number_len();
            if n > 0 {
                let text: String = self.chars[start..start + n].iter().collect();
                self.pos += n;
                let d = math::Decimal::parse(&text).expect("the lexer only accepts decimal syntax");
                return Ok(Token::Datum(math::decimal_value(&d)));
            }
            for (word, value) in [("true", JValue::Bool(true)), ("false", JValue::Bool(false)), ("null", JValue::Null)] {
                if self.chars[start..].iter().take(word.len()).copied().eq(word.chars()) {
                    self.pos += word.len();
                    return Ok(Token::Datum(value));
                }
            }
            return Err(json_error(start, &format!("The unexpected character ({c}) was found")));
        }
    }

    fn string(&mut self) -> Result<Token, JError> {
        let mut units: Vec<u16> = Vec::new();
        let push = |units: &mut Vec<u16>, c: char| {
            let mut b = [0u16; 2];
            units.extend_from_slice(c.encode_utf16(&mut b));
        };
        loop {
            let Some(c) = self.peek(0) else { return Ok(Token::End) };
            match c {
                '"' => {
                    self.pos += 1;
                    return Ok(Token::Datum(JValue::str(String::from_utf16_lossy(&units))));
                }
                '\\' => {
                    let escape = match self.peek(1) {
                        Some('"') => Some('"'),
                        Some('\\') => Some('\\'),
                        Some('/') => Some('/'),
                        Some('b') => Some('\u{8}'),
                        Some('f') => Some('\u{c}'),
                        Some('n') => Some('\n'),
                        Some('r') => Some('\r'),
                        Some('t') => Some('\t'),
                        _ => None,
                    };
                    if let Some(e) = escape {
                        push(&mut units, e);
                        self.pos += 2;
                    } else if self.peek(1) == Some('u') && (2..6).all(|i| self.peek(i).is_some_and(|c| c.is_ascii_hexdigit())) {
                        let hex: String = self.chars[self.pos + 2..self.pos + 6].iter().collect();
                        units.push(u16::from_str_radix(&hex, 16).expect("hex digits"));
                        self.pos += 6;
                    } else {
                        units.push(b'\\' as u16);
                        self.pos += 1;
                    }
                }
                other => {
                    push(&mut units, other);
                    self.pos += 1;
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Initial,
    Done,
    ParsedError,
    ParsingArray,
    ParsingObject,
    ParsingEntry,
}

enum Slot {
    Value(JValue),
    Key(String),
}

fn token_name(t: &Token) -> String {
    match t {
        Token::Datum(v) => format!("DATUM({})", if v.is_null() { "null".into() } else { v.to_java_string() }),
        Token::LeftBrace => "LEFT_BRACE({)".into(),
        Token::RightBrace => "RIGHT_BRACE(})".into(),
        Token::LeftSquare => "LEFT_SQUARE([)".into(),
        Token::RightSquare => "RIGHT_SQUARE(])".into(),
        Token::Comma => "COMMA(,)".into(),
        Token::Colon => "COLON(:)".into(),
        Token::End => "END(null)".into(),
    }
}

/// `Jsoner.deserialize(String)` with arrays, objects and bare data allowed.
pub fn deserialize(src: &str) -> Result<JValue, JError> {
    let mut lexer = Lexer::new(src);
    let mut states = vec![State::Initial];
    let mut values: Vec<Slot> = Vec::new();
    let unexpected = |lexer: &Lexer, t: &Token| json_error(lexer.start, &format!("The unexpected token {} was found", token_name(t)));
    let last_value = |values: &Vec<Slot>| match values.last() {
        Some(Slot::Value(v)) => v.clone(),
        _ => JValue::Null,
    };
    loop {
        let state = states.pop().unwrap_or(State::ParsedError);
        let token = lexer.next()?;
        let end = matches!(token, Token::End);
        match state {
            State::Done => {}
            State::Initial => match token {
                Token::Datum(ref v) => {
                    values.push(Slot::Value(v.clone()));
                    states.push(State::Done);
                }
                Token::LeftBrace => {
                    values.push(Slot::Value(JValue::Map(JMap::new(MapKind::Hash))));
                    states.push(State::ParsingObject);
                }
                Token::LeftSquare => {
                    values.push(Slot::Value(JValue::List(JList::array_list(Vec::new()))));
                    states.push(State::ParsingArray);
                }
                ref t => return Err(unexpected(&lexer, t)),
            },
            State::ParsedError => return Err(unexpected(&lexer, &token)),
            State::ParsingArray => {
                let JValue::List(array) = last_value(&values) else { unreachable!("an array is on top") };
                match token {
                    Token::Comma => states.push(state),
                    Token::Datum(v) => {
                        array.push(v);
                        states.push(state);
                    }
                    Token::LeftBrace | Token::LeftSquare => {
                        let brace = matches!(token, Token::LeftBrace);
                        let child = if brace { JValue::Map(JMap::new(MapKind::Hash)) } else { JValue::List(JList::array_list(Vec::new())) };
                        array.push(child.clone());
                        values.push(Slot::Value(child));
                        states.push(state);
                        states.push(if brace { State::ParsingObject } else { State::ParsingArray });
                    }
                    Token::RightSquare => {
                        if values.len() > 1 {
                            values.pop();
                        } else {
                            states.push(State::Done);
                        }
                    }
                    ref t => return Err(unexpected(&lexer, t)),
                }
            }
            State::ParsingObject => match token {
                Token::Comma => states.push(state),
                Token::Datum(JValue::Str(k)) => {
                    values.push(Slot::Key(k.to_string()));
                    states.push(state);
                    states.push(State::ParsingEntry);
                }
                Token::RightBrace => {
                    if values.len() > 1 {
                        values.pop();
                    } else {
                        states.push(State::Done);
                    }
                }
                ref t => return Err(unexpected(&lexer, t)),
            },
            State::ParsingEntry => {
                let put = |values: &mut Vec<Slot>, v: JValue| {
                    let Some(Slot::Key(key)) = values.pop() else { unreachable!("a key is on top") };
                    if let JValue::Map(parent) = last_value(values) {
                        parent.put(JValue::str(key), v);
                    }
                };
                match token {
                    Token::Colon => states.push(state),
                    Token::Datum(v) => put(&mut values, v),
                    Token::LeftBrace | Token::LeftSquare => {
                        let brace = matches!(token, Token::LeftBrace);
                        let child = if brace { JValue::Map(JMap::new(MapKind::Hash)) } else { JValue::List(JList::array_list(Vec::new())) };
                        put(&mut values, child.clone());
                        values.push(Slot::Value(child));
                        states.push(if brace { State::ParsingObject } else { State::ParsingArray });
                    }
                    ref t => return Err(unexpected(&lexer, t)),
                }
            }
        }
        if state == State::Done && end {
            break;
        }
    }
    Ok(match values.into_iter().next() {
        Some(Slot::Value(v)) => v,
        _ => JValue::Null,
    })
}

/// `JsonContent`: a JSON object or array whose nested objects and arrays come back wrapped.
pub struct JsonContent {
    value: JValue,
}

impl JsonContent {
    fn wrap(v: JValue) -> JValue {
        match v {
            JValue::Map(_) | JValue::List(_) => JValue::Object(Arc::new(JsonContent { value: v })),
            other => other,
        }
    }

    fn get_index(&self, index: i32) -> Result<JValue, JError> {
        Ok(match &self.value {
            JValue::List(l) => match usize::try_from(index).ok().and_then(|i| l.get(i)) {
                Some(v) => JsonContent::wrap(v),
                None => return Err(JError::new("java.lang.IndexOutOfBoundsException", format!("Index {index} out of bounds for length {}", l.len()))),
            },
            JValue::Map(m) => JsonContent::wrap(m.get_str(&index.to_string()).unwrap_or(JValue::Null)),
            _ => JValue::Null,
        })
    }

    fn get_key(&self, key: &str) -> Result<JValue, JError> {
        match &self.value {
            JValue::List(_) => match crate::uberspect::parse_integral(key, 32) {
                Ok(i) => self.get_index(i as i32),
                Err(_) => Ok(JValue::Null),
            },
            JValue::Map(m) => Ok(JsonContent::wrap(m.get_str(key).unwrap_or(JValue::Null))),
            _ => Ok(JValue::Null),
        }
    }

    fn keys(&self) -> Option<Vec<JValue>> {
        match &self.value {
            JValue::Map(m) => Some(m.entries().into_iter().map(|(k, _)| k).collect()),
            _ => None,
        }
    }

    /// `iterator()`: an object's keys, an array's (unwrapped) elements.
    fn iteration(&self) -> Option<Vec<JValue>> {
        match &self.value {
            JValue::Map(_) => self.keys(),
            JValue::List(l) => Some(l.items()),
            _ => None,
        }
    }

    fn size(&self) -> i32 {
        match &self.value {
            JValue::Map(m) => m.len() as i32,
            JValue::List(l) => l.len() as i32,
            _ => 0,
        }
    }

    fn java_string(&self) -> String {
        self.value.to_java_string()
    }
}

fn keys_value(keys: Option<Vec<JValue>>, kind: ListKind) -> JValue {
    keys.map(|k| JValue::List(JList::new(kind, k))).unwrap_or(JValue::Null)
}

fn json_content_def() -> Arc<ClassDef> {
    let def = ClassDef::new("org.apache.velocity.tools.generic.JsonContent")
        .method::<JsonContent>("get", &[JType::Int], Some(o()), |j, _, a| val(j.get_index(math::int_value(&a[0]))?))
        .method::<JsonContent>("get", &[s()], Some(o()), |j, _, a| val(j.get_key(a[0].as_str().unwrap_or("null"))?))
        .method::<JsonContent>("keys", &[], Some(c("java.util.Iterator")), |j, _, _| val(keys_value(j.keys(), ListKind::Iterator)))
        .method::<JsonContent>("keySet", &[], Some(c("java.util.Set")), |j, _, _| val(keys_value(j.keys(), ListKind::Set)))
        .method::<JsonContent>("iterator", &[], Some(c("java.util.Iterator")), |j, _, _| val(keys_value(j.iteration(), ListKind::Iterator)))
        .method::<JsonContent>("size", &[], Some(JType::Int), |j, _, _| val(JValue::Int(j.size())))
        .method::<JsonContent>("toString", &[], Some(s()), |j, _, _| val(JValue::str(j.java_string())))
        .method::<JsonContent>("isNull", &[], Some(JType::Boolean), |j, _, _| val(JValue::Bool(j.value.is_null())))
        .method::<JsonContent>("isObject", &[], Some(JType::Boolean), |j, _, _| val(JValue::Bool(matches!(j.value, JValue::Map(_)))))
        .method::<JsonContent>("isArray", &[], Some(JType::Boolean), |j, _, _| val(JValue::Bool(matches!(j.value, JValue::List(_)))))
        .method::<JsonContent>("getObject", &[], Some(c("com.github.cliftonlabs.json_simple.JsonObject")), |j, _, _| {
            val(if matches!(j.value, JValue::Map(_)) { j.value.clone() } else { JValue::Null })
        })
        .method::<JsonContent>("getArray", &[], Some(c("com.github.cliftonlabs.json_simple.JsonArray")), |j, _, _| {
            val(if matches!(j.value, JValue::List(_)) { j.value.clone() } else { JValue::Null })
        });
    object_methods::<JsonContent>(def).register()
}

tool_object!(JsonContent, json_content_def);

/// `$json`: request-scoped, holding the root the last `parse` / `read` / `fetch` produced.
pub struct JsonTool {
    safe: super::safe::SafeConfig,
    root: Mutex<Option<Arc<JsonContent>>>,
    me: Weak<JsonTool>,
}

/// The class name of `org.apache.velocity.tools.generic.JsonTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.JsonTool";

impl JsonTool {
    /// `new JsonTool()`: no JSON loaded yet.
    pub fn new() -> Arc<JsonTool> {
        Arc::new_cyclic(|me| JsonTool { safe: Default::default(), root: Mutex::new(None), me: me.clone() })
    }

    fn this(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.this()
    }

    /// `SafeConfig.configure(Map)`: nothing once the configuration is locked.
    pub fn configure(&self, params: &JMap) {
        super::safe::configure(self, params)
    }

    /// `parse(json)`.
    pub fn parse(&self, json: Option<&str>) -> JValue {
        if let Some(j) = json {
            self.init(j);
        }
        self.this()
    }

    /// `read(resource)`: a file or class path resource.
    pub fn read(&self, resource: Option<&str>) -> JValue {
        if let Some(content) = resource.and_then(|r| super::import::resource_string(r, CLASS)) {
            self.init(&content);
        }
        self.this()
    }

    /// `fetch(url)`.
    pub fn fetch(&self, url: Option<&str>) -> JValue {
        if let Some(content) = url.and_then(super::import::fetch) {
            self.init(&content);
        }
        self.this()
    }

    /// `get(key)`: a member of the root object (or an array's element by index), wrapped when it is itself
    /// an object or array.
    pub fn get(&self, key: &str) -> Result<JValue, JError> {
        match self.root() {
            Some(r) => r.get_key(key),
            None => Ok(JValue::Null),
        }
    }

    /// `get(index)`: the root array's element (wrapped when it is itself an object or array); `null` without a root.
    pub fn get_index(&self, index: i32) -> Result<JValue, JError> {
        match self.root() {
            Some(r) => r.get_index(index),
            None => Ok(JValue::Null),
        }
    }

    /// `initJSON`: a failed parse (or a bare datum) leaves no root.
    fn init(&self, json: &str) {
        let root = match deserialize(json) {
            Ok(v @ (JValue::Map(_) | JValue::List(_))) => Some(Arc::new(JsonContent { value: v })),
            _ => None,
        };
        *self.root.lock() = root;
    }

    fn root(&self) -> Option<Arc<JsonContent>> {
        self.root.lock().clone()
    }

    fn java_string(&self) -> String {
        // `root == null ? null : root.toString()`: a null `toString()` renders as the reference itself.
        self.root().map(|r| r.java_string()).unwrap_or_else(|| "null".into())
    }
}

impl super::safe::Tool for JsonTool {
    fn safe(&self) -> &super::safe::SafeConfig {
        &self.safe
    }

    fn configure_values(&self, values: &super::values::ValueParser) {
        if let Some(resource) = values.get_string(super::import::RESOURCE_KEY) {
            self.read(Some(&resource));
        } else if let Some(url) = values.get_string(super::import::URL_KEY) {
            self.fetch(Some(&url));
        }
    }
}

fn npe() -> JError {
    JError::without_message("java.lang.NullPointerException")
}

fn json_tool_def() -> Arc<ClassDef> {
    let def = ClassDef::new(CLASS)
        .extends(&[super::import::IMPORT_SUPPORT, super::SAFE_CONFIG])
        .implements(&["java.lang.Iterable", "java.io.Serializable"])
        .method::<JsonTool>("parse", &[s()], Some(c(CLASS)), |t, _, a| val(t.parse(a[0].as_str())))
        .method::<JsonTool>("read", &[s()], Some(c(CLASS)), |t, _, a| val(t.read(a[0].as_str())))
        .method::<JsonTool>("fetch", &[s()], Some(c(CLASS)), |t, _, a| val(t.fetch(a[0].as_str())))
        .method::<JsonTool>("root", &[], Some(o()), |t, _, _| val(t.root().map(|r| JValue::Object(r as Arc<dyn JObject>)).unwrap_or(JValue::Null)))
        .method::<JsonTool>("get", &[JType::Int], Some(o()), |t, _, a| match t.root() {
            Some(r) => val(r.get_index(math::int_value(&a[0]))?),
            None => val(JValue::Null),
        })
        .method::<JsonTool>("get", &[s()], Some(o()), |t, _, a| match t.root() {
            Some(r) => val(r.get_key(a[0].as_str().unwrap_or("null"))?),
            None => val(JValue::Null),
        })
        .method::<JsonTool>("keys", &[], Some(c("java.util.Iterator")), |t, _, _| val(t.root().map(|r| keys_value(r.keys(), ListKind::Iterator)).unwrap_or(JValue::Null)))
        .method::<JsonTool>("keySet", &[], Some(c("java.util.Set")), |t, _, _| val(t.root().map(|r| keys_value(r.keys(), ListKind::Set)).unwrap_or(JValue::Null)))
        .method::<JsonTool>("iterator", &[], Some(c("java.util.Iterator")), |t, _, _| val(t.root().map(|r| keys_value(r.iteration(), ListKind::Iterator)).unwrap_or(JValue::Null)))
        .method::<JsonTool>("size", &[], Some(JType::Int), |t, _, _| match t.root() {
            Some(r) => val(JValue::Int(r.size())),
            None => Err(npe()),
        })
        .method::<JsonTool>("toString", &[], Some(s()), |t, _, _| val(t.root().map(|r| JValue::str(r.java_string())).unwrap_or(JValue::Null)));
    let def = super::import::import_support_methods::<JsonTool>(def);
    object_methods::<JsonTool>(super::safe::safe_config_methods::<JsonTool>(def)).register()
}

tool_object!(JsonTool, json_tool_def);

pub(crate) fn class() -> super::classes::JavaClass {
    super::classes::JavaClass::tool(json_tool_def(), || JsonTool::new().value())
        .extends(Some(super::import::IMPORT_SUPPORT))
        .annotated(super::classes::Annotation::default_key("json"))
        .annotated(super::classes::Annotation::invalid_scope(&[super::scope::SESSION]))
}

impl std::fmt::Debug for JsonContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.java_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_simple_leniency() {
        assert_eq!(deserialize("{\"b\":1,\"a\":[1,2.50,1e3,-0,true,null,\"x\\u00e9\"], \"c\":{}}").unwrap().to_java_string(), "{a=[1, 2.50, 1E+3, 0, true, null, xé], b=1, c={}}");
        assert_eq!(deserialize("[1,]").unwrap().to_java_string(), "[1]");
        assert_eq!(deserialize("[1 2]").unwrap().to_java_string(), "[1, 2]");
        assert_eq!(deserialize("{\"a\" 1}").unwrap().to_java_string(), "{a=1}");
        assert_eq!(deserialize("  3 ").unwrap().to_java_string(), "3");
        assert_eq!(deserialize("{\"a\":}").unwrap_err().message.unwrap(), "The unexpected token RIGHT_BRACE(}) was found at position 5. Fix the parsable string and try again.");
        assert!(deserialize("[1] 2").is_err());
        assert!(deserialize("-").is_err());
    }
}
