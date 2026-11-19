//! Rendering: each node's `render()`, `value()` and `evaluate()`, and the directives' behaviour, with
//! `space.gobbling = lines` (directive prefixes and postfixes are dropped unless `MORE` text came before the `#`).

use std::sync::Arc;

use java_compat::lang::{JError, JList, JMap, JValue, MapKind};
use parking_lot::Mutex;

use crate::ast::*;
use crate::events::{self, Info};
use crate::introspect::{self, Outcome};
use crate::uberspector::Iteration;
use crate::config::SpaceGobbling;
use crate::log::LogExt;
use crate::math;
use java_compat::lang::JArray;
use crate::scope::{BlockRef, OwnerInfo, STOP_COMMAND, Scope};
use crate::vm::{Macro, TemplateInfo};
use crate::{Context, Engine, ErrorKind, Owner, StopTarget, VelocityError};

type R<T> = Result<T, VelocityError>;

pub struct Renderer<'e> {
    pub engine: &'e Engine,
    /// `getTemplateNameStack()`: the template being rendered and the `#parse`d ones inside it.
    template_names: Vec<String>,
    /// The templates the blocks being rendered were parsed into (`node.getTemplate()`; a macro body keeps its own).
    block_templates: Vec<Arc<TemplateInfo>>,
    /// `getCurrentResource()`: the template being merged or evaluated.
    resources: Vec<Arc<TemplateInfo>>,
    /// `getMacroNameStack()`.
    macro_names: Vec<String>,
    /// `getMacroLibraries()`: the templates `#parse`d so far (and `merge`'s libraries), searched for macros.
    macro_libraries: Vec<Arc<TemplateInfo>>,
    /// The engine's conversion handler is the introspector's while rendering.
    _conversions: crate::uberspect::HandlerGuard,
}

/// A reference child's first token image.
fn part_image(p: &RefPart) -> String {
    match p {
        RefPart::Identifier(n, _) | RefPart::Method(n, _, _) => n.clone(),
        RefPart::Index(..) => "[".into(),
    }
}

fn part_pos(p: &RefPart) -> Pos {
    match p {
        RefPart::Identifier(_, pos) | RefPart::Method(_, _, pos) | RefPart::Index(_, pos) => *pos,
    }
}

fn java_error(method: &str, e: JError) -> VelocityError {
    if e.class == STOP_COMMAND {
        return VelocityError::stop(StopTarget::Owner(Owner::Node(e.message.as_deref().and_then(|m| m.parse().ok()).unwrap_or(0))));
    }
    VelocityError::method_invocation(format!("Invocation of method '{method}' threw exception {e}"), e, method, None, 0, 0)
}

/// Identity, as `==` on Java references (primitives by value).
pub(crate) fn same_value(a: &JValue, b: &JValue) -> bool {
    same(a, b)
}

fn same(a: &JValue, b: &JValue) -> bool {
    match (a, b) {
        (JValue::Null, JValue::Null) => true,
        (JValue::Str(x), JValue::Str(y)) => Arc::ptr_eq(x, y),
        (JValue::List(x), JValue::List(y)) => x.same(y),
        (JValue::Map(x), JValue::Map(y)) => x.same(y),
        (JValue::Object(x), JValue::Object(y)) => std::ptr::addr_eq(Arc::as_ptr(x), Arc::as_ptr(y)),
        (JValue::Array(x), JValue::Array(y)) => Arc::ptr_eq(x, y),
        (JValue::Int(x), JValue::Int(y)) => x == y,
        (JValue::Long(x), JValue::Long(y)) => x == y,
        (JValue::Bool(x), JValue::Bool(y)) => x == y,
        _ => false,
    }
}

impl<'e> Renderer<'e> {
    pub fn new(engine: &'e Engine) -> Renderer<'e> {
        Renderer {
            engine,
            template_names: Vec::new(),
            block_templates: Vec::new(),
            resources: Vec::new(),
            macro_names: Vec::new(),
            macro_libraries: Vec::new(),
            _conversions: crate::uberspect::with_handler(engine.conversion_handler()),
        }
    }

    /// `getCurrentTemplateName()`.
    pub fn current_template(&self) -> Option<&str> {
        self.template_names.last().map(String::as_str)
    }

    /// `Directive.preRender`: puts a new scope under `name` when that scope control is enabled.
    fn pre_render(&self, ctx: &mut Context, name: &str, owner: Owner, info: OwnerInfo, foreach: bool) -> Option<Arc<Scope>> {
        if !self.engine.config.scope_enabled(name) {
            return None;
        }
        let scope = Scope::new(owner, info, ctx.get(name), foreach);
        ctx.put(name, JValue::Object(scope.clone()));
        Some(scope)
    }

    /// `Directive.postRender`: puts back what the scope replaced.
    fn post_render(&self, ctx: &mut Context, name: &str) {
        if !self.engine.config.scope_enabled(name) {
            return;
        }
        let current = ctx.get(name).unwrap_or(JValue::Null);
        let Some(scope) = crate::scope::as_scope(&current) else { return };
        if let Some(p) = scope.parent() {
            ctx.put(name, p);
        } else if let Some(r) = scope.replaced() {
            ctx.put(name, r);
        } else {
            ctx.remove(name);
        }
    }

    fn directive_info(&self, name: &str, block: bool, d: &Directive) -> OwnerInfo {
        OwnerInfo {
            name: name.to_string(),
            directive: true,
            kind: if block { "block".into() } else { "line".into() },
            template: self.node_template().unwrap_or_default(),
            line: d.line as i32,
            column: d.column as i32,
        }
    }

    /// `Template.merge(context, writer)`.
    /// `Template.merge(context, writer, macroLibraries)`: the libraries' macros are visible to the template.
    pub fn with_macro_libraries(mut self, libraries: Vec<Arc<TemplateInfo>>) -> Renderer<'e> {
        self.macro_libraries = libraries;
        self
    }

    pub fn merge(&mut self, template: &Arc<TemplateInfo>, root: &Block, ctx: &mut Context, out: &mut String) -> R<()> {
        let info = OwnerInfo { name: template.name.clone(), directive: false, kind: template.encoding.clone(), template: template.name.clone(), line: 0, column: 0 };
        self.pre_render(ctx, "template", Owner::Template(template.id()), info, false);
        self.template_names.push(template.name.clone());
        self.resources.push(template.clone());
        let result = match self.render_block(root, ctx, out) {
            Err(e) if e.stops(Owner::Template(template.id())) => {
                self.engine.log_for("rendering").debug(|| e.message());
                Ok(())
            }
            other => other,
        };
        self.template_names.pop();
        self.resources.pop();
        self.post_render(ctx, "template");
        result
    }

    /// `RuntimeInstance.render`: an `evaluate`d template.
    pub fn evaluate_top(&mut self, template: &Arc<TemplateInfo>, root: &Block, ctx: &mut Context, out: &mut String) -> R<()> {
        self.template_names.push(template.name.clone());
        self.resources.push(template.clone());
        let info = OwnerInfo { name: String::new(), directive: false, kind: String::new(), template: template.name.clone(), line: 0, column: 0 };
        self.pre_render(ctx, "evaluate", Owner::Runtime, info, false);
        // `context.self_reference_key`: the context, in itself.
        if let Some(key) = self.engine.config.self_reference_key.clone() {
            ctx.put(&key, JValue::Object(Arc::new(ContextSelf)));
        }
        let result = match self.render_block(root, ctx, out) {
            Err(e) if e.stops(Owner::Runtime) => {
                self.engine.log().debug(|| e.message());
                Ok(())
            }
            other => other,
        };
        self.template_names.pop();
        self.resources.pop();
        self.post_render(ctx, "evaluate");
        result
    }

    pub fn render_block(&mut self, b: &Block, ctx: &mut Context, out: &mut String) -> R<()> {
        if let Some(t) = &b.template {
            self.block_templates.push(t.clone());
        }
        let gobbling = self.engine.config.gobbling;
        let result = (|| {
            // `ASTBlock.render`.
            if gobbling == SpaceGobbling::None {
                out.push_str(&b.prefix);
            }
            for n in &b.nodes {
                self.render_node(n, ctx, out)?;
            }
            if !b.more_postfix.is_empty() || gobbling < SpaceGobbling::Lines {
                out.push_str(&b.postfix);
            }
            out.push_str(&b.more_postfix);
            Ok(())
        })();
        if b.template.is_some() {
            self.block_templates.pop();
        }
        result
    }

    fn render_node(&mut self, n: &Node, ctx: &mut Context, out: &mut String) -> R<()> {
        // Space gobbling: prefixes are written in `none` and `bc` (or after `MORE` text), postfixes in `none`.
        let gobbling = self.engine.config.gobbling;
        let prefix_shown = |more: &str| !more.is_empty() || gobbling < SpaceGobbling::Lines;
        let postfix_shown = |more: &str| !more.is_empty() || gobbling == SpaceGobbling::None;
        match n {
            Node::Text(t) => {
                out.push_str(t);
                Ok(())
            }
            Node::Reference(r) => self.render_reference(r, ctx, out),
            Node::If(i) => {
                if prefix_shown(&i.more_prefix) {
                    out.push_str(&i.prefix);
                }
                out.push_str(&i.more_prefix);
                if self.evaluate(&i.condition, ctx)? {
                    self.render_block(&i.then, ctx, out)?;
                } else {
                    let mut done = false;
                    for (cond, block) in &i.else_ifs {
                        if self.evaluate(cond, ctx)? {
                            self.render_block(block, ctx, out)?;
                            done = true;
                            break;
                        }
                    }
                    if !done {
                        if let Some(b) = &i.otherwise {
                            self.render_block(b, ctx, out)?;
                        }
                    }
                }
                if postfix_shown(&i.more_prefix) {
                    out.push_str(&i.postfix);
                }
                Ok(())
            }
            Node::Set(s) => {
                if prefix_shown(&s.more_prefix) {
                    out.push_str(&s.prefix);
                }
                out.push_str(&s.more_prefix);
                let value = self.value(&s.right, ctx)?;
                if value.is_null() && !self.engine.config.strict_ref {
                    let info = self.info(s.pos);
                    events::invalid_set_method(self.engine, ctx, s.left.first_image.trim_start_matches('$'), &s.right_last_image, &info)?;
                }
                if postfix_shown(&s.more_prefix) {
                    out.push_str(&s.postfix);
                }
                self.set_value(&s.left, ctx, value)?;
                Ok(())
            }
            Node::Directive(d) => {
                if prefix_shown(&d.more_prefix) {
                    out.push_str(&d.prefix);
                }
                out.push_str(&d.more_prefix);
                self.directive(d, ctx, out)?;
                if postfix_shown(&d.more_prefix) {
                    out.push_str(&d.postfix);
                }
                Ok(())
            }
        }
    }

    // ---- references ----------------------------------------------------------------------------------------

    /// `getTemplateName()` of the node being rendered: the template its block was parsed from.
    fn node_template(&self) -> Option<String> {
        self.block_templates.last().map(|t| t.name.clone()).or_else(|| self.current_template().map(str::to_string))
    }

    fn info(&self, pos: Pos) -> Info {
        Info::new(self.node_template().as_deref(), pos.line, pos.column)
    }

    fn at(&self, pos: Pos) -> String {
        crate::error::format_file_string(self.node_template().as_deref(), pos.line, pos.column)
    }

    fn strict(&self, r: &Reference) -> bool {
        self.engine.config.strict_ref && !(r.parts.is_empty() && r.if_test)
    }

    /// `ASTReference.getRootVariableValue`.
    fn root_value(&mut self, r: &Reference, ctx: &Context, strict: bool) -> R<JValue> {
        let v = ctx.get(&r.root).unwrap_or(JValue::Null);
        if v.is_null() && strict && r.alternate.is_none() && !ctx.contains_key(&r.root) {
            let identifier = r.parts.last().map(part_image).unwrap_or_default();
            let mut e = VelocityError::new(ErrorKind::MethodInvocation, format!("Variable ${} has not been set", r.root)).at(self.node_template().as_deref(), r.pos.line, r.pos.column);
            e.method_name = Some(identifier);
            return Err(e);
        }
        Ok(v)
    }

    /// `ASTReference.execute(null, context)`.
    pub fn reference_value(&mut self, r: &Reference, ctx: &mut Context) -> R<JValue> {
        self.execute_reference(r, ctx, false)
    }

    /// `ASTReference.execute`: `only_testing` when the reference is an `#if` operand.
    fn execute_reference(&mut self, r: &Reference, ctx: &mut Context, only_testing: bool) -> R<JValue> {
        if r.ref_type == RefType::Runt {
            return Ok(JValue::Null);
        }
        let config = &self.engine.config;
        let (warn_quiet, warn_null, warn_tested) = (config.invalid_references_quiet, config.invalid_references_null, config.invalid_references_tested);
        let strict = self.strict(r);
        let quiet = r.ref_type == RefType::Quiet;
        let mut result = self.root_value(r, ctx, strict)?;
        let info = self.info(r.pos);
        if result.is_null() && !strict {
            if (!quiet || warn_quiet) && (!r.parts.is_empty() || (!ctx.contains_key(&r.root) || warn_null) && (!only_testing || warn_tested)) {
                result = events::invalid_get_method(self.engine, ctx, &format!("${}", r.root), &JValue::Null, None, &info)?;
            }
            return self.alternate(r, result, ctx);
        }
        let mut previous = result.clone();
        let mut failed: Option<(usize, bool)> = None;
        for (i, part) in r.parts.iter().enumerate() {
            if strict && result.is_null() {
                let p = part_pos(part);
                return Err(VelocityError::velocity(format!("Attempted to access '{}' on a null value at {}", part_image(part), self.at(p))));
            }
            previous = result;
            let (v, found) = match self.execute_part(part, previous.clone(), ctx, strict) {
                Ok(x) => x,
                Err(mut e) => {
                    if e.kind == ErrorKind::MethodInvocation {
                        e.reference_name = Some(r.root.clone());
                    }
                    return Err(e);
                }
            };
            result = v;
            if result.is_null() && !strict {
                failed = Some((i, found));
                break;
            }
        }
        if result.is_null() {
            match failed {
                None => {
                    if (!ctx.contains_key(&r.root) || warn_null) && (!quiet || warn_quiet) && (!only_testing || warn_tested || !r.parts.is_empty()) {
                        result = events::invalid_get_method(self.engine, ctx, &format!("${}", r.root), &previous, None, &info)?;
                    }
                }
                Some((i, getter_found)) => {
                    if (!getter_found || warn_null) && (!quiet || warn_quiet) && (!only_testing || warn_tested || i + 1 < r.parts.len()) {
                        let mut name = format!("${}", r.root);
                        for p in &r.parts[..=i] {
                            match p {
                                RefPart::Method(m, _, _) => name.push_str(&format!(".{m}()")),
                                other => name.push_str(&format!(".{}", part_image(other))),
                            }
                        }
                        result = match &r.parts[i] {
                            RefPart::Method(m, _, _) => events::invalid_method(self.engine, ctx, &name, &previous, m, &info)?,
                            other => events::invalid_get_method(self.engine, ctx, &name, &previous, Some(&part_image(other)), &info)?,
                        };
                    }
                }
            }
        }
        self.alternate(r, result, ctx)
    }

    fn alternate(&mut self, r: &Reference, result: JValue, ctx: &mut Context) -> R<JValue> {
        match &r.alternate {
            Some(alt) if !introspect::as_boolean(&result, true) => self.value(alt, ctx),
            _ => Ok(result),
        }
    }

    /// `Object.getClass().toString()`: `class x.Y`.
    fn class_string(v: &JValue) -> String {
        format!("class {}", v.class_name())
    }

    /// `EventHandlerUtil.methodException`, or the `MethodInvocationException` a method or getter's exception becomes.
    fn method_exception(&mut self, ctx: &Context, target: &JValue, method: &str, e: JError, pos: Pos) -> R<JValue> {
        if e.class == STOP_COMMAND {
            return Err(java_error(method, e));
        }
        let info = self.info(pos);
        match events::method_exception(self.engine, ctx, &target.class_name(), method, &e, &info) {
            Ok(v) => Ok(v),
            // `handleInvocationException`: what the handler throws is wrapped too.
            Err(Some(h)) if h.is_stop() => Err(h),
            Err(Some(h)) => Err(VelocityError::method_invocation(
                format!("Invocation of method '{method}' in  {} threw exception {}", Renderer::class_string(target), h.java_to_string()),
                JError::new(&h.class_name(), h.message()),
                method,
                self.node_template().as_deref(),
                pos.line,
                pos.column,
            )),
            Err(None) => Err(VelocityError::method_invocation(
                format!("Invocation of method '{method}' in  {} threw exception {e}", Renderer::class_string(target)),
                e,
                method,
                self.node_template().as_deref(),
                pos.line,
                pos.column,
            )),
        }
    }

    // ---- introspection through the engine's uberspector ------------------------------------------------------

    /// `getMethod(obj, name, args, info)` then `invoke`: `NotFound` when there is no method.
    fn call_method(&self, target: &JValue, name: &str, args: &[JValue], pos: Pos) -> R<Outcome> {
        let info = self.info(pos);
        Ok(match self.engine.uberspect().get_method(target, name, args, &info)? {
            Some(m) => m.invoke(target, args),
            None => Outcome::NotFound,
        })
    }

    /// `ClassUtils.getMethod`: the method from the context's introspection cache when it was found for this class,
    /// else from the uberspector (and then cached), then invoked.
    fn call_method_cached(&self, target: &JValue, name: &str, args: &[JValue], pos: Pos, ctx: &mut Context) -> R<Outcome> {
        use crate::uberspector::{IntrospectionCacheData, MethodCacheKey};
        let classes = args.iter().map(|a| if a.is_null() { None } else { Some(a.class_name()) }).collect();
        let class_object = match target {
            JValue::Object(o) => o.as_any().downcast_ref::<crate::introspect::JClass>().map(|c| c.name.clone()),
            _ => None,
        };
        let key = MethodCacheKey::new(name, Some(classes), class_object.is_some());
        let class = class_object.unwrap_or_else(|| target.class_name());
        if let Some(icd) = ctx.icache_get(&key) {
            if !target.is_null() && icd.context_data == class {
                return Ok(icd.thingy.invoke(target, args));
            }
        }
        let info = self.info(pos);
        Ok(match self.engine.uberspect().get_method(target, name, args, &info)? {
            Some(m) => {
                if !target.is_null() {
                    ctx.icache_put(key, IntrospectionCacheData { context_data: class, thingy: m.clone() });
                }
                m.invoke(target, args)
            }
            None => Outcome::NotFound,
        })
    }

    /// `getPropertyGet(obj, identifier, info)` then `invoke`, with the executor's method name.
    fn call_getter(&self, target: &JValue, name: &str, pos: Pos) -> R<(Outcome, String)> {
        let info = self.info(pos);
        Ok(match self.engine.uberspect().get_property_get(target, Some(name), &info)? {
            Some(g) => (g.invoke(target), g.method_name()),
            None => (Outcome::NotFound, String::new()),
        })
    }

    /// `getPropertySet(obj, identifier, value, info)` then `invoke`: `false` when nothing can set it.
    fn call_setter(&self, target: &JValue, name: &str, value: JValue, pos: Pos) -> R<Result<bool, JError>> {
        let info = self.info(pos);
        Ok(match self.engine.uberspect().get_property_set(target, Some(name), &value, &info)? {
            Some(set) => match set.invoke(target, &value) {
                Outcome::NotFound => Ok(false),
                Outcome::Returned(_) => Ok(true),
                Outcome::Threw(e) | Outcome::IllegalArgument(e) => Err(e),
            },
            None => Ok(false),
        })
    }

    /// One `ASTIdentifier` / `ASTMethod` / `ASTIndex` child's `execute(o)`, and whether a method or getter was found
    /// (the introspection cache entry `invalidGetMethod` looks at).
    fn execute_part(&mut self, part: &RefPart, target: JValue, ctx: &mut Context, strict: bool) -> R<(JValue, bool)> {
        if let JValue::Object(o) = &target {
            if o.as_any().is::<ContextSelf>() {
                if let Some(v) = self.context_self_part(part, ctx)? {
                    return Ok((v, true));
                }
            }
        }
        match part {
            RefPart::Identifier(name, pos) => match self.call_getter(&target, name, *pos)? {
                (Outcome::NotFound, _) => {
                    if strict {
                        let mut e = VelocityError::new(ErrorKind::MethodInvocation, format!("Object '{}' does not contain property '{name}'", target.class_name())).at(self.node_template().as_deref(), pos.line, pos.column);
                        e.method_name = Some(name.clone());
                        return Err(e);
                    }
                    Ok((JValue::Null, false))
                }
                (Outcome::Returned(v), _) => Ok((v.unwrap_or(JValue::Null), true)),
                (Outcome::IllegalArgument(_), _) => Ok((JValue::Null, true)),
                (Outcome::Threw(e), method) => Ok((self.method_exception(ctx, &target, &method, e, *pos)?, true)),
            },
            RefPart::Method(name, args, pos) => {
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(self.value(a, ctx)?);
                }
                match self.call_method_cached(&target, name, &values, *pos, ctx)? {
                    Outcome::NotFound => {
                        if strict {
                            let classes: Vec<String> = values.iter().map(|v| if v.is_null() { "null".into() } else { v.class_name() }).collect();
                            let mut e = VelocityError::new(ErrorKind::MethodInvocation, format!("Object '{}' does not contain method {name}({})", target.class_name(), classes.join(", ")))
                                .at(self.node_template().as_deref(), pos.line, pos.column);
                            e.method_name = Some(name.clone());
                            return Err(e);
                        }
                        Ok((JValue::Null, false))
                    }
                    // A `void` method renders as "".
                    Outcome::Returned(None) => Ok((JValue::str(""), true)),
                    Outcome::Returned(Some(v)) => Ok((v, true)),
                    Outcome::Threw(e) | Outcome::IllegalArgument(e) => Ok((self.method_exception(ctx, &target, name, e, *pos)?, true)),
                }
            }
            RefPart::Index(e, pos) => {
                let arg = self.value(e, ctx)?;
                let arg = self.adjust_negative_index(arg, &target, *pos)?;
                let class = if arg.is_null() { "null".to_string() } else { arg.class_name() };
                match self.call_method(&target, "get", std::slice::from_ref(&arg), *pos)? {
                    Outcome::NotFound => {
                        if strict {
                            let mut err = VelocityError::new(ErrorKind::MethodInvocation, format!("Object '{}' does not contain method get({class})", target.class_name()))
                                .at(self.node_template().as_deref(), pos.line, pos.column);
                            err.method_name = Some("get".into());
                            return Err(err);
                        }
                        Ok((JValue::Null, false))
                    }
                    Outcome::Returned(None) => Ok((JValue::str(""), false)),
                    Outcome::Returned(Some(v)) => Ok((v, false)),
                    // A `RuntimeException` from `Method.invoke` itself passes through.
                    Outcome::IllegalArgument(e) => Err(VelocityError::runtime(e)),
                    Outcome::Threw(cause) => Err(VelocityError::velocity(format!("Error invoking method 'get({class})' in {} at {}", target.class_name(), self.at(*pos))).with_cause(cause)),
                }
            }
        }
    }

    /// A reference through the context itself: `VelocityContext`'s `get`, `put`, `containsKey`, `remove` and `getKeys`
    /// (a property is `get(name)`).
    fn context_self_part(&mut self, part: &RefPart, ctx: &mut Context) -> R<Option<JValue>> {
        Ok(match part {
            RefPart::Identifier(name, _) if name == "keys" || name == "Keys" => Some(JArray::objects(ctx.all_keys().into_iter().map(JValue::str).collect())),
            RefPart::Identifier(name, _) => Some(ctx.get(name).unwrap_or(JValue::Null)),
            RefPart::Method(name, args, _) => {
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(self.value(a, ctx)?);
                }
                match (name.as_str(), values.as_slice()) {
                    ("get", [k]) => Some(ctx.get(&k.to_java_string()).unwrap_or(JValue::Null)),
                    ("put", [k, v]) => {
                        let key = k.to_java_string();
                        let old = ctx.get(&key).unwrap_or(JValue::Null);
                        ctx.put(&key, v.clone());
                        Some(old)
                    }
                    ("containsKey", [k]) => Some(JValue::Bool(ctx.contains_key(&k.to_java_string()))),
                    ("remove", [k]) => Some(ctx.remove(&k.to_java_string()).unwrap_or(JValue::Null)),
                    ("getKeys", []) => Some(JArray::objects(ctx.all_keys().into_iter().map(JValue::str).collect())),
                    _ => None,
                }
            }
            RefPart::Index(..) => None,
        })
    }

    /// `ASTIndex.adjMinusIndexArg`: a negative `Integer` index counts from the end.
    fn adjust_negative_index(&mut self, arg: JValue, target: &JValue, pos: Pos) -> R<JValue> {
        let JValue::Int(i) = arg else { return Ok(arg) };
        if i >= 0 {
            return Ok(arg);
        }
        match self.call_method(target, "size", &[], pos)? {
            Outcome::Returned(Some(JValue::Int(size))) => Ok(JValue::Int(size + i)),
            Outcome::Returned(Some(other)) => Err(VelocityError::velocity(format!(
                "Method 'size()' on class '{}' returned '{}' when Integer was expected at {}",
                target.class_name(),
                other.class_name(),
                self.at(pos)
            ))),
            Outcome::Returned(None) | Outcome::NotFound => Err(VelocityError::velocity(format!(
                "A 'size()' method required for negative value {i} does not exist for class '{}' at {}",
                target.class_name(),
                self.at(pos)
            ))),
            Outcome::Threw(e) | Outcome::IllegalArgument(e) => {
                Err(VelocityError::velocity(format!("Error trying to calls the 'size()' method on '{}' at {}", target.class_name(), self.at(pos))).with_cause(e))
            }
        }
    }

    /// `ASTReference.evaluate`: a reference as an `#if` operand.
    fn evaluate_reference(&mut self, r: &Reference, ctx: &mut Context) -> R<bool> {
        let v = self.execute_reference(r, ctx, true)?;
        Ok(!v.is_null() && introspect::as_boolean(&v, self.engine.config.check_empty_objects))
    }

    /// `ASTReference.render`.
    /// `getNullString`: with `velocimacro.enable_bc_mode`, a macro argument that is null renders the literal the call
    /// passed.
    fn null_string(&self, r: &Reference, ctx: &Context) -> String {
        if self.engine.config.vm_bc_mode {
            if let Some(v) = ctx.get(&format!(".literal.{}", r.null_string)) {
                if let Some(first) = literal_stack(&v).and_then(|s| s.0.lock().front().cloned()) {
                    return first;
                }
            }
        }
        r.null_string.clone()
    }

    fn render_reference(&mut self, r: &Reference, ctx: &mut Context, out: &mut String) -> R<()> {
        if r.ref_type == RefType::Runt {
            out.push_str(&r.literal);
            return Ok(());
        }
        let strict_escape = self.engine.config.strict_escape;
        let value = if r.escaped && strict_escape { JValue::Bool(true) } else { self.execute_reference(r, ctx, false)? };
        if r.escaped {
            out.push_str(&r.esc_prefix);
            if value.is_null() {
                out.push('\\');
            }
            out.push_str(&self.null_string(r, ctx));
            return Ok(());
        }
        let value = events::reference_insert(self.engine, ctx, &r.literal, value)?;
        let mut text = None;
        if !value.is_null() {
            if let Some(b) = block_ref(&value) {
                out.push_str(&r.esc_prefix);
                out.push_str(&r.more_prefix);
                if self.render_block_ref(b, ctx, out)? {
                    return Ok(());
                }
            }
            text = self.as_string(&value, ctx)?;
        }
        match text {
            None => {
                if self.strict(r) {
                    if r.ref_type != RefType::Quiet {
                        let message = if value.is_null() {
                            format!("Reference {} evaluated to null when attempting to render at {}", r.literal, self.at(r.pos))
                        } else {
                            format!("Reference {} evaluated to object {} whose toString() method returned null at {}", r.literal, value.class_name(), self.at(r.pos))
                        };
                        return Err(VelocityError::velocity(message));
                    }
                    return Ok(());
                }
                let null_string = self.null_string(r, ctx);
                if !strict_escape {
                    out.push_str(&r.esc_prefix);
                }
                out.push_str(&r.esc_prefix);
                out.push_str(&r.more_prefix);
                out.push_str(&null_string);
                // `logOnNull`: strict mode allows null bare references.
                let log_on_null = self.engine.config.log_invalid_references && !(self.engine.config.strict_ref && r.parts.is_empty());
                if log_on_null && r.ref_type != RefType::Quiet {
                    self.engine.log_for("rendering").debug(|| {
                        format!(
                            "Null reference [template '{}', line {}, column {}]: {} cannot be resolved.",
                            self.node_template().unwrap_or_default(),
                            r.pos.line,
                            r.pos.column,
                            r.literal
                        )
                    });
                }
            }
            Some(t) => {
                out.push_str(&r.esc_prefix);
                out.push_str(&r.more_prefix);
                out.push_str(&t);
            }
        }
        Ok(())
    }

    /// `DuckType.asString`, rendering `#define`d blocks (`Block.Reference.toString()`).
    fn as_string(&mut self, v: &JValue, ctx: &mut Context) -> R<Option<String>> {
        if let Some(b) = block_ref(v) {
            let mut s = String::new();
            return Ok(if self.render_block_ref(b, ctx, &mut s)? { Some(s) } else { None });
        }
        Ok(introspect::as_string(v))
    }

    /// `Block.Reference.render`: `false` past the recursion limit; else `Block.render` with its directive's scope.
    fn render_block_ref(&mut self, b: &BlockRef, ctx: &mut Context, out: &mut String) -> R<bool> {
        crate::deadline::check()?;
        {
            let mut depth = b.depth.lock();
            *depth += 1;
            if *depth > b.max_depth {
                *depth -= 1;
                return Ok(false);
            }
        }
        self.pre_render(ctx, &b.scope_name, b.owner, b.owner_info.clone(), false);
        let result = match self.render_block(&b.block, ctx, out) {
            Err(e) if e.stops(b.owner) => Ok(()),
            other => other,
        };
        self.post_render(ctx, &b.scope_name);
        *b.depth.lock() -= 1;
        result.map(|_| true)
    }

    /// `ASTReference.setValue`: `false` when nothing could be set.
    fn set_value(&mut self, r: &Reference, ctx: &mut Context, value: JValue) -> R<bool> {
        if r.alternate.is_some() {
            return Ok(false);
        }
        if r.parts.is_empty() {
            ctx.put(&r.root, value);
            return Ok(true);
        }
        let strict = self.engine.config.strict_ref;
        let mut result = self.root_value(r, ctx, strict)?;
        if result.is_null() {
            return Ok(false);
        }
        let (last, init) = r.parts.split_last().expect("parts is not empty");
        for (i, part) in init.iter().enumerate() {
            result = self.execute_part(part, result, ctx, strict)?.0;
            if result.is_null() {
                if strict {
                    let next = &r.parts[i + 1];
                    let p = part_pos(next);
                    let mut e = VelocityError::new(ErrorKind::MethodInvocation, format!("Attempted to access '{}' on a null value", part_image(next))).at(self.node_template().as_deref(), p.line, p.column);
                    e.method_name = Some(part_image(next));
                    return Err(e);
                }
                return Ok(false);
            }
        }
        match last {
            RefPart::Index(e, pos) => {
                let arg = self.value(e, ctx)?;
                let arg = self.adjust_negative_index(arg, &result, *pos)?;
                let classes = [&arg, &value].map(|v| if v.is_null() { "null".to_string() } else { v.class_name() });
                let args = [arg.clone(), value.clone()];
                let mut outcome = self.call_method(&result, "set", &args, *pos)?;
                let mut name = "set";
                if !outcome.is_found() {
                    name = "put";
                    outcome = self.call_method(&result, "put", &args, *pos)?;
                }
                match outcome {
                    Outcome::NotFound => {
                        if strict {
                            return Err(VelocityError::velocity(format!(
                                "Found neither a 'set' or 'put' method with param types '({},{})' on class '{}' at {}",
                                classes[0],
                                classes[1],
                                result.class_name(),
                                self.at(*pos)
                            )));
                        }
                        Ok(false)
                    }
                    Outcome::Returned(_) => Ok(true),
                    Outcome::IllegalArgument(e) => Err(VelocityError::runtime(e)),
                    Outcome::Threw(e) => Err(VelocityError::method_invocation(
                        format!("Exception calling method '{name}({},{})' in  {}", classes[0], classes[1], Renderer::class_string(&result)),
                        e,
                        &r.parts.last().map(part_image).unwrap_or_default(),
                        self.node_template().as_deref(),
                        pos.line,
                        pos.column,
                    )),
                }
            }
            RefPart::Identifier(name, _) | RefPart::Method(name, _, _) => match self.call_setter(&result, name, value, r.pos)? {
                Ok(true) => Ok(true),
                Ok(false) => {
                    if strict {
                        let mut e = VelocityError::new(ErrorKind::MethodInvocation, format!("Object '{}' does not contain property '{name}'", result.class_name())).at(self.node_template().as_deref(), r.pos.line, r.pos.column);
                        e.method_name = Some(name.clone());
                        return Err(e);
                    }
                    Ok(false)
                }
                Err(e) => Err(VelocityError::method_invocation(
                    format!("ASTReference: Invocation of method '{name}' in  {} threw exception {e}", Renderer::class_string(&result)),
                    e,
                    name,
                    self.node_template().as_deref(),
                    r.pos.line,
                    r.pos.column,
                )),
            },
        }
    }

    // ---- expressions ---------------------------------------------------------------------------------------

    pub fn value(&mut self, e: &Expr, ctx: &mut Context) -> R<JValue> {
        Ok(match e {
            Expr::Str { image, interpolated, .. } => match interpolated {
                Some(block) => {
                    let mut s = String::new();
                    self.render_block(block, ctx, &mut s)?;
                    JValue::str(s)
                }
                None => JValue::str(image.as_str()),
            },
            Expr::Integer(v, _) | Expr::Float(v, _) => v.clone(),
            Expr::True => JValue::Bool(true),
            Expr::False => JValue::Bool(false),
            Expr::Reference(r) => self.reference_value(r, ctx)?,
            Expr::Range(l, r) => {
                let (l, r) = (self.value(l, ctx)?, self.value(r, ctx)?);
                if l.is_null() || r.is_null() {
                    return Ok(JValue::Null);
                }
                let (Some(l), Some(r)) = (introspect::as_number(&l), introspect::as_number(&r)) else { return Ok(JValue::Null) };
                let (l, r) = (math::int_value(&l), math::int_value(&r));
                if self.engine.config.immutable_ranges {
                    crate::range::IntegerRange::value(l, r)
                } else {
                    // Backward compatible: the list instantiated in memory.
                    let items: Vec<JValue> = if l >= r { (r..=l).rev().map(JValue::Int).collect() } else { (l..=r).map(JValue::Int).collect() };
                    JValue::List(JList::array_list(items))
                }
            }
            Expr::List(items) => {
                let mut values = Vec::with_capacity(items.len());
                for i in items {
                    values.push(self.value(i, ctx)?);
                }
                JValue::List(JList::array_list(values))
            }
            Expr::Map(entries) => {
                let map = JMap::new(MapKind::Linked);
                for (k, v) in entries {
                    let key = self.value(k, ctx)?;
                    let value = self.value(v, ctx)?;
                    map.put(key, value);
                }
                JValue::Map(map)
            }
            Expr::Word(_) => JValue::Null,
            Expr::Binary(op, l, r, _) => match op {
                BinOp::Or | BinOp::And | BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => JValue::Bool(self.evaluate(e, ctx)?),
                _ => {
                    let Expr::Binary(_, _, _, at) = e else { unreachable!() };
                    self.math(*op, l, r, *at, ctx)?
                }
            },
            Expr::Not(inner) => JValue::Bool(!self.evaluate(inner, ctx)?),
            Expr::Negate(inner) => {
                let v = self.value(inner, ctx)?;
                match introspect::as_number(&v) {
                    Some(n) => math::negate(&n),
                    None => JValue::Null,
                }
            }
            Expr::Group(inner) => self.value(inner, ctx)?,
        })
    }

    /// `ASTMathNode.value`.
    fn math(&mut self, op: BinOp, l: &Expr, r: &Expr, at: Pos, ctx: &mut Context) -> R<JValue> {
        let left = self.value(l, ctx)?;
        let right = self.value(r, ctx)?;
        if op == BinOp::Add {
            // `ASTAddNode.handleSpecial`: a string on either side concatenates.
            let (ls, rs) = (introspect::as_string_strict(&left), introspect::as_string_strict(&right));
            if ls.is_some() || rs.is_some() {
                let other = |v: &JValue, e: &Expr| if v.is_null() { e.literal() } else { v.to_java_string() };
                let ls = ls.unwrap_or_else(|| other(&left, l));
                let rs = rs.unwrap_or_else(|| other(&right, r));
                return Ok(JValue::str(ls + &rs));
            }
        }
        let (a, b) = (introspect::as_number(&left), introspect::as_number(&right));
        let (Some(a), Some(b)) = (a.clone(), b.clone()) else {
            let wrong_right = a.is_some();
            let wrong_type = if wrong_right { !right.is_null() } else { !left.is_null() };
            let message = format!(
                "{} side of math operation ({}) {}{}",
                if wrong_right { "Right" } else { "Left" },
                if wrong_right { r.literal() } else { l.literal() },
                if wrong_type { "is not a Number. " } else { "has a null value. " },
                self.at(at)
            );
            return self.math_failed(message);
        };
        Ok(match op {
            BinOp::Add => math::add(&a, &b),
            BinOp::Sub => math::subtract(&a, &b),
            BinOp::Mul => math::multiply(&a, &b),
            BinOp::Div if math::is_zero(&b) => return self.math_failed(format!("Right side of division operation is zero. Must be non-zero. {}", self.at(at))),
            BinOp::Div => math::divide(&a, &b),
            BinOp::Mod if math::is_zero(&b) => return self.math_failed(format!("Right side of modulus operation is zero. Must be non-zero. {}", self.at(at))),
            BinOp::Mod => math::modulo(&a, &b).map_err(VelocityError::velocity)?,
            _ => unreachable!("math operators only"),
        })
    }

    pub fn evaluate(&mut self, e: &Expr, ctx: &mut Context) -> R<bool> {
        Ok(match e {
            // Literals are empty only with `directive.if.empty_check`.
            Expr::Str { .. } => self.value(e, ctx)?.as_str().is_some_and(|s| !self.engine.config.check_empty_objects || !s.is_empty()),
            Expr::Integer(v, _) | Expr::Float(v, _) => !self.engine.config.check_empty_objects || !math::is_zero(v),
            Expr::True => true,
            Expr::False => false,
            Expr::Reference(r) => self.evaluate_reference(r, ctx)?,
            Expr::List(items) => !self.engine.config.check_empty_objects || !items.is_empty(),
            Expr::Map(entries) => !self.engine.config.check_empty_objects || !entries.is_empty(),
            // `SimpleNode.evaluate`: ranges, words and arithmetic are false.
            Expr::Range(..) | Expr::Word(_) => false,
            Expr::Binary(op, l, r, at) => match op {
                BinOp::Or => self.evaluate(l, ctx)? || self.evaluate(r, ctx)?,
                BinOp::And => self.evaluate(l, ctx)? && self.evaluate(r, ctx)?,
                BinOp::Eq => self.compare(*op, l, r, *at, ctx)?,
                BinOp::Ne => !self.compare(BinOp::Eq, l, r, *at, ctx)?,
                BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => self.compare(*op, l, r, *at, ctx)?,
                _ => false,
            },
            Expr::Not(inner) => !self.evaluate(inner, ctx)?,
            Expr::Negate(inner) | Expr::Group(inner) => self.evaluate(inner, ctx)?,
        })
    }

    /// `ASTComparisonNode.evaluate` (and `ASTEQNode`'s overrides).
    fn compare(&mut self, op: BinOp, l: &Expr, r: &Expr, at: Pos, ctx: &mut Context) -> R<bool> {
        let left = self.value(l, ctx)?;
        let right = self.value(r, ctx)?;
        if left.is_null() || right.is_null() {
            if op == BinOp::Eq {
                return Ok(left.is_null() && right.is_null() || introspect::as_null(if left.is_null() { &right } else { &left }));
            }
            let (side, e) = if left.is_null() { ("Left", l) } else { ("Right", r) };
            return self.comparison_failed(format!("{side} side ({}) of comparison operation has null value at {}", e.literal(), self.at(at)));
        }
        if !(left.as_str().is_some() && right.as_str().is_some()) {
            if let (Some(a), Some(b)) = (introspect::as_number(&left), introspect::as_number(&right)) {
                let c = math::compare(&a, &b);
                return Ok(match op {
                    BinOp::Eq => c.is_eq(),
                    BinOp::Lt => c.is_lt(),
                    BinOp::Gt => c.is_gt(),
                    BinOp::Le => c.is_le(),
                    BinOp::Ge => c.is_ge(),
                    _ => false,
                });
            }
        }
        if op != BinOp::Eq {
            let side = if math::is_number(&right) { "Left" } else { "Right" };
            return self.comparison_failed(format!("{side} side of comparison operation is not a number at {}", self.at(at)));
        }
        if introspect::same_class_family(&left, &right) {
            return Ok(java_compat::lang::java_equals(&left, &right));
        }
        Ok(introspect::as_string(&left) == introspect::as_string(&right))
    }

    /// `ASTMathNode`'s failures: a `MathException` with `runtime.strict_math`, else null.
    fn math_failed(&self, message: String) -> R<JValue> {
        if self.engine.config.strict_math {
            return Err(VelocityError::new(ErrorKind::Math, message));
        }
        Ok(JValue::Null)
    }

    /// `compareNull` / `compareNonNumber`: an error in strict mode, else logged and false.
    fn comparison_failed(&self, message: String) -> R<bool> {
        if self.engine.config.strict_ref {
            return Err(VelocityError::velocity(message));
        }
        Ok(false)
    }

    // ---- directives ----------------------------------------------------------------------------------------

    fn directive(&mut self, d: &Directive, ctx: &mut Context, out: &mut String) -> R<()> {
        if self.engine.is_removed_directive(&d.name) {
            return self.macro_call(d, ctx, out);
        }
        match d.name.as_str() {
            "foreach" => self.foreach(d, ctx, out),
            "macro" => Ok(()),
            "define" => {
                let key = match d.args.first() {
                    Some(Expr::Reference(r)) => r.first_image.chars().skip(1).collect(),
                    Some(other) => other.literal().chars().skip(1).collect(),
                    None => String::new(),
                };
                let value = BlockRef {
                    block: d.block.clone().unwrap_or_default(),
                    max_depth: self.engine.config.define_max_depth.max(0) as usize,
                    depth: Mutex::new(0),
                    scope_name: "define".into(),
                    owner: Owner::Node(d.id),
                    owner_info: self.directive_info("define", true, d),
                };
                ctx.put(&key, JValue::Object(Arc::new(value)));
                Ok(())
            }
            "evaluate" => self.evaluate_directive(d, ctx, out),
            "parse" | "include" => self.input(d, ctx, out),
            "break" => match d.args.first() {
                None => Err(VelocityError::stop(StopTarget::Nearest)),
                Some(arg) => {
                    let v = self.value(arg, ctx)?;
                    match crate::scope::as_scope(&v) {
                        Some(scope) => Err(VelocityError::stop(StopTarget::Owner(scope.owner))),
                        None => Err(VelocityError::velocity(format!(
                            "{} is not a valid org.apache.velocity.runtime.directive.Scope instance at {}",
                            arg.literal(),
                            self.at(Pos { line: d.line as i32, column: d.column as i32 })
                        ))),
                    }
                }
            },
            "stop" => match d.args.first() {
                None => Err(VelocityError::stop(StopTarget::Top(None))),
                Some(arg) => {
                    let v = self.value(arg, ctx)?;
                    Err(VelocityError::stop(StopTarget::Top(Some(v.to_java_string()))))
                }
            },
            name if self.engine.custom_directive(name).is_some() && crate::parser::builtin_directive_type(name).is_none() => {
                let directive = self.engine.custom_directive(name).expect("checked");
                let mut call = DirectiveCall { renderer: self, node: d, ctx, out };
                directive.render(&mut call).map(|_| ())
            }
            _ => self.macro_call(d, ctx, out),
        }
    }

    /// The uberspector's `getIterator`: the iteration, or `None` when the value cannot be iterated.
    fn iterator(&mut self, v: &JValue, pos: Pos) -> R<Option<Iteration>> {
        let info = self.info(pos);
        self.engine.uberspect().get_iterator(v, &info)
    }

    /// `Foreach.render`.
    fn foreach(&mut self, d: &Directive, ctx: &mut Context, out: &mut String) -> R<()> {
        let element_key = match d.args.first() {
            Some(Expr::Reference(r)) => r.root.clone(),
            Some(other) => other.literal().chars().skip(1).collect(),
            None => String::new(),
        };
        let config = &self.engine.config;
        let max_loops = if config.max_loops < 1 { i64::MAX } else { config.max_loops as i64 };
        let skip_invalid = if config.strict_ref { config.props.boolean(crate::config::SKIP_INVALID_ITERATOR, false) } else { config.skip_invalid_iterator };
        let iterable = match d.args.get(2) {
            Some(e) => self.value(e, ctx)?,
            None => JValue::Null,
        };
        let items = if iterable.is_null() { None } else { self.iterator(&iterable, Pos { line: d.line as i32, column: d.column as i32 })? };
        if items.is_none() && !iterable.is_null() && !skip_invalid {
            let node = d.args.get(2).map(Expr::literal).unwrap_or_default();
            let uberspect = if self.engine.config.is_secure() { crate::config::SECURE_UBERSPECTOR } else { crate::config::UBERSPECT_IMPL };
            return Err(VelocityError::velocity(format!(
                "#foreach parameter {node} at {} is of type {} and cannot be iterated by {uberspect}",
                self.at(d.arg_pos.get(2).copied().unwrap_or_default()),
                iterable.class_name()
            )));
        }
        let block = d.block.clone().unwrap_or_default();
        let mut items = items;
        let non_empty = match items.as_mut() {
            Some(i) => i.has_next()?,
            None => false,
        };
        let Some(mut items) = items.filter(|_| non_empty) else {
            if let Some(e) = &d.else_block {
                self.render_block(e, ctx, out)?;
            }
            return Ok(());
        };
        let old = ctx.get(&element_key).filter(|v| !v.is_null());
        let info = self.directive_info("foreach", true, d);
        let scope = self.pre_render(ctx, "foreach", Owner::Node(d.id), info, true);
        let mut count: i64 = 1;
        loop {
            if count > max_loops {
                break;
            }
            match items.has_next() {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => {
                    self.clean(ctx, &element_key, old);
                    return Err(e);
                }
            }
            count += 1;
            // `TimeLimitedForeach`: each `next()` counts towards the render's deadline
            if let Err(e) = crate::deadline::check() {
                self.clean(ctx, &element_key, old);
                return Err(e);
            }
            let item = match items.next() {
                Ok(v) => v,
                Err(e) => {
                    self.clean(ctx, &element_key, old);
                    return Err(e);
                }
            };
            ctx.put(&element_key, item);
            if let Some(s) = &scope {
                let has_next = items.has_next().unwrap_or(false);
                s.advance(has_next);
            }
            match self.render_block(&block, ctx, out) {
                Ok(()) => {}
                Err(e) if e.stops(Owner::Node(d.id)) => break,
                Err(e) => {
                    self.clean(ctx, &element_key, old);
                    return Err(e);
                }
            }
        }
        self.clean(ctx, &element_key, old);
        Ok(())
    }

    /// `Foreach.clean`: the element key back as it was, then `postRender`.
    fn clean(&mut self, ctx: &mut Context, key: &str, old: Option<JValue>) {
        match old {
            Some(o) => ctx.put(key, o),
            None => {
                ctx.remove(key);
            }
        }
        self.post_render(ctx, "foreach");
    }

    /// `Evaluate.render`: parses the argument into the current template and renders it.
    fn evaluate_directive(&mut self, d: &Directive, ctx: &mut Context, out: &mut String) -> R<()> {
        crate::deadline::check()?;
        let v = self.value(&d.args[0], ctx)?;
        let source = if v.is_null() { String::new() } else { v.to_java_string() };
        let template_name = self.current_template().unwrap_or_default().to_string();
        let template = self.resources.last().cloned().unwrap_or_else(|| TemplateInfo::new(&template_name, &self.engine.config.default_encoding));
        let at = |e: VelocityError| {
            let mut p = VelocityError::parse(e.message());
            p.template = Some(template_name.clone());
            p.line = d.line as i32;
            p.column = d.column as i32;
            p
        };
        let tree = self.engine.parse_into(&template, &source).map_err(|e| if e.is_parse() { at(e) } else { e })?;
        self.template_names.push(template_name.clone());
        let info = self.directive_info("evaluate", false, d);
        self.pre_render(ctx, "evaluate", Owner::Node(d.id), info, false);
        let result = match self.render_block(&tree, ctx, out) {
            Err(e) if e.stops(Owner::Node(d.id)) => Ok(()),
            Err(e) if e.is_parse() => Err(at(e)),
            other => other,
        };
        self.template_names.pop();
        self.post_render(ctx, "evaluate");
        result
    }

    /// `#parse` (`Parse.render`) and `#include` (`Include.render`).
    fn input(&mut self, d: &Directive, ctx: &mut Context, out: &mut String) -> R<()> {
        let at = self.at(Pos { line: d.line as i32, column: d.column as i32 });
        let current = self.current_template().map(str::to_string);
        if d.name == "parse" {
            crate::deadline::check()?;
            let Some(arg) = d.args.first() else { return Err(VelocityError::velocity(format!("#parse(): argument missing at {at}"))) };
            let v = self.value(arg, ctx)?;
            let source_arg = if v.is_null() { None } else { Some(v.to_java_string()) };
            let name = events::include_event(self.engine, ctx, source_arg, current.as_deref(), "parse")?;
            if self.engine.config.strict_ref && v.is_null() && name.is_none() {
                return Err(VelocityError::velocity(format!("The argument to #parse returned null at {at}")));
            }
            let Some(name) = name else { return Ok(()) };
            let max = self.engine.config.parse_max_depth;
            if max > 0 && self.template_names.len() >= max as usize {
                return Ok(());
            }
            let encoding = self.resources.last().map(|r| r.encoding.clone());
            let template = self.engine.get_template(&name, encoding.as_deref())?;
            let crate::Loaded::Template(t) = &*template else { return Ok(()) };
            self.macro_libraries.push(t.info.clone());
            let info = OwnerInfo { name: "parse".into(), directive: true, kind: "line".into(), template: self.node_template().unwrap_or_default(), line: d.line as i32, column: d.column as i32 };
            self.pre_render(ctx, "template", Owner::Node(d.id), info, false);
            self.template_names.push(name);
            let result = match self.render_block(&t.tree, ctx, out) {
                Err(e) if e.stops(Owner::Node(d.id)) => Ok(()),
                other => other,
            };
            self.template_names.pop();
            self.post_render(ctx, "template");
            return result;
        }
        let start = format!("{} ", self.engine.config.include_error_start.as_deref().unwrap_or("null"));
        let end = format!(" {}", self.engine.config.include_error_end.as_deref().unwrap_or("null"));
        let error_output = |out: &mut String, i: usize| out.push_str(&format!("{start}error with arg {i} please see log.{end}"));
        for (i, arg) in d.args.iter().enumerate() {
            if !matches!(d.arg_types[i], ArgType::StringLiteral | ArgType::Reference) {
                error_output(out, i);
                return Err(VelocityError::velocity(format!("invalid #include() argument '{}' at {at}", arg.literal())));
            }
            let v = self.value(arg, ctx)?;
            if v.is_null() {
                error_output(out, i);
                continue;
            }
            let Some(name) = events::include_event(self.engine, ctx, Some(v.to_java_string()), current.as_deref(), "include")? else { continue };
            let encoding = self.resources.last().map(|r| r.encoding.clone());
            let content = self.engine.get_content(&name, encoding.as_deref())?;
            if let crate::Loaded::Content(text) = &*content {
                out.push_str(text);
            }
        }
        Ok(())
    }

    /// `RuntimeMacro.render` (and `BlockMacro.render` for `#@name`).
    fn macro_call(&mut self, d: &Directive, ctx: &mut Context, out: &mut String) -> R<()> {
        let (name, body) = match d.name.strip_prefix('@') {
            Some(n) => (n.to_string(), d.block.clone()),
            None => (d.name.clone(), None),
        };
        let rendering = self.resources.last().cloned();
        let defining = self.block_templates.last().cloned();
        let mut vm = self.engine.velocimacro(&name, rendering.as_deref(), defining.as_deref())?;
        if vm.is_none() {
            for lib in self.macro_libraries.clone().iter().rev() {
                if let Some(m) = self.engine.velocimacro(&name, rendering.as_deref(), Some(lib))? {
                    vm = Some(m);
                    break;
                }
            }
        }
        let strict = self.engine.config.strict_ref && d.macro_call_closed;
        let Some(vm) = vm else {
            if strict {
                return Err(VelocityError::velocity(format!("Macro '#{name}' is not defined at {}", self.at(Pos { line: d.line as i32, column: d.column as i32 }))));
            }
            out.push_str(&d.macro_literal);
            return Ok(());
        };
        if let Some(i) = d.arg_types.iter().position(|t| *t == ArgType::Word) {
            let p = d.arg_pos.get(i).copied().unwrap_or_default();
            let mut e = VelocityError::new(ErrorKind::TemplateInit, format!("Invalid arg '{}' in macro #{name} at {}", d.args[i].literal(), self.at(p)));
            e.template = self.current_template().map(str::to_string);
            e.line = d.column as i32;
            e.column = d.line as i32;
            return Err(e);
        }
        let body_ref = body.map(|b| BlockRef {
            block: b,
            max_depth: self.engine.config.vm_max_depth.max(0) as usize,
            depth: Mutex::new(0),
            scope_name: name.clone(),
            owner: Owner::BlockMacro(d.id),
            owner_info: self.directive_info(&self.engine.config.vm_body_reference, true, d),
        });
        let info = self.directive_info(&name, false, d);
        self.pre_render(ctx, "macro", Owner::Node(d.id), info, false);
        let result = match self.render_macro(&vm, d, body_ref.map(|b| JValue::Object(Arc::new(b))), ctx, out) {
            Err(e) if e.stops(Owner::Node(d.id)) => Ok(()),
            other => other,
        };
        self.post_render(ctx, "macro");
        result
    }

    /// `VelocimacroProxy.render`.
    fn render_macro(&mut self, vm: &Arc<Macro>, d: &Directive, body: Option<JValue>, ctx: &mut Context, out: &mut String) -> R<()> {
        let config = &self.engine.config;
        let body_key = config.vm_body_reference.clone();
        let old_body = ctx.remove(&body_key);
        if let Some(b) = &body {
            ctx.put(&body_key, b.clone());
        }
        let call_args = d.args.len();
        let pos = Pos { line: d.line as i32, column: d.column as i32 };
        let log = self.engine.log_for("macro");
        if call_args > vm.args.len() {
            if config.vm_arguments_strict {
                return Err(VelocityError::velocity(format!(
                    "Provided {call_args} arguments but macro #{} accepts at most {} at {}",
                    vm.name,
                    vm.args.len(),
                    self.at(pos)
                )));
            }
            log.debug(|| format!("VM #{}: too many arguments to macro. Wanted {} got {call_args}", vm.name, vm.args.len()));
        }
        let max_depth = config.vm_max_depth;
        if max_depth > 0 && max_depth as usize == self.macro_names.len() {
            let mut message = format!("Max calling depth of {max_depth} was exceeded in macro '{}' with Call Stack:{}", vm.name, self.macro_names.join("->"));
            message.push_str(&format!(" at {}", crate::error::format_file_string(Some(&vm.source_template), vm.line, vm.column)));
            log.error(|| message.clone());
            self.macro_names.clear();
            return Err(VelocityError::new(ErrorKind::MacroOverflow, message));
        }
        // `handleArgValues`: evaluate every argument (or its default), then bind them all.
        let mut saved: Vec<(String, Option<JValue>, JValue)> = Vec::new();
        let mut warned_missing_arguments = false;
        for (i, arg) in vm.args.iter().enumerate() {
            let old = if ctx.contains_key(&arg.name) { Some(ctx.get(&arg.name).unwrap_or(JValue::Null)) } else { None };
            let value = match (d.args.get(i), &arg.default) {
                (Some(a), _) => self.value(a, ctx)?,
                (None, Some(default)) => self.value(default, ctx)?,
                (None, None) if config.vm_arguments_strict => {
                    let min = vm.args.iter().filter(|a| a.default.is_none()).count();
                    return Err(VelocityError::velocity(format!(
                        "Need at least {min} argument for macro #{} but only {call_args} where provided at {}",
                        vm.name,
                        self.at(pos)
                    )));
                }
                (None, None) => {
                    if !warned_missing_arguments {
                        log.debug(|| format!("VM #{}: too few arguments to macro. Wanted {} got {call_args}", vm.name, vm.args.len()));
                        warned_missing_arguments = true;
                    }
                    // In bc mode the global value is the default.
                    if config.vm_bc_mode { old.clone().unwrap_or(JValue::Null) } else { JValue::Null }
                }
            };
            if config.vm_bc_mode {
                if let Some(a) = d.args.get(i) {
                    // Velocity 1.7 kept the literal of a reference or string argument, else `$<name>`.
                    let literal = match a {
                        Expr::Reference(_) | Expr::Str { .. } => a.literal(),
                        _ => format!("${}", arg.name),
                    };
                    let key = format!(".literal.${}", arg.name);
                    let existing = ctx.get(&key).filter(|v| literal_stack(v).is_some());
                    let stack = match existing {
                        Some(v) => v,
                        None => {
                            let v = JValue::Object(Arc::new(LiteralStack(Mutex::new(Default::default()))));
                            ctx.put(&key, v.clone());
                            v
                        }
                    };
                    literal_stack(&stack).expect("a literal stack").0.lock().push_front(literal);
                }
            }
            saved.push((arg.name.clone(), old, value));
        }
        for (name, _, value) in &saved {
            ctx.put(name, value.clone());
        }
        // `MacroCallCheckingContext.pushCurrentMacroName`: each macro call counts towards the render's deadline
        let result = crate::deadline::check();
        self.macro_names.push(vm.name.clone());
        let result = result.and_then(|()| self.render_block(&vm.body, ctx, out));
        self.macro_names.pop();
        let current = ctx.get(&body_key);
        let untouched = match (&current, &body) {
            (Some(c), Some(b)) => same(c, b),
            (None, None) => true,
            _ => false,
        };
        if untouched {
            match old_body {
                Some(o) => ctx.put(&body_key, o),
                None => {
                    ctx.remove(&body_key);
                }
            }
        }
        for (name, old, given) in saved {
            let current = ctx.get(&name).unwrap_or(JValue::Null);
            if same(&current, &given) || current.is_null() && given.is_null() {
                match old {
                    None => {
                        ctx.remove(&name);
                    }
                    Some(o) => ctx.put(&name, o),
                }
            }
            if config.vm_bc_mode {
                // Every argument pops, passed or not (as in Velocity).
                let key = format!(".literal.${name}");
                if let Some(v) = ctx.get(&key) {
                    if let Some(stack) = literal_stack(&v) {
                        let mut stack = stack.0.lock();
                        stack.pop_front();
                        if stack.is_empty() {
                            drop(stack);
                            ctx.remove(&key);
                        }
                    }
                }
            }
        }
        result
    }
}

/// The context as a value (`context.self_reference_key`), class `VelocityContext`.
struct ContextSelf;

impl java_compat::lang::JObject for ContextSelf {
    fn class_name(&self) -> String {
        "org.apache.velocity.VelocityContext".into()
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<Result<Option<JValue>, java_compat::lang::JError>> {
        None
    }

    fn to_java_string(&self) -> String {
        java_compat::reflect::identity_string(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A custom directive's view of its node while it renders (`context`, `writer`, `node` in Java).
pub struct DirectiveCall<'a, 'e> {
    renderer: &'a mut Renderer<'e>,
    node: &'a Directive,
    ctx: &'a mut Context,
    out: &'a mut String,
}

impl DirectiveCall<'_, '_> {
    /// The engine rendering (`rsvc`).
    pub fn engine(&self) -> &Engine {
        self.renderer.engine
    }

    /// The context the directive renders in.
    pub fn context(&mut self) -> &mut Context {
        self.ctx
    }

    /// The number of arguments (the body not counted).
    pub fn arg_count(&self) -> usize {
        self.node.args.len()
    }

    /// `node.jjtGetChild(i).value(context)`.
    pub fn arg_value(&mut self, i: usize) -> R<JValue> {
        match self.node.args.get(i) {
            Some(a) => self.renderer.value(a, self.ctx),
            None => Err(VelocityError::runtime(java_compat::lang::JError::new("java.lang.ArrayIndexOutOfBoundsException", format!("Index {i} out of bounds for length {}", self.node.args.len())))),
        }
    }

    /// `node.jjtGetChild(i).literal()`.
    pub fn arg_literal(&self, i: usize) -> Option<String> {
        self.node.args.get(i).map(Expr::literal)
    }

    /// Whether the directive has a body (a block directive).
    pub fn has_body(&self) -> bool {
        self.node.block.is_some()
    }

    /// Renders the body into the output.
    pub fn render_body(&mut self) -> R<()> {
        if let Some(b) = self.node.block.clone() {
            self.renderer.render_block(&b, self.ctx, self.out)?;
        }
        Ok(())
    }

    /// Renders the body on its own.
    pub fn render_body_to_string(&mut self) -> R<String> {
        let mut s = String::new();
        if let Some(b) = self.node.block.clone() {
            self.renderer.render_block(&b, self.ctx, &mut s)?;
        }
        Ok(s)
    }

    /// Writes `text` to the output.
    pub fn write(&mut self, text: &str) {
        self.out.push_str(text);
    }

    /// `getTemplateName()`, `getLine()`, `getColumn()`.
    pub fn template_name(&self) -> Option<String> {
        self.renderer.node_template()
    }

    /// `getLine()`: the directive's line.
    pub fn line(&self) -> i32 {
        self.node.line as i32
    }

    /// `getColumn()`: the directive's column.
    pub fn column(&self) -> i32 {
        self.node.column as i32
    }
}

/// The `Deque<String>` `velocimacro.enable_bc_mode` keeps under `.literal.$<arg>`: the literals the calls passed.
struct LiteralStack(Mutex<std::collections::VecDeque<String>>);

impl java_compat::lang::JObject for LiteralStack {
    fn class_name(&self) -> String {
        "java.util.LinkedList".into()
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<Result<Option<JValue>, java_compat::lang::JError>> {
        None
    }

    fn to_java_string(&self) -> String {
        format!("[{}]", self.0.lock().iter().cloned().collect::<Vec<_>>().join(", "))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn literal_stack(v: &JValue) -> Option<&LiteralStack> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<LiteralStack>(),
        _ => None,
    }
}

fn block_ref(v: &JValue) -> Option<&BlockRef> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<BlockRef>(),
        _ => None,
    }
}

