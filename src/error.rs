//! Velocity's exceptions (`org.apache.velocity.exception`) and the `StopCommand` that unwinds `#break`/`#stop`.

use std::fmt;

use java_compat::lang::JError;

/// Who catches a `StopCommand`: a directive instance, a merged template, or the `RuntimeInstance` (`evaluate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// A directive node (`#foreach`, `#evaluate`, `#parse`, a macro call site, `#define`).
    Node(usize),
    /// The `Block` half of a block macro call (`#@name`), whose scope is named after the macro.
    BlockMacro(usize),
    /// A `Template` being merged, by identity.
    Template(usize),
    /// `RuntimeInstance.evaluate` / `render`.
    Runtime,
}

/// Where a `StopCommand` goes (`#break`, `#stop`).
#[derive(Debug, Clone, PartialEq)]
pub enum StopTarget {
    /// `#break`: the first catcher (loop, block, macro call, `#evaluate`, `#parse`, template).
    Nearest,
    /// `#break($scope)`: that scope's owner.
    Owner(Owner),
    /// `#stop`: only a template or the runtime instance; the message, when one was given.
    Top(Option<String>),
}

impl StopTarget {
    /// `StopCommand.isFor(that)`.
    pub fn is_for(&self, catcher: Owner) -> bool {
        match self {
            StopTarget::Nearest => true,
            StopTarget::Owner(o) => *o == catcher,
            StopTarget::Top(_) => matches!(catcher, Owner::Template(_) | Owner::Runtime),
        }
    }
}

/// The exception class.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorKind {
    /// `ParseErrorException` (from a `ParseException` or `TemplateInitException`).
    ParseError,
    /// `TemplateInitException`, before it is wrapped.
    TemplateInit,
    /// `MethodInvocationException`: a method, getter or setter threw.
    MethodInvocation,
    /// `ResourceNotFoundException`.
    ResourceNotFound,
    /// `VelocityException` itself.
    Velocity,
    /// `MathException` (`runtime.strict_math`).
    Math,
    /// `MacroOverflowException`.
    MacroOverflow,
    /// A `StopCommand` that reached no directive it was for.
    Stop(StopTarget),
    /// A Java `RuntimeException` that Velocity lets through unwrapped (thrown by user code or the JDK).
    Runtime,
}

/// A thrown exception: its class, `getMessage()` pieces and cause, boxed so that every `Result` carrying one stays
/// one pointer wide on its error side. It dereferences to its [`VelocityErrorData`], so the fields read as its own.
#[derive(Clone, PartialEq)]
pub struct VelocityError(Box<VelocityErrorData>);

/// The fields of a [`VelocityError`].
#[derive(Debug, Clone, PartialEq)]
pub struct VelocityErrorData {
    /// Which exception it is.
    pub kind: ErrorKind,
    /// The message without the location suffix `MethodInvocationException.getMessage()` appends.
    pub message: String,
    /// The template it was thrown in (`getTemplateName()`), when known.
    pub template: Option<String>,
    /// `getLineNumber()`, 0 when unknown.
    pub line: i32,
    /// `getColumnNumber()`, 0 when unknown.
    pub column: i32,
    /// `ParseErrorException.getInvalidSyntax()`.
    pub invalid_syntax: Option<String>,
    /// `MethodInvocationException.getMethodName()` / `getReferenceName()`.
    pub method_name: Option<String>,
    /// `MethodInvocationException.getReferenceName()`.
    pub reference_name: Option<String>,
    /// `getCause()`: the Java exception wrapped, if any.
    pub cause: Option<JError>,
}

impl VelocityError {
    /// An exception of `kind` with this message and no location.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> VelocityError {
        VelocityError(Box::new(VelocityErrorData {
            kind,
            message: message.into(),
            template: None,
            line: 0,
            column: 0,
            invalid_syntax: None,
            method_name: None,
            reference_name: None,
            cause: None,
        }))
    }

    /// The fields, out of the box, to move one out.
    pub fn into_data(self) -> VelocityErrorData {
        *self.0
    }

    /// A `ParseErrorException`.
    pub fn parse(message: impl Into<String>) -> VelocityError {
        VelocityError::new(ErrorKind::ParseError, message)
    }

    /// A `VelocityException`.
    pub fn velocity(message: impl Into<String>) -> VelocityError {
        VelocityError::new(ErrorKind::Velocity, message)
    }

    /// A `ResourceNotFoundException`.
    pub fn resource_not_found(message: impl Into<String>) -> VelocityError {
        VelocityError::new(ErrorKind::ResourceNotFound, message)
    }

    /// A `StopCommand` for `target` (`#stop`, `#break`), with Velocity's message.
    pub fn stop(target: StopTarget) -> VelocityError {
        let message = match &target {
            StopTarget::Top(Some(m)) => format!("StopCommand: {m}"),
            StopTarget::Top(None) => "StopCommand: StopCommand to exit merging".into(),
            _ => "StopCommand: null".into(),
        };
        VelocityError::new(ErrorKind::Stop(target), message)
    }

    /// A `StopCommand` this catcher takes.
    pub fn stops(&self, catcher: Owner) -> bool {
        self.stop_target().is_some_and(|t| t.is_for(catcher))
    }

    /// A Java exception passing through as itself.
    pub fn runtime(e: JError) -> VelocityError {
        let mut v = VelocityError::new(ErrorKind::Runtime, e.message.clone().unwrap_or_default());
        v.cause = Some(e);
        v
    }

    /// `MethodInvocationException(message, cause, methodName, template, line, column)`.
    pub fn method_invocation(message: impl Into<String>, cause: JError, method: &str, template: Option<&str>, line: i32, column: i32) -> VelocityError {
        let mut v = VelocityError::new(ErrorKind::MethodInvocation, message);
        v.cause = Some(cause);
        v.method_name = Some(method.to_string());
        v.template = template.map(str::to_string);
        v.line = line;
        v.column = column;
        v
    }

    /// The same exception located at `line` and `column` of `template`.
    pub fn at(mut self, template: Option<&str>, line: i32, column: i32) -> VelocityError {
        self.template = template.map(str::to_string);
        self.line = line;
        self.column = column;
        self
    }

    /// The same exception with `cause` as `getCause()`.
    pub fn with_cause(mut self, cause: JError) -> VelocityError {
        self.cause = Some(cause);
        self
    }

    /// Whether it is a `StopCommand`.
    pub fn is_stop(&self) -> bool {
        matches!(self.kind, ErrorKind::Stop(_))
    }

    /// What a `StopCommand` stops; `None` for any other exception.
    pub fn stop_target(&self) -> Option<&StopTarget> {
        match &self.kind {
            ErrorKind::Stop(t) => Some(t),
            _ => None,
        }
    }

    /// Whether it is a `ParseErrorException` or a `TemplateInitException`.
    pub fn is_parse(&self) -> bool {
        matches!(self.kind, ErrorKind::ParseError | ErrorKind::TemplateInit)
    }

    /// `getClass().getName()`.
    pub fn class_name(&self) -> String {
        match &self.kind {
            ErrorKind::ParseError => "org.apache.velocity.exception.ParseErrorException".into(),
            ErrorKind::TemplateInit => "org.apache.velocity.exception.TemplateInitException".into(),
            ErrorKind::MethodInvocation => "org.apache.velocity.exception.MethodInvocationException".into(),
            ErrorKind::ResourceNotFound => "org.apache.velocity.exception.ResourceNotFoundException".into(),
            ErrorKind::Velocity => "org.apache.velocity.exception.VelocityException".into(),
            ErrorKind::Math => "org.apache.velocity.exception.MathException".into(),
            ErrorKind::MacroOverflow => "org.apache.velocity.exception.MacroOverflowException".into(),
            ErrorKind::Stop(_) => "org.apache.velocity.runtime.directive.StopCommand".into(),
            ErrorKind::Runtime => self.cause.as_ref().map(|c| c.class.clone()).unwrap_or_else(|| "java.lang.RuntimeException".into()),
        }
    }

    /// `getClass().getSimpleName()`.
    pub fn simple_class_name(&self) -> String {
        self.class_name().rsplit(['.', '$']).next().unwrap_or_default().to_string()
    }

    /// `Throwable.toString()`: the class, then `: message` when there is one.
    pub fn java_to_string(&self) -> String {
        // A Java exception passing through keeps its own `toString()` (no message: the class alone).
        if let (ErrorKind::Runtime, Some(c)) = (&self.kind, &self.cause) {
            return c.to_string();
        }
        format!("{}: {}", self.class_name(), self.message())
    }

    /// `getMessage()`.
    pub fn message(&self) -> String {
        match self.kind {
            ErrorKind::MethodInvocation => format!("{} at {}", self.message, format_file_string(self.template.as_deref(), self.line, self.column)),
            _ => self.message.clone(),
        }
    }

    /// `instanceof`: this exception's class is `class` or a subclass of it.
    pub fn is_a(&self, class: &str) -> bool {
        let velocity = matches!(
            self.kind,
            ErrorKind::ParseError | ErrorKind::TemplateInit | ErrorKind::MethodInvocation | ErrorKind::ResourceNotFound | ErrorKind::Velocity | ErrorKind::Math | ErrorKind::MacroOverflow
        );
        let name = self.class_name();
        name == class
            || (velocity && matches!(class, "org.apache.velocity.exception.VelocityException" | "java.lang.RuntimeException" | "java.lang.Exception" | "java.lang.Throwable"))
            || matches!(class, "java.lang.Exception" | "java.lang.Throwable")
            || (class == "java.lang.RuntimeException" && self.kind == ErrorKind::Runtime)
    }
}

/// JavaCC's `ParseException.add_escapes` / `TokenMgrError.addEscapes`.
pub fn add_escapes(s: &str) -> String {
    add_escapes_utf16(&s.encode_utf16().collect::<Vec<_>>())
}

/// [`add_escapes`] over UTF-16 units, which may be a lone surrogate.
pub fn add_escapes_utf16(units: &[u16]) -> String {
    let mut out = String::new();
    for &ch in units {
        match ch {
            0 => {}
            0x08 => out.push_str("\\b"),
            0x09 => out.push_str("\\t"),
            0x0a => out.push_str("\\n"),
            0x0c => out.push_str("\\f"),
            0x0d => out.push_str("\\r"),
            0x22 => out.push_str("\\\""),
            0x27 => out.push_str("\\'"),
            0x5c => out.push_str("\\\\"),
            c if !(0x20..=0x7e).contains(&c) => out.push_str(&format!("\\u{c:04x}")),
            c => out.push(c as u8 as char),
        }
    }
    out
}

/// Placeholder a parser error's message holds for the location, filled in once the template's name is known.
pub const LOCATION: &str = "\u{0}LOC\u{0}";

impl VelocityError {
    /// A `ParseException` at a token: its message with [`LOCATION`] for `template[line, column]`.
    pub fn parse_at(message: String, line: i32, column: i32) -> VelocityError {
        let mut e = VelocityError::parse(message);
        e.line = line;
        e.column = column;
        e
    }

    /// `ParseErrorException(pex, templateName)`: fills in the template and the location in the message.
    pub fn in_template(mut self, name: &str) -> VelocityError {
        if self.template.is_none() {
            self.template = Some(name.to_string());
        }
        if self.message.contains(LOCATION) {
            let loc = format_file_string(self.template.as_deref(), self.line, self.column);
            self.message = self.message.replace(LOCATION, &loc);
        }
        self
    }
}

/// How the `ParseException` made of a `TokenMgrError` begins (`StandardParser.parse`).
pub const LEXICAL_ERROR: &str = "Lexical error: org.apache.velocity.runtime.parser.TokenMgrError: ";

impl VelocityError {
    /// `ParseErrorException(pex, templateName)` for the plain `ParseException` a lexical error is: Velocity matches
    /// `Lexical error.*TokenMgrError.*line (\d+),.*column (\d+)\.(.*)` against its message and, when it matches,
    /// reports `Lexical error, <rest> at <template>[line L, column C]` (`*unset*` without a template name). The
    /// pattern's greedy wildcards take the last `line`/`column` pair in the text, which may lie in the echoed prefix.
    pub(crate) fn lexical_in(mut self, name: Option<&str>) -> VelocityError {
        if self.kind != ErrorKind::ParseError || !self.message.starts_with(LEXICAL_ERROR) {
            return self;
        }
        let template = name.unwrap_or("*unset*").to_string();
        if let Some((line, column, rest)) = match_lexical_error(&self.message) {
            self.message = format!("Lexical error, {rest} at {}", format_file_string(Some(&template), line, column));
            self.line = line;
            self.column = column;
        }
        self.template = Some(template);
        self
    }
}

/// `line (\d+),.*column (\d+)\.(.*)` after `Lexical error.*TokenMgrError.*`, the wildcards greedy: the line number,
/// the column number and the rest of the text.
fn match_lexical_error(message: &str) -> Option<(i32, i32, &str)> {
    let after_class = message.find("TokenMgrError")? + "TokenMgrError".len();
    // The pattern needs digits that `Integer.parseInt` reads; longer runs throw in Java and are not reproduced.
    let number = |s: &str, end: char| -> Option<(i32, usize)> {
        let digits = s.bytes().take_while(u8::is_ascii_digit).count();
        (digits > 0 && s[digits..].starts_with(end)).then(|| s[..digits].parse().ok().map(|n| (n, digits + 1))).flatten()
    };
    let lines: Vec<usize> = message.match_indices("line ").map(|(i, _)| i).filter(|&i| i >= after_class).collect();
    for &l in lines.iter().rev() {
        let Some((line, used)) = number(&message[l + 5..], ',') else { continue };
        let from = l + 5 + used;
        let columns: Vec<usize> = message[from..].match_indices("column ").map(|(i, _)| from + i).collect();
        for &c in columns.iter().rev() {
            if let Some((column, used)) = number(&message[c + 7..], '.') {
                return Some((line, column, &message[c + 7 + used..]));
            }
        }
    }
    None
}

/// `StringUtils.formatFileString`: `template[line L, column C]`, `<unknown template>` for an empty name.
pub fn format_file_string(template: Option<&str>, line: i32, column: i32) -> String {
    let template = template.filter(|t| !t.is_empty()).unwrap_or("<unknown template>");
    format!("{template}[line {line}, column {column}]")
}

impl std::ops::Deref for VelocityError {
    type Target = VelocityErrorData;

    fn deref(&self) -> &VelocityErrorData {
        &self.0
    }
}

impl std::ops::DerefMut for VelocityError {
    fn deref_mut(&mut self) -> &mut VelocityErrorData {
        &mut self.0
    }
}

/// The fields under the type's own name, as before the box.
impl fmt::Debug for VelocityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = &*self.0;
        f.debug_struct("VelocityError")
            .field("kind", &d.kind)
            .field("message", &d.message)
            .field("template", &d.template)
            .field("line", &d.line)
            .field("column", &d.column)
            .field("invalid_syntax", &d.invalid_syntax)
            .field("method_name", &d.method_name)
            .field("reference_name", &d.reference_name)
            .field("cause", &d.cause)
            .finish()
    }
}

impl fmt::Display for VelocityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for VelocityError {}
