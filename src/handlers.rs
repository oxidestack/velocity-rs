//! `org.apache.velocity.app.event.implement`: the event handlers Velocity ships.

use std::collections::HashMap;
use std::sync::Arc;

use java_compat::lang::{JError, JValue};
use parking_lot::Mutex;

use crate::events::{Handler, HandlerFactory, IncludeEventHandler, Info, InvalidReferenceEventHandler, MethodExceptionEventHandler, ReferenceInsertionEventHandler};
use crate::{Context, Engine, VelocityError};

type R<T> = Result<T, VelocityError>;

/// `EscapeReference`: escapes every inserted value, or those whose reference matches `eventhandler.escape.*.match`
/// (read once, when the handler gets the runtime; an invalid expression is logged and escapes everything).
pub struct EscapeReference {
    escape: fn(&str) -> String,
    match_attribute: &'static str,
    pattern: std::sync::OnceLock<Option<String>>,
}

impl EscapeReference {
    fn new(escape: fn(&str) -> String, match_attribute: &'static str) -> EscapeReference {
        EscapeReference { escape, match_attribute, pattern: std::sync::OnceLock::new() }
    }

    /// `new EscapeHtmlReference()`.
    pub fn html() -> EscapeReference {
        EscapeReference::new(java_compat::escape_html4, "eventhandler.escape.html.match")
    }

    /// `new EscapeJavaScriptReference()`.
    pub fn javascript() -> EscapeReference {
        EscapeReference::new(java_compat::escape_ecma_script, "eventhandler.escape.javascript.match")
    }

    /// `new EscapeXmlReference()`.
    pub fn xml() -> EscapeReference {
        EscapeReference::new(java_compat::escape_xml10, "eventhandler.escape.xml.match")
    }

    /// `new EscapeSqlReference()`.
    pub fn sql() -> EscapeReference {
        EscapeReference::new(escape_sql, "eventhandler.escape.sql.match")
    }

    /// `escape(text)`.
    pub fn escape(&self, text: &str) -> String {
        (self.escape)(text)
    }

    /// `setRuntimeServices`: the trimmed match expression, none when empty or invalid.
    fn pattern(&self, rsvc: &Engine) -> Option<String> {
        self.pattern
            .get_or_init(|| {
                let p = rsvc.config.props.string(self.match_attribute).map(|p| java_compat::lang::java_trim(&p).to_string()).filter(|p| !p.is_empty())?;
                match java_compat::methods::java_regex(&format!("^(?:{p})$")) {
                    Ok(_) => Some(p),
                    Err(e) => {
                        use crate::log::LogExt;
                        rsvc.log_for("event").thrown(
                            crate::log::Level::Error,
                            || format!("Invalid regular expression '{p}'. No escaping will be performed."),
                            &crate::log::Throwable { class: "java.util.regex.PatternSyntaxException".into(), message: Some(e.to_string()) },
                        );
                        None
                    }
                }
            })
            .clone()
    }
}

impl ReferenceInsertionEventHandler for EscapeReference {
    fn reference_insert(&self, rsvc: &Engine, _: &Context, reference: &str, value: JValue) -> R<JValue> {
        if value.is_null() {
            return Ok(value);
        }
        let escape = match self.pattern(rsvc) {
            None => true,
            Some(p) => java_compat::methods::java_regex(&format!("^(?:{p})$")).is_ok_and(|re| re.is_match(reference).unwrap_or(false)),
        };
        Ok(if escape { JValue::str((self.escape)(&value.to_java_string())) } else { value })
    }
}

fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}

fn escaper(make: fn() -> EscapeReference) -> HandlerFactory {
    Arc::new(move || Handler { reference_insertion: Some(Arc::new(make())), ..Default::default() })
}

/// `IncludeNotFound`: a missing resource becomes `eventhandler.include.notfound` (`notfound.vm`), with
/// `$missingResource` set.
pub struct IncludeNotFound;

impl IncludeEventHandler for IncludeNotFound {
    fn include_event(&self, rsvc: &Engine, ctx: &mut Context, path: Option<&str>, _: Option<&str>, _: &str) -> R<Option<String>> {
        let path = path.unwrap_or_default();
        if rsvc.loader_name_for_resource(path).is_some() {
            return Ok(Some(path.to_string()));
        }
        ctx.put("missingResource", JValue::str(path));
        let notfound = java_compat::lang::java_trim(&rsvc.config.props.string_or("eventhandler.include.notfound", "notfound.vm")).to_string();
        Ok(rsvc.loader_name_for_resource(&notfound).is_some().then_some(notfound))
    }
}

/// `IncludeRelativePath`: relative names resolve against the current template's directory.
pub struct IncludeRelativePath;

impl IncludeEventHandler for IncludeRelativePath {
    fn include_event(&self, _: &Engine, _: &mut Context, path: Option<&str>, current: Option<&str>, _: &str) -> R<Option<String>> {
        let path = path.unwrap_or_default();
        if path.starts_with('/') || path.starts_with('\\') {
            return Ok(Some(path.to_string()));
        }
        let current = current.unwrap_or_default();
        Ok(Some(match current.rfind(['/', '\\']) {
            None => path.to_string(),
            Some(i) => format!("{}/{path}", &current[..i]),
        }))
    }
}

/// `PrintExceptions`: renders the exception instead of throwing it.
pub struct PrintExceptions;

impl MethodExceptionEventHandler for PrintExceptions {
    fn method_exception(&self, rsvc: &Engine, _: &Context, class: &str, method: &str, e: &JError, info: &Info) -> Result<JValue, Option<VelocityError>> {
        let props = &rsvc.config.props;
        let mut s = format!("Exception while executing method class {class}.{method}: {}: {}", e.class, e.message.as_deref().unwrap_or("null"));
        if props.boolean("eventhandler.methodexception.templateinfo", false) {
            s.push_str(&format!(" at {} (line {}, column {})", info.template, info.line, info.column));
        }
        if props.boolean("eventhandler.methodexception.stacktrace", false) {
            s.push('\n');
            s.push_str(&e.print_stack_trace());
        }
        Ok(JValue::str(s))
    }
}

/// `InvalidReferenceInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct InvalidReferenceInfo {
    /// `getInvalidReference()`: the reference as the template wrote it.
    pub reference: String,
    /// Where it is: the template, line and column.
    pub info: Info,
}

impl InvalidReferenceInfo {
    /// `toString()`: `template [line L, column C]: reference`.
    pub fn java_string(&self) -> String {
        format!("{} [line {}, column {}]: {}", self.info.template, self.info.line, self.info.column, self.reference)
    }
}

/// `ReportInvalidReferences`: records every invalid reference, or throws on the first one when
/// `event_handler.invalid_references.exception` is set.
#[derive(Default)]
pub struct ReportInvalidReferences {
    /// `getInvalidReferences()`, in the order met.
    pub invalid: Mutex<Vec<InvalidReferenceInfo>>,
}

impl ReportInvalidReferences {
    fn report(&self, rsvc: &Engine, reference: &str, info: &Info) -> R<()> {
        let i = InvalidReferenceInfo { reference: reference.to_string(), info: info.clone() };
        self.invalid.lock().push(i);
        let props = &rsvc.config.props;
        let stop = match props.get("eventhandler.invalidreference.exception") {
            Some(_) => props.boolean("eventhandler.invalidreference.exception", false),
            None => props.boolean("event_handler.invalid_references.exception", false),
        };
        if stop {
            let mut e = VelocityError::parse("Error in page - invalid reference.  ");
            e.template = Some(info.template.clone());
            e.line = info.line;
            e.column = info.column;
            e.invalid_syntax = Some(reference.to_string());
            return Err(e);
        }
        Ok(())
    }
}

impl InvalidReferenceEventHandler for ReportInvalidReferences {
    fn invalid_get_method(&self, rsvc: &Engine, _: &Context, reference: &str, _: &JValue, _: Option<&str>, info: &Info) -> R<JValue> {
        self.report(rsvc, reference, info)?;
        Ok(JValue::Null)
    }

    fn invalid_set_method(&self, rsvc: &Engine, _: &Context, left: &str, _: &str, info: &Info) -> R<bool> {
        self.report(rsvc, left, info)?;
        Ok(false)
    }

    fn invalid_method(&self, rsvc: &Engine, _: &Context, reference: &str, object: &JValue, method: &str, info: &Info) -> R<JValue> {
        let reference = if reference.is_empty() { format!("{}.{method}", object.class_name()) } else { reference.to_string() };
        self.report(rsvc, &reference, info)?;
        Ok(JValue::Null)
    }
}

const PACKAGE: &str = "org.apache.velocity.app.event.implement.";

pub(crate) fn builtin() -> HashMap<String, HandlerFactory> {
    let mut m: HashMap<String, HandlerFactory> = HashMap::new();
    let mut add = |name: &str, f: HandlerFactory| {
        m.insert(format!("{PACKAGE}{name}"), f);
    };
    add("EscapeHtmlReference", escaper(EscapeReference::html));
    add("EscapeJavaScriptReference", escaper(EscapeReference::javascript));
    add("EscapeXmlReference", escaper(EscapeReference::xml));
    add("EscapeSqlReference", escaper(EscapeReference::sql));
    add("IncludeNotFound", Arc::new(|| Handler { include: Some(Arc::new(IncludeNotFound)), ..Default::default() }));
    add("IncludeRelativePath", Arc::new(|| Handler { include: Some(Arc::new(IncludeRelativePath)), ..Default::default() }));
    add("PrintExceptions", Arc::new(|| Handler { method_exception: Some(Arc::new(PrintExceptions)), ..Default::default() }));
    add("ReportInvalidReferences", Arc::new(|| Handler { invalid_reference: Some(Arc::new(ReportInvalidReferences::default())), ..Default::default() }));
    m
}
