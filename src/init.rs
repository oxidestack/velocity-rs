//! What the nodes' `init()` derives after parsing: the template every block belongs to (`getTemplateName()`), and
//! which bare references are `#if` operands (`ASTReference.init`'s strict-mode exemption).

use std::sync::Arc;

use crate::ast::*;
use crate::{ErrorKind, VelocityError};
use crate::vm::TemplateInfo;

/// The node above an expression, as far as the exemption cares.
#[derive(Clone, Copy, PartialEq)]
enum Parent {
    /// `ASTNotNode`, `ASTAndNode`, `ASTOrNode` or `ASTExpression`.
    Relaxing,
    Other,
}

/// The settings directive `init()`s read.
pub struct InitSettings {
    pub strict_ref: bool,
    /// `space.gobbling = bc`.
    pub bc: bool,
    /// `space.gobbling = structured`.
    pub structured: bool,
}

/// `nodeTree.init`: `Err` is a `TemplateInitException` (kind `TemplateInit`, located).
pub fn init(tree: &mut Block, template: &Arc<TemplateInfo>, settings: &InitSettings) -> Result<(), VelocityError> {
    block(tree, template, false);
    if settings.bc {
        bc_set_prefixes(tree);
    }
    if settings.structured {
        structured(tree);
    }
    check_block(tree, template, settings)
}

fn init_error(message: String, template: &TemplateInfo, line: i32, column: i32) -> VelocityError {
    let mut e = VelocityError::new(ErrorKind::TemplateInit, message);
    e.template = Some(template.name.clone());
    e.line = line;
    e.column = column;
    e
}

fn check_block(b: &Block, template: &TemplateInfo, settings: &InitSettings) -> Result<(), VelocityError> {
    for n in &b.nodes {
        match n {
            Node::Directive(d) => {
                check_directive(d, template, settings)?;
                if let Some(b) = &d.block {
                    check_block(b, template, settings)?;
                }
                if let Some(b) = &d.else_block {
                    check_block(b, template, settings)?;
                }
            }
            Node::If(i) => {
                check_block(&i.then, template, settings)?;
                for (_, b) in &i.else_ifs {
                    check_block(b, template, settings)?;
                }
                if let Some(b) = &i.otherwise {
                    check_block(b, template, settings)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_directive(d: &Directive, template: &TemplateInfo, settings: &InitSettings) -> Result<(), VelocityError> {
    let (line, column) = (d.line as i32, d.column as i32);
    match d.name.as_str() {
        "evaluate" => {
            if d.args.is_empty() {
                return Err(init_error("#evaluate() requires exactly one argument".into(), template, line, column));
            }
            if d.args.len() > 1 {
                let p = d.arg_pos[1];
                return Err(init_error("#evaluate() requires exactly one argument".into(), template, p.line, p.column));
            }
            if !matches!(d.arg_types[0], ArgType::StringLiteral | ArgType::Reference) {
                let p = d.arg_pos[0];
                return Err(init_error("#evaluate()  argument must be a string literal or reference".into(), template, p.line, p.column));
            }
        }
        name if settings.strict_ref && d.macro_call_closed && !is_builtin(name) => {
            // `RuntimeMacro.init`: a word argument is an error right away in strict mode.
            if let Some(i) = d.arg_types.iter().position(|t| *t == ArgType::Word) {
                let p = d.arg_pos[i];
                let at = crate::error::format_file_string(Some(&template.name), p.line, p.column);
                return Err(init_error(format!("Invalid arg '{}' in macro #{} at {at}", d.args[i].literal(), name.trim_start_matches('@')), template, 0, 0));
            }
        }
        _ => {}
    }
    Ok(())
}

/// `ASTSetDirective.init` in `bc` mode: Velocity 1.x eats a `#set`'s prefix only when the previous sibling is not
/// text; a blank previous text is emptied instead.
fn bc_set_prefixes(b: &mut Block) {
    for i in 0..b.nodes.len() {
        let (before, rest) = b.nodes.split_at_mut(i);
        match &mut rest[0] {
            Node::Set(s) => match before.last_mut() {
                Some(Node::Text(t)) => {
                    if t.chars().all(|c| c == ' ' || c == '\t') {
                        t.clear();
                    }
                }
                _ => s.prefix.clear(),
            },
            Node::If(n) => {
                bc_set_prefixes(&mut n.then);
                for (_, b) in &mut n.else_ifs {
                    bc_set_prefixes(b);
                }
                if let Some(b) = &mut n.otherwise {
                    bc_set_prefixes(b);
                }
            }
            Node::Directive(d) => {
                for b in [&mut d.block, &mut d.else_block].into_iter().flatten() {
                    if let Some(b) = Arc::get_mut(b) {
                        bc_set_prefixes(b);
                    }
                }
            }
            _ => {}
        }
    }
}

fn block_mut(b: &mut Arc<Block>) -> Option<&mut Block> {
    Arc::get_mut(b)
}

/// `space.gobbling = structured`, bottom-up like the nodes' `init()`: a block directive without `MORE` text before
/// its `#` (`ASTDirective.init`), and an `#if` ending its line (`ASTIfStatement.init`), un-indent their content by
/// the indentation it has beyond their own (`NodeUtils.fixIndentation`).
fn structured(b: &mut Block) {
    for n in &mut b.nodes {
        match n {
            Node::Directive(d) => {
                for b in [&mut d.block, &mut d.else_block].into_iter().flatten() {
                    if let Some(b) = block_mut(b) {
                        structured(b);
                    }
                }
                if d.more_prefix.is_empty() && d.block.is_some() {
                    let mut fixer = IndentationFixer::new(d.prefix.clone());
                    fixer.directive_children(d);
                }
            }
            Node::If(i) => {
                structured(&mut i.then);
                for (_, b) in &mut i.else_ifs {
                    structured(b);
                }
                if let Some(b) = &mut i.otherwise {
                    structured(b);
                }
                if !i.postfix.is_empty() {
                    let mut fixer = IndentationFixer::new(i.prefix.clone());
                    fixer.if_children(i);
                }
            }
            _ => {}
        }
    }
}

/// `IndentationFixer`: the first text or prefix inside the parent tells the extra indentation, which is then removed
/// from the start of every line of texts and prefixes.
struct IndentationFixer {
    parent: String,
    extra: Option<String>,
}

impl IndentationFixer {
    fn new(parent: String) -> IndentationFixer {
        IndentationFixer { parent, extra: None }
    }

    /// `fillExtraIndentation`: `^parent(\s+)`.
    fn fill_extra(&mut self, prefix: &str) {
        let extra = prefix.strip_prefix(self.parent.as_str()).map(|rest| {
            let n = rest.char_indices().find(|(_, c)| !is_java_space(*c)).map_or(rest.len(), |(i, _)| i);
            rest[..n].to_string()
        });
        self.extra = Some(extra.unwrap_or_default());
    }

    fn extra(&mut self, text: &str) -> &str {
        if self.extra.is_none() {
            self.fill_extra(text);
        }
        self.extra.as_deref().unwrap_or_default()
    }

    /// `Pattern.compile("^" + parent + extra, MULTILINE).matcher(text).replaceAll(parent)`.
    fn fix(&self, text: &str) -> String {
        let needle = format!("{}{}", self.parent, self.extra.as_deref().unwrap_or_default());
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        let mut copied = 0;
        let bytes = text.as_bytes();
        while i <= text.len() {
            let line_start = i == 0 || {
                let prev = text[..i].chars().next_back().unwrap_or_default();
                matches!(prev, '\n' | '\u{85}' | '\u{2028}' | '\u{2029}') || prev == '\r' && bytes.get(i) != Some(&b'\n')
            };
            if line_start && i < text.len() && text[i..].starts_with(&needle) {
                out.push_str(&text[copied..i]);
                out.push_str(&self.parent);
                i += needle.len();
                copied = i;
                continue;
            }
            match text[i..].chars().next() {
                Some(c) => i += c.len_utf8(),
                None => break,
            }
        }
        out.push_str(&text[copied..]);
        out
    }

    fn text(&mut self, t: &mut String) {
        if !self.extra(t).is_empty() {
            *t = self.fix(t);
        }
    }

    /// `visit(ASTDirective)` / `visit(ASTIfStatement)` / `visit(ASTSetDirective)`: `true` when the prefix was fixed.
    fn prefix(&mut self, prefix: &mut String) -> bool {
        if prefix.is_empty() || self.extra(prefix).is_empty() {
            return false;
        }
        *prefix = self.fix(prefix);
        true
    }

    fn node(&mut self, n: &mut Node) {
        match n {
            Node::Text(t) => self.text(t),
            Node::Directive(d) => {
                if self.prefix(&mut d.prefix) && d.block.is_some() {
                    self.directive_children(d);
                }
            }
            Node::If(i) => {
                if self.prefix(&mut i.prefix) {
                    self.if_children(i);
                }
            }
            Node::Set(s) => {
                self.prefix(&mut s.prefix);
            }
            Node::Reference(_) => {}
        }
    }

    /// `visit(ASTBlock)`.
    fn block(&mut self, b: &mut Block) {
        if !b.prefix.is_empty() {
            for n in &mut b.nodes {
                self.node(n);
            }
        }
    }

    fn directive_children(&mut self, d: &mut Directive) {
        for b in [&mut d.block, &mut d.else_block].into_iter().flatten() {
            if let Some(b) = block_mut(b) {
                self.block(b);
            }
        }
    }

    /// The `#if`'s children: its block, then `#elseif` and `#else` statements, which only visit their blocks once the
    /// extra indentation is known and not empty.
    fn if_children(&mut self, i: &mut If) {
        self.block(&mut i.then);
        let known = |f: &IndentationFixer| f.extra.as_deref().is_some_and(|e| !e.is_empty());
        for (_, b) in &mut i.else_ifs {
            if known(self) {
                self.block(b);
            }
        }
        if let Some(b) = &mut i.otherwise {
            if known(self) {
                self.block(b);
            }
        }
    }
}

/// `\s` in `java.util.regex`.
fn is_java_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r')
}

fn is_builtin(name: &str) -> bool {
    crate::parser::builtin_directive_type(name).is_some() || name == "ifnull"
}

fn block(b: &mut Block, name: &Arc<TemplateInfo>, in_if: bool) {
    b.template = Some(name.clone());
    for n in &mut b.nodes {
        node(n, name, in_if);
    }
}

fn shared(b: &mut Arc<Block>, name: &Arc<TemplateInfo>, in_if: bool) {
    if let Some(b) = Arc::get_mut(b) {
        block(b, name, in_if);
    }
}

fn node(n: &mut Node, name: &Arc<TemplateInfo>, in_if: bool) {
    match n {
        Node::Text(_) => {}
        Node::Reference(r) => reference(r, name, Parent::Other, in_if),
        Node::If(i) => {
            expr(&mut i.condition, name, Parent::Relaxing, true);
            block(&mut i.then, name, true);
            for (c, b) in &mut i.else_ifs {
                expr(c, name, Parent::Relaxing, true);
                block(b, name, true);
            }
            if let Some(b) = &mut i.otherwise {
                block(b, name, true);
            }
        }
        Node::Set(s) => {
            reference(&mut s.left, name, Parent::Other, in_if);
            expr(&mut s.right, name, Parent::Relaxing, in_if);
        }
        Node::Directive(d) => {
            for a in &mut d.args {
                expr(a, name, Parent::Other, in_if);
            }
            if let Some(b) = &mut d.block {
                shared(b, name, in_if);
            }
            if let Some(b) = &mut d.else_block {
                shared(b, name, in_if);
            }
        }
    }
}

fn reference(r: &mut Reference, name: &Arc<TemplateInfo>, parent: Parent, in_if: bool) {
    r.if_test = r.parts.is_empty() && parent == Parent::Relaxing && in_if;
    for p in &mut r.parts {
        match p {
            RefPart::Method(_, args, _) => args.iter_mut().for_each(|a| expr(a, name, Parent::Other, in_if)),
            RefPart::Index(e, _) => expr(e, name, Parent::Other, in_if),
            RefPart::Identifier(..) => {}
        }
    }
    if let Some(alt) = &mut r.alternate {
        expr(alt, name, Parent::Other, in_if);
    }
}

fn expr(e: &mut Expr, name: &Arc<TemplateInfo>, parent: Parent, in_if: bool) {
    match e {
        Expr::Reference(r) => reference(r, name, parent, in_if),
        Expr::Str { interpolated: Some(b), .. } => block(b, name, false),
        Expr::Binary(op, l, r, _) => {
            let p = if matches!(op, BinOp::And | BinOp::Or) { Parent::Relaxing } else { Parent::Other };
            expr(l, name, p, in_if);
            expr(r, name, p, in_if);
        }
        Expr::Range(l, r) => {
            expr(l, name, Parent::Other, in_if);
            expr(r, name, Parent::Other, in_if);
        }
        Expr::Not(x) | Expr::Group(x) => expr(x, name, Parent::Relaxing, in_if),
        Expr::Negate(x) => expr(x, name, Parent::Other, in_if),
        Expr::List(items) => items.iter_mut().for_each(|i| expr(i, name, Parent::Other, in_if)),
        Expr::Map(entries) => entries.iter_mut().for_each(|(k, v)| {
            expr(k, name, Parent::Other, in_if);
            expr(v, name, Parent::Other, in_if);
        }),
        _ => {}
    }
}
