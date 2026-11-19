//! `VelocimacroFactory` and `VelocimacroManager`: where `#macro` definitions go (the global namespace, a library's,
//! or the defining template's own), which definitions may replace which, and how a call finds its macro.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::ast::{Block, Expr};

/// A macro argument: its name and default value.
#[derive(Debug, Clone)]
pub struct MacroArg {
    /// The argument's name, without the `$`.
    pub name: String,
    /// Its default value (`#macro(m $a=1)`), if it has one.
    pub default: Option<Expr>,
}

/// A `#macro` as its `VelocimacroProxy` holds it.
#[derive(Debug)]
pub struct Macro {
    /// The macro's name.
    pub name: String,
    /// Its arguments, in order.
    pub args: Vec<MacroArg>,
    /// Its body.
    pub body: Arc<Block>,
    /// `MacroEntry.getSourceTemplate()`: the defining template's name.
    pub source_template: String,
    /// Where the macro's body starts (`VelocimacroProxy.setLocation`).
    pub line: i32,
    /// The column where the macro's body starts.
    pub column: i32,
}

/// `org.apache.velocity.Template`'s identity as the macro system and scopes see it: its name, encoding and its own
/// (template-local) macros.
#[derive(Debug, Default)]
pub struct TemplateInfo {
    /// The template's name.
    pub name: String,
    /// The encoding it was read in.
    pub encoding: String,
    /// The macros the template defined for itself (inline macros under `velocimacro.inline.local_scope`).
    pub macros: Mutex<HashMap<String, Arc<Macro>>>,
}

impl TemplateInfo {
    /// A template's identity, with no macros yet.
    pub fn new(name: &str, encoding: &str) -> Arc<TemplateInfo> {
        Arc::new(TemplateInfo { name: name.to_string(), encoding: encoding.to_string(), macros: Mutex::default() })
    }

    /// Identity, for `StopCommand` targets.
    pub fn id(self: &Arc<TemplateInfo>) -> usize {
        Arc::as_ptr(self) as usize
    }
}

#[derive(Debug, Default)]
struct State {
    global: HashMap<String, Arc<Macro>>,
    libraries: HashSet<String>,
    register_from_lib: bool,
    namespaces_on: bool,
    inline_local_mode: bool,
    inline_replaces_global: bool,
    replace_allowed: bool,
    add_new_allowed: bool,
    template_local: bool,
    autoload: bool,
    library_names: Vec<String>,
}

/// The engine's velocimacro namespaces.
#[derive(Debug, Default)]
pub struct Velocimacros {
    state: Mutex<State>,
}

impl Velocimacros {
    /// `new VelocimacroManager()`: no macros, namespaces on, new macros allowed.
    pub fn new() -> Velocimacros {
        Velocimacros { state: Mutex::new(State { namespaces_on: true, add_new_allowed: true, ..State::default() }) }
    }

    /// The permissions `initVelocimacro` sets while loading libraries (`setReplacementPermission(true)`, no
    /// namespaces) and restores after.
    pub fn begin_libraries(&self, names: Vec<String>) {
        let mut s = self.state.lock();
        s.replace_allowed = true;
        s.inline_replaces_global = true;
        s.namespaces_on = false;
        s.library_names = names;
    }

    /// `setRegisterFromLib`: whether the macros added now come from a velocimacro library.
    pub fn set_register_from_lib(&self, on: bool) {
        self.state.lock().register_from_lib = on;
    }

    /// The rest of `initVelocimacro`: inline permissions from the configuration.
    pub fn finish_init(&self, inline_allow: bool, replace_global: bool, local_scope: bool, autoload: bool) {
        let mut s = self.state.lock();
        s.add_new_allowed = inline_allow;
        s.replace_allowed = replace_global;
        s.inline_replaces_global = replace_global;
        s.namespaces_on = true;
        s.template_local = local_scope;
        s.inline_local_mode = local_scope;
        s.autoload = autoload;
    }

    fn using_namespaces(s: &State) -> bool {
        s.namespaces_on && s.inline_local_mode
    }

    fn lookup(s: &State, name: &str, rendering: Option<&TemplateInfo>, template: Option<&TemplateInfo>) -> Option<Arc<Macro>> {
        if s.inline_replaces_global {
            if let Some(m) = rendering.and_then(|r| r.macros.lock().get(name).cloned()) {
                return Some(m);
            }
        }
        if Velocimacros::using_namespaces(s) {
            if let Some(t) = template {
                let macros = t.macros.lock();
                if !macros.is_empty() {
                    if let Some(m) = macros.get(name) {
                        return Some(m.clone());
                    }
                }
            }
        }
        s.global.get(name).cloned()
    }

    /// `VelocimacroManager.get(vmName, renderingTemplate, template)`.
    pub fn get(&self, name: &str, rendering: Option<&TemplateInfo>, template: Option<&TemplateInfo>) -> Option<Arc<Macro>> {
        let s = self.state.lock();
        Velocimacros::lookup(&s, name, rendering, template)
    }

    /// `isVelocimacro(name, template)`.
    pub fn is_macro(&self, name: &str, template: Option<&TemplateInfo>) -> bool {
        self.get(name, None, template).is_some()
    }

    /// `VelocimacroFactory.addVelocimacro`: `false` when the definition is rejected.
    pub fn add(&self, m: Macro, defining: &TemplateInfo) -> bool {
        let mut s = self.state.lock();
        let from_autoloaded_library = s.autoload && s.library_names.contains(&defining.name);
        if !from_autoloaded_library {
            if !s.add_new_allowed {
                return false;
            }
            if !s.template_local && !s.replace_allowed && Velocimacros::lookup(&s, &m.name, None, Some(defining)).is_some() {
                return false;
            }
        }
        let is_lib = if s.register_from_lib {
            s.libraries.insert(defining.name.clone());
            true
        } else {
            s.libraries.contains(&defining.name)
        };
        let m = Arc::new(m);
        if !is_lib && Velocimacros::using_namespaces(&s) {
            defining.macros.lock().insert(m.name.clone(), m);
        } else {
            s.global.insert(m.name.clone(), m);
        }
        true
    }

    /// `getLibraryName(vmName, template)`: the library a global macro came from.
    pub fn library_name(&self, name: &str, template: Option<&TemplateInfo>) -> Option<String> {
        let s = self.state.lock();
        if Velocimacros::using_namespaces(&s) && template.is_some_and(|t| t.macros.lock().contains_key(name)) {
            return None;
        }
        s.global.get(name).map(|m| m.source_template.clone())
    }

    /// Whether a changed velocimacro library is reloaded (`velocimacro.library.autoreload`).
    pub fn autoload(&self) -> bool {
        self.state.lock().autoload
    }
}
