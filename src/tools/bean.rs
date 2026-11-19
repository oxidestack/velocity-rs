//! Commons BeanUtils' `PropertyUtils.getProperty(bean, name)`: nested (`a.b`), indexed (`a[0]`) and mapped
//! (`a(key)`) properties, maps read by key.

use java_compat::lang::{JError, JValue};

use crate::introspect;

fn illegal(message: String) -> JError {
    JError::new("java.lang.IllegalArgumentException", message)
}

/// One `DefaultResolver.next` step: the property name, and an index or map key.
enum Token {
    Simple(String),
    Indexed(String, String),
    Mapped(String, String),
}

fn split(expr: &str) -> (String, &str) {
    let (mut indexed, mut mapped) = (false, false);
    for (i, c) in expr.char_indices() {
        if indexed {
            if c == ']' {
                indexed = false;
            }
        } else if mapped {
            if c == ')' {
                mapped = false;
            }
        } else {
            match c {
                '.' => return (expr[..i].to_string(), &expr[i + 1..]),
                '[' => indexed = true,
                '(' => mapped = true,
                _ => {}
            }
        }
    }
    (expr.to_string(), "")
}

fn token(t: &str) -> Token {
    if let (Some(open), true) = (t.find('('), t.ends_with(')')) {
        return Token::Mapped(t[..open].to_string(), t[open + 1..t.len() - 1].to_string());
    }
    if let (Some(open), true) = (t.find('['), t.ends_with(']')) {
        return Token::Indexed(t[..open].to_string(), t[open + 1..t.len() - 1].to_string());
    }
    Token::Simple(t.to_string())
}

fn no_such(name: &str, bean: &JValue) -> JError {
    JError::new("java.lang.NoSuchMethodException", format!("Unknown property '{name}' on class '{}'", bean_class(bean)))
}

fn bean_class(bean: &JValue) -> String {
    format!("class {}", bean.class_name())
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn found(r: Option<Result<Option<JValue>, JError>>) -> Option<Result<JValue, JError>> {
    r.map(|r| r.map(|v| v.unwrap_or(JValue::Null)).map_err(|e| JError::new("java.lang.reflect.InvocationTargetException", e.to_string())))
}

/// `getSimpleProperty`: the JavaBeans read method.
fn simple(bean: &JValue, name: &str) -> Result<JValue, JError> {
    if let JValue::Map(m) = bean {
        return Ok(m.get_str(name).unwrap_or(JValue::Null));
    }
    let cap = capitalize(name);
    if let Some(r) = found(introspect::call(bean, &format!("get{cap}"), &[])) {
        return r;
    }
    if let Some(r) = found(introspect::call(bean, &format!("is{cap}"), &[])) {
        if let Ok(JValue::Bool(_)) = r {
            return r;
        }
    }
    Err(no_such(name, bean))
}

fn indexed(bean: &JValue, name: &str, index: &str) -> Result<JValue, JError> {
    let i: i64 = index.trim().parse().map_err(|_| illegal(format!("Invalid indexed property '{name}[{index}]'")))?;
    let value = if name.is_empty() { bean.clone() } else { simple(bean, name)? };
    let items = match &value {
        JValue::List(l) => l.items(),
        JValue::Array(a) => a.items(),
        _ => return Err(illegal(format!("Property '{name}' is not indexed on bean class '{}'", bean_class(bean)))),
    };
    usize::try_from(i).ok().and_then(|i| items.get(i).cloned()).ok_or_else(|| {
        JError::new("java.lang.IndexOutOfBoundsException", format!("Index {i} out of bounds for length {}", items.len()))
    })
}

fn mapped(bean: &JValue, name: &str, key: &str) -> Result<JValue, JError> {
    let value = if name.is_empty() { bean.clone() } else { simple(bean, name)? };
    match &value {
        JValue::Map(m) => Ok(m.get_str(key).unwrap_or(JValue::Null)),
        _ => match found(introspect::call(bean, &format!("get{}", capitalize(name)), &[JValue::str(key)])) {
            Some(r) => r,
            None => Err(JError::new("java.lang.NoSuchMethodException", format!("Property '{name}' has no mapped getter method on bean class '{}'", bean_class(bean)))),
        },
    }
}

/// `PropertyUtils.getProperty(bean, name)`.
pub fn get_property(bean: &JValue, name: &str) -> Result<JValue, JError> {
    if bean.is_null() {
        return Err(illegal("No bean specified".into()));
    }
    let mut bean = bean.clone();
    let mut rest = name;
    loop {
        let (next, remaining) = split(rest);
        if remaining.is_empty() {
            break;
        }
        let value = step(&bean, &next, name)?;
        if value.is_null() {
            return Err(JError::new(
                "org.apache.commons.beanutils.NestedNullException",
                format!("Null property value for '{}' on bean class '{}'", &name[..name.len() - remaining.len() - 1], bean_class(&bean)),
            ));
        }
        bean = value;
        rest = remaining;
    }
    step(&bean, rest, name)
}

fn step(bean: &JValue, t: &str, whole: &str) -> Result<JValue, JError> {
    match (bean, token(t)) {
        (JValue::Map(_), Token::Indexed(..) | Token::Mapped(..)) => Err(illegal(format!("Indexed or mapped properties are not supported on objects of type Map: {whole}"))),
        (_, Token::Simple(n)) => simple(bean, &n),
        (_, Token::Indexed(n, i)) => indexed(bean, &n, &i),
        (_, Token::Mapped(n, k)) => mapped(bean, &n, &k),
    }
}
