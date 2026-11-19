//! `org.apache.velocity.runtime.directive.Directive` for directives an application adds
//! (`runtime.custom_directives`, `RuntimeInstance.addDirective`), and MockServer's `#ifnull`.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use java_compat::lang::JValue;
use parking_lot::RwLock;

pub use crate::parser::DirectiveType;
pub use crate::render::DirectiveCall;
use crate::VelocityError;

/// A directive: its name, whether it has a body (`BLOCK`) or not (`LINE`), and `render`.
pub trait Directive: Send + Sync {
    /// `getName()`: the word after `#`.
    fn name(&self) -> String;

    /// `getType()`.
    fn directive_type(&self) -> DirectiveType;

    /// `render(context, writer, node)`: the arguments, body and output come through `call`.
    fn render(&self, call: &mut DirectiveCall<'_, '_>) -> Result<bool, VelocityError>;
}

/// Makes a directive's instance, as `RuntimeInstance` instantiates a directive's class for each use.
pub type DirectiveFactory = Arc<dyn Fn() -> Arc<dyn Directive> + Send + Sync>;

/// MockServer's `org.mockserver.templates.engine.velocity.directives.Ifnull`.
pub const IFNULL_DIRECTIVE: &str = "org.mockserver.templates.engine.velocity.directives.Ifnull";

/// `#ifnull($value) … #end`: the body when the value is null.
pub struct Ifnull;

impl Directive for Ifnull {
    fn name(&self) -> String {
        "ifnull".into()
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Block
    }

    fn render(&self, call: &mut DirectiveCall<'_, '_>) -> Result<bool, VelocityError> {
        let v = if call.arg_count() > 0 { call.arg_value(0)? } else { JValue::Null };
        if v.is_null() {
            call.render_body()?;
        }
        Ok(true)
    }
}

static DIRECTIVE_CLASSES: LazyLock<RwLock<HashMap<String, DirectiveFactory>>> = LazyLock::new(|| {
    let mut m: HashMap<String, DirectiveFactory> = HashMap::new();
    m.insert(IFNULL_DIRECTIVE.into(), Arc::new(|| Arc::new(Ifnull)));
    RwLock::new(m)
});

/// Makes a directive class known to `runtime.custom_directives` (`ClassUtils.getNewInstance`).
pub fn register_directive_class(class: &str, factory: DirectiveFactory) {
    DIRECTIVE_CLASSES.write().insert(class.to_string(), factory);
}

/// `loadDirective(directiveClass)`.
pub(crate) fn load(class: &str) -> Result<Arc<dyn Directive>, VelocityError> {
    match DIRECTIVE_CLASSES.read().get(class) {
        Some(f) => Ok(f()),
        None => Err(VelocityError::velocity(format!("Failed to load Directive: {class}")).with_cause(java_compat::lang::JError::new("java.lang.ClassNotFoundException", class))),
    }
}
