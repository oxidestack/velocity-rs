//! `org.apache.velocity.app.event`: the event handler interfaces, `EventCartridge` and `EventHandlerUtil`
//! (the application's cartridge first, then the one attached to the context).

use std::sync::Arc;

use java_compat::lang::{JError, JValue};
use parking_lot::RwLock;

use crate::{Context, Engine, VelocityError};

type R<T> = Result<T, VelocityError>;

/// `org.apache.velocity.util.introspection.Info`: where an event happened.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Info {
    /// The template name, empty when unknown.
    pub template: String,
    /// The line, 1-based.
    pub line: i32,
    /// The column, 1-based.
    pub column: i32,
}

impl Info {
    /// `new Info(template, line, column)`; a missing template name becomes the empty string.
    pub fn new(template: Option<&str>, line: i32, column: i32) -> Info {
        Info { template: template.unwrap_or_default().to_string(), line, column }
    }

    /// `Info.toString()`: `template[line L, column C]` (`StringUtils.formatFileString`).
    pub fn java_string(&self) -> String {
        crate::error::format_file_string(Some(&self.template), self.line, self.column)
    }
}

/// Handlers get the runtime (`RuntimeServicesAware`) and may throw.
pub trait ReferenceInsertionEventHandler: Send + Sync {
    /// The value to render for `reference` (its literal) instead of `value`.
    fn reference_insert(&self, rsvc: &Engine, context: &Context, reference: &str, value: JValue) -> R<JValue>;
}

/// `MethodExceptionEventHandler`: turns an exception thrown by a method call into a rendered value.
pub trait MethodExceptionEventHandler: Send + Sync {
    /// A replacement value for a method (or getter) that threw `e`; `Err(None)` rethrows `e`.
    fn method_exception(&self, rsvc: &Engine, context: &Context, class: &str, method: &str, e: &JError, info: &Info) -> Result<JValue, Option<VelocityError>>;
}

/// `IncludeEventHandler`: rewrites or blocks the resource an `#include` or `#parse` loads.
pub trait IncludeEventHandler: Send + Sync {
    /// The resource `#include` / `#parse` should load instead, or `None` to block it.
    fn include_event(&self, rsvc: &Engine, context: &mut Context, include_resource: Option<&str>, current_resource: Option<&str>, directive: &str) -> R<Option<String>>;
}

/// `InvalidReferenceEventHandler`: called for references, `#set` targets and method calls that cannot be resolved.
pub trait InvalidReferenceEventHandler: Send + Sync {
    /// A value for a reference that resolved to null (`property` is the failing identifier, if any).
    fn invalid_get_method(&self, rsvc: &Engine, context: &Context, reference: &str, object: &JValue, property: Option<&str>, info: &Info) -> R<JValue>;

    /// `true` to stop further handlers after an `#set` of an invalid reference.
    fn invalid_set_method(&self, rsvc: &Engine, context: &Context, left: &str, right: &str, info: &Info) -> R<bool>;

    /// A value for a method call `$reference.method(...)` that does not exist on `object`; null leaves the next handler
    /// to answer.
    fn invalid_method(&self, rsvc: &Engine, context: &Context, reference: &str, object: &JValue, method: &str, info: &Info) -> R<JValue>;
}

/// `EventCartridge`.
#[derive(Default)]
pub struct EventCartridge {
    /// The `ReferenceInsertionEventHandler`s, applied in turn, each to the previous one's result.
    pub reference_insertion: RwLock<Vec<Arc<dyn ReferenceInsertionEventHandler>>>,
    /// The `MethodExceptionEventHandler`: only the first one added is kept.
    pub method_exception: RwLock<Option<Arc<dyn MethodExceptionEventHandler>>>,
    /// The `IncludeEventHandler`s, applied in turn until one blocks the include.
    pub include: RwLock<Vec<Arc<dyn IncludeEventHandler>>>,
    /// The `InvalidReferenceEventHandler`s, asked in turn until one answers non-null (or `true`).
    pub invalid_reference: RwLock<Vec<Arc<dyn InvalidReferenceEventHandler>>>,
}

/// An event handler implementing any of the interfaces (`EventHandler`).
#[derive(Clone, Default)]
pub struct Handler {
    /// Set when the handler is a `ReferenceInsertionEventHandler`.
    pub reference_insertion: Option<Arc<dyn ReferenceInsertionEventHandler>>,
    /// Set when the handler is a `MethodExceptionEventHandler`.
    pub method_exception: Option<Arc<dyn MethodExceptionEventHandler>>,
    /// Set when the handler is an `IncludeEventHandler`.
    pub include: Option<Arc<dyn IncludeEventHandler>>,
    /// Set when the handler is an `InvalidReferenceEventHandler`.
    pub invalid_reference: Option<Arc<dyn InvalidReferenceEventHandler>>,
}

impl EventCartridge {
    /// `new EventCartridge()`: an empty cartridge, shared.
    pub fn new() -> Arc<EventCartridge> {
        Arc::new(EventCartridge::default())
    }

    /// `addEventHandler`: `false` when it implements none of the interfaces.
    pub fn add(&self, h: Handler) -> bool {
        let mut found = false;
        if let Some(r) = h.reference_insertion {
            self.reference_insertion.write().push(r);
            found = true;
        }
        if let Some(m) = h.method_exception {
            let mut slot = self.method_exception.write();
            if slot.is_none() {
                *slot = Some(m);
            }
            found = true;
        }
        if let Some(i) = h.include {
            self.include.write().push(i);
            found = true;
        }
        if let Some(i) = h.invalid_reference {
            self.invalid_reference.write().push(i);
            found = true;
        }
        found
    }

    /// Whether a `MethodExceptionEventHandler` was added.
    pub fn has_method_exception_handler(&self) -> bool {
        self.method_exception.read().is_some()
    }

    fn reference_insert(&self, rsvc: &Engine, ctx: &Context, reference: &str, mut value: JValue) -> R<JValue> {
        for h in self.reference_insertion.read().clone() {
            value = h.reference_insert(rsvc, ctx, reference, value)?;
        }
        Ok(value)
    }

    fn include_event(&self, rsvc: &Engine, ctx: &mut Context, mut path: Option<String>, current: Option<&str>, directive: &str) -> R<Option<String>> {
        for h in self.include.read().clone() {
            path = h.include_event(rsvc, ctx, path.as_deref(), current, directive)?;
            if path.is_none() {
                break;
            }
        }
        Ok(path)
    }

    fn invalid_get_method(&self, rsvc: &Engine, ctx: &Context, reference: &str, object: &JValue, property: Option<&str>, info: &Info) -> R<JValue> {
        for h in self.invalid_reference.read().clone() {
            let r = h.invalid_get_method(rsvc, ctx, reference, object, property, info)?;
            if !r.is_null() {
                return Ok(r);
            }
        }
        Ok(JValue::Null)
    }

    fn invalid_set_method(&self, rsvc: &Engine, ctx: &Context, left: &str, right: &str, info: &Info) -> R<bool> {
        for h in self.invalid_reference.read().clone() {
            if h.invalid_set_method(rsvc, ctx, left, right, info)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn invalid_method(&self, rsvc: &Engine, ctx: &Context, reference: &str, object: &JValue, method: &str, info: &Info) -> R<JValue> {
        for h in self.invalid_reference.read().clone() {
            let r = h.invalid_method(rsvc, ctx, reference, object, method, info)?;
            if !r.is_null() {
                return Ok(r);
            }
        }
        Ok(JValue::Null)
    }
}

/// `EventHandlerUtil.referenceInsert`.
pub fn reference_insert(rsvc: &Engine, ctx: &Context, reference: &str, value: JValue) -> R<JValue> {
    let value = rsvc.events.reference_insert(rsvc, ctx, reference, value)?;
    match ctx.event_cartridge() {
        Some(c) => c.reference_insert(rsvc, ctx, reference, value),
        None => Ok(value),
    }
}

/// `EventHandlerUtil.methodException`: `Err(None)` rethrows the original exception.
pub fn method_exception(rsvc: &Engine, ctx: &Context, class: &str, method: &str, e: &JError, info: &Info) -> Result<JValue, Option<VelocityError>> {
    if let Some(h) = rsvc.events.method_exception.read().clone() {
        return h.method_exception(rsvc, ctx, class, method, e, info);
    }
    if let Some(c) = ctx.event_cartridge() {
        let h = c.method_exception.read().clone();
        return match h {
            Some(h) => h.method_exception(rsvc, ctx, class, method, e, info),
            None => Ok(JValue::Null),
        };
    }
    Err(None)
}

/// `EventHandlerUtil.includeEvent`.
pub fn include_event(rsvc: &Engine, ctx: &mut Context, path: Option<String>, current: Option<&str>, directive: &str) -> R<Option<String>> {
    let path = rsvc.events.include_event(rsvc, ctx, path, current, directive)?;
    let cartridge = ctx.cartridge_arc();
    match cartridge {
        Some(c) => c.include_event(rsvc, ctx, path, current, directive),
        None => Ok(path),
    }
}

/// `EventHandlerUtil.invalidGetMethod`: the context cartridge's answer replaces the application's.
pub fn invalid_get_method(rsvc: &Engine, ctx: &Context, reference: &str, object: &JValue, property: Option<&str>, info: &Info) -> R<JValue> {
    let result = rsvc.events.invalid_get_method(rsvc, ctx, reference, object, property, info)?;
    match ctx.event_cartridge() {
        Some(c) => c.invalid_get_method(rsvc, ctx, reference, object, property, info),
        None => Ok(result),
    }
}

/// `EventHandlerUtil.invalidSetMethod`: the context's cartridge is asked only when no application handler returned
/// `true`.
pub fn invalid_set_method(rsvc: &Engine, ctx: &Context, left: &str, right: &str, info: &Info) -> R<()> {
    if !rsvc.events.invalid_set_method(rsvc, ctx, left, right, info)? {
        if let Some(c) = ctx.event_cartridge() {
            c.invalid_set_method(rsvc, ctx, left, right, info)?;
        }
    }
    Ok(())
}

/// `EventHandlerUtil.invalidMethod`: the context cartridge's answer replaces the application's.
pub fn invalid_method(rsvc: &Engine, ctx: &Context, reference: &str, object: &JValue, method: &str, info: &Info) -> R<JValue> {
    let result = rsvc.events.invalid_method(rsvc, ctx, reference, object, method, info)?;
    match ctx.event_cartridge() {
        Some(c) => c.invalid_method(rsvc, ctx, reference, object, method, info),
        None => Ok(result),
    }
}

/// Makes an event handler from its class name (`event_handler.*.class`).
pub type HandlerFactory = Arc<dyn Fn() -> Handler + Send + Sync>;

static HANDLER_CLASSES: std::sync::LazyLock<RwLock<std::collections::HashMap<String, HandlerFactory>>> = std::sync::LazyLock::new(|| RwLock::new(crate::handlers::builtin()));

/// Makes a handler class known (`ClassUtils.getNewInstance` for event handlers).
pub fn register_handler_class(class: &str, factory: HandlerFactory) {
    HANDLER_CLASSES.write().insert(class.to_string(), factory);
}

pub(crate) fn handler_class(class: &str) -> Option<HandlerFactory> {
    HANDLER_CLASSES.read().get(class).cloned()
}
