//! XPath 1.0 over the DOM, as the JDK's (Xalan's) `XPath.evaluate(…, NODESET)` sees it: namespace declarations
//! are namespace nodes, not attributes; adjacent text and CDATA nodes are one text node (the first DOM node
//! stands for it); no variables, no namespace context (a prefixed name test is an error).

use std::sync::Arc;

use super::dom::*;

/// An XPath expression that cannot be parsed or evaluated, with its message.
#[derive(Debug, Clone, PartialEq)]
pub struct XPathError(pub String);

type R<T> = Result<T, XPathError>;

fn err<T>(m: impl Into<String>) -> R<T> {
    Err(XPathError(m.into()))
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Slash,
    DoubleSlash,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Dot,
    DotDot,
    At,
    Comma,
    ColonColon,
    Pipe,
    Plus,
    Minus,
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
    Star,
    /// `*` as the multiplication operator.
    Mul,
    And,
    Or,
    Div,
    Mod,
    Literal(String),
    Number(f64),
    Variable(String),
    /// A name test: `name`, `prefix:name`, `prefix:*`.
    Name(String),
    /// A name followed by `(` (function or node type).
    Function(String),
    /// A name followed by `::`.
    Axis(String),
}

fn is_ncname_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ncname_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '\u{B7}')
}

fn tokenize(src: &str) -> R<Vec<Tok>> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out: Vec<Tok> = Vec::new();
    // Whether the previous token allows an operator to follow (XPath 3.7 lexical rules).
    let operand_before = |out: &Vec<Tok>| match out.last() {
        None => false,
        Some(t) => !matches!(
            t,
            Tok::At | Tok::ColonColon | Tok::LParen | Tok::LBracket | Tok::Comma | Tok::And | Tok::Or | Tok::Div | Tok::Mod | Tok::Mul | Tok::Slash | Tok::DoubleSlash | Tok::Pipe | Tok::Plus | Tok::Minus | Tok::Eq | Tok::Neq | Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge
        ),
    };
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let next = chars.get(i + 1).copied();
        let tok = match c {
            '/' if next == Some('/') => {
                i += 2;
                Tok::DoubleSlash
            }
            '/' => {
                i += 1;
                Tok::Slash
            }
            '(' => {
                i += 1;
                Tok::LParen
            }
            ')' => {
                i += 1;
                Tok::RParen
            }
            '[' => {
                i += 1;
                Tok::LBracket
            }
            ']' => {
                i += 1;
                Tok::RBracket
            }
            '.' if next == Some('.') => {
                i += 2;
                Tok::DotDot
            }
            '.' if !next.is_some_and(|n| n.is_ascii_digit()) => {
                i += 1;
                Tok::Dot
            }
            '@' => {
                i += 1;
                Tok::At
            }
            ',' => {
                i += 1;
                Tok::Comma
            }
            ':' if next == Some(':') => {
                i += 2;
                Tok::ColonColon
            }
            '|' => {
                i += 1;
                Tok::Pipe
            }
            '+' => {
                i += 1;
                Tok::Plus
            }
            '-' => {
                i += 1;
                Tok::Minus
            }
            '=' => {
                i += 1;
                Tok::Eq
            }
            '!' if next == Some('=') => {
                i += 2;
                Tok::Neq
            }
            '<' if next == Some('=') => {
                i += 2;
                Tok::Le
            }
            '<' => {
                i += 1;
                Tok::Lt
            }
            '>' if next == Some('=') => {
                i += 2;
                Tok::Ge
            }
            '>' => {
                i += 1;
                Tok::Gt
            }
            '*' => {
                i += 1;
                if operand_before(&out) { Tok::Mul } else { Tok::Star }
            }
            '"' | '\'' => {
                let start = i + 1;
                let Some(end) = chars[start..].iter().position(|x| *x == c) else { return err("misquoted literal... expected double quote!") };
                i = start + end + 1;
                Tok::Literal(chars[start..start + end].iter().collect())
            }
            '$' => {
                i += 1;
                let start = i;
                while i < chars.len() && (is_ncname_char(chars[i]) || chars[i] == ':') {
                    i += 1;
                }
                Tok::Variable(chars[start..i].iter().collect())
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i < chars.len() && chars[i] == '.' {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let s: String = chars[start..i].iter().collect();
                Tok::Number(s.parse().unwrap_or(f64::NAN))
            }
            c if is_ncname_start(c) => {
                let start = i;
                while i < chars.len() && is_ncname_char(chars[i]) {
                    i += 1;
                }
                let mut name: String = chars[start..i].iter().collect();
                if i + 1 < chars.len() && chars[i] == ':' && chars[i + 1] != ':' {
                    if chars[i + 1] == '*' {
                        name.push_str(":*");
                        i += 2;
                    } else if is_ncname_start(chars[i + 1]) {
                        i += 1;
                        let s2 = i;
                        while i < chars.len() && is_ncname_char(chars[i]) {
                            i += 1;
                        }
                        name.push(':');
                        name.extend(chars[s2..i].iter());
                    }
                }
                if operand_before(&out) && !name.contains(':') {
                    match name.as_str() {
                        "and" => {
                            out.push(Tok::And);
                            continue;
                        }
                        "or" => {
                            out.push(Tok::Or);
                            continue;
                        }
                        "div" => {
                            out.push(Tok::Div);
                            continue;
                        }
                        "mod" => {
                            out.push(Tok::Mod);
                            continue;
                        }
                        _ => {}
                    }
                }
                let mut j = i;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if chars.get(j) == Some(&'(') {
                    Tok::Function(name)
                } else if chars.get(j) == Some(&':') && chars.get(j + 1) == Some(&':') {
                    i = j + 2;
                    Tok::Axis(name)
                } else {
                    Tok::Name(name)
                }
            }
            other => return err(format!("Illegal character in XPath expression: {other}")),
        };
        out.push(tok);
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Axis {
    Ancestor,
    AncestorOrSelf,
    Attribute,
    Child,
    Descendant,
    DescendantOrSelf,
    Following,
    FollowingSibling,
    Namespace,
    Parent,
    Preceding,
    PrecedingSibling,
    SelfOnly,
}

#[derive(Debug, Clone, PartialEq)]
enum NodeTest {
    /// `*`, `prefix:*` (prefix resolved: always an error here), a name.
    Any,
    Name(String),
    Node,
    Text,
    Comment,
    Pi(Option<String>),
}

#[derive(Debug, Clone)]
struct Step {
    axis: Axis,
    test: NodeTest,
    predicates: Vec<Expr>,
}

#[derive(Debug, Clone)]
enum Expr {
    Or(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Cmp(Tok, Box<Expr>, Box<Expr>),
    Arith(Tok, Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Union(Box<Expr>, Box<Expr>),
    Literal(String),
    Number(f64),
    Function(String, Vec<Expr>),
    /// A filter expression with predicates, then an optional relative path.
    Filter(Box<Expr>, Vec<Expr>),
    Path { start: Option<Box<Expr>>, absolute: bool, steps: Vec<Step> },
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn expect(&mut self, t: Tok) -> R<()> {
        if self.peek() == Some(&t) {
            self.pos += 1;
            Ok(())
        } else {
            err(format!("Expected {t:?}, but found: {:?}", self.peek()))
        }
    }

    fn expr(&mut self) -> R<Expr> {
        let mut l = self.and()?;
        while self.peek() == Some(&Tok::Or) {
            self.pos += 1;
            l = Expr::Or(Box::new(l), Box::new(self.and()?));
        }
        Ok(l)
    }

    fn and(&mut self) -> R<Expr> {
        let mut l = self.equality()?;
        while self.peek() == Some(&Tok::And) {
            self.pos += 1;
            l = Expr::And(Box::new(l), Box::new(self.equality()?));
        }
        Ok(l)
    }

    fn equality(&mut self) -> R<Expr> {
        let mut l = self.relational()?;
        while let Some(t @ (Tok::Eq | Tok::Neq)) = self.peek().cloned() {
            self.pos += 1;
            l = Expr::Cmp(t, Box::new(l), Box::new(self.relational()?));
        }
        Ok(l)
    }

    fn relational(&mut self) -> R<Expr> {
        let mut l = self.additive()?;
        while let Some(t @ (Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge)) = self.peek().cloned() {
            self.pos += 1;
            l = Expr::Cmp(t, Box::new(l), Box::new(self.additive()?));
        }
        Ok(l)
    }

    fn additive(&mut self) -> R<Expr> {
        let mut l = self.multiplicative()?;
        while let Some(t @ (Tok::Plus | Tok::Minus)) = self.peek().cloned() {
            self.pos += 1;
            l = Expr::Arith(t, Box::new(l), Box::new(self.multiplicative()?));
        }
        Ok(l)
    }

    fn multiplicative(&mut self) -> R<Expr> {
        let mut l = self.unary()?;
        while let Some(t @ (Tok::Mul | Tok::Div | Tok::Mod)) = self.peek().cloned() {
            self.pos += 1;
            l = Expr::Arith(t, Box::new(l), Box::new(self.unary()?));
        }
        Ok(l)
    }

    fn unary(&mut self) -> R<Expr> {
        if self.peek() == Some(&Tok::Minus) {
            self.pos += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        let mut l = self.path()?;
        while self.peek() == Some(&Tok::Pipe) {
            self.pos += 1;
            l = Expr::Union(Box::new(l), Box::new(self.path()?));
        }
        Ok(l)
    }

    fn path(&mut self) -> R<Expr> {
        match self.peek() {
            Some(Tok::Slash) => {
                self.pos += 1;
                let steps = if self.starts_step() { self.relative()? } else { Vec::new() };
                Ok(Expr::Path { start: None, absolute: true, steps })
            }
            Some(Tok::DoubleSlash) => {
                self.pos += 1;
                let mut steps = vec![Step { axis: Axis::DescendantOrSelf, test: NodeTest::Node, predicates: Vec::new() }];
                steps.extend(self.relative()?);
                Ok(Expr::Path { start: None, absolute: true, steps })
            }
            Some(Tok::LParen | Tok::Literal(_) | Tok::Number(_) | Tok::Variable(_)) => self.filter_path(),
            Some(Tok::Function(n)) if !matches!(n.as_str(), "node" | "text" | "comment" | "processing-instruction") => self.filter_path(),
            _ => Ok(Expr::Path { start: None, absolute: false, steps: self.relative()? }),
        }
    }

    fn filter_path(&mut self) -> R<Expr> {
        let primary = self.primary()?;
        let mut predicates = Vec::new();
        while self.peek() == Some(&Tok::LBracket) {
            predicates.push(self.predicate()?);
        }
        let mut e = if predicates.is_empty() { primary } else { Expr::Filter(Box::new(primary), predicates) };
        match self.peek() {
            Some(Tok::Slash) => {
                self.pos += 1;
                e = Expr::Path { start: Some(Box::new(e)), absolute: false, steps: self.relative()? };
            }
            Some(Tok::DoubleSlash) => {
                self.pos += 1;
                let mut steps = vec![Step { axis: Axis::DescendantOrSelf, test: NodeTest::Node, predicates: Vec::new() }];
                steps.extend(self.relative()?);
                e = Expr::Path { start: Some(Box::new(e)), absolute: false, steps };
            }
            _ => {}
        }
        Ok(e)
    }

    fn primary(&mut self) -> R<Expr> {
        match self.next() {
            Some(Tok::LParen) => {
                let e = self.expr()?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Some(Tok::Literal(s)) => Ok(Expr::Literal(s)),
            Some(Tok::Number(n)) => Ok(Expr::Number(n)),
            Some(Tok::Variable(v)) => err(format!("Could not resolve variable: {v}")),
            Some(Tok::Function(name)) => {
                self.expect(Tok::LParen)?;
                let mut args = Vec::new();
                if self.peek() != Some(&Tok::RParen) {
                    args.push(self.expr()?);
                    while self.peek() == Some(&Tok::Comma) {
                        self.pos += 1;
                        args.push(self.expr()?);
                    }
                }
                self.expect(Tok::RParen)?;
                Ok(Expr::Function(name, args))
            }
            other => err(format!("Unexpected token: {other:?}")),
        }
    }

    fn starts_step(&self) -> bool {
        matches!(self.peek(), Some(Tok::Dot | Tok::DotDot | Tok::At | Tok::Star | Tok::Name(_) | Tok::Axis(_) | Tok::Function(_)))
    }

    fn relative(&mut self) -> R<Vec<Step>> {
        let mut steps = vec![self.step()?];
        loop {
            match self.peek() {
                Some(Tok::Slash) => {
                    self.pos += 1;
                    steps.push(self.step()?);
                }
                Some(Tok::DoubleSlash) => {
                    self.pos += 1;
                    steps.push(Step { axis: Axis::DescendantOrSelf, test: NodeTest::Node, predicates: Vec::new() });
                    steps.push(self.step()?);
                }
                _ => return Ok(steps),
            }
        }
    }

    fn step(&mut self) -> R<Step> {
        match self.peek() {
            Some(Tok::Dot) => {
                self.pos += 1;
                return Ok(Step { axis: Axis::SelfOnly, test: NodeTest::Node, predicates: Vec::new() });
            }
            Some(Tok::DotDot) => {
                self.pos += 1;
                return Ok(Step { axis: Axis::Parent, test: NodeTest::Node, predicates: Vec::new() });
            }
            _ => {}
        }
        let axis = match self.peek().cloned() {
            Some(Tok::At) => {
                self.pos += 1;
                Axis::Attribute
            }
            Some(Tok::Axis(a)) => {
                self.pos += 1;
                match a.as_str() {
                    "ancestor" => Axis::Ancestor,
                    "ancestor-or-self" => Axis::AncestorOrSelf,
                    "attribute" => Axis::Attribute,
                    "child" => Axis::Child,
                    "descendant" => Axis::Descendant,
                    "descendant-or-self" => Axis::DescendantOrSelf,
                    "following" => Axis::Following,
                    "following-sibling" => Axis::FollowingSibling,
                    "namespace" => Axis::Namespace,
                    "parent" => Axis::Parent,
                    "preceding" => Axis::Preceding,
                    "preceding-sibling" => Axis::PrecedingSibling,
                    "self" => Axis::SelfOnly,
                    other => return err(format!("illegal axis name: {other}")),
                }
            }
            _ => Axis::Child,
        };
        let test = match self.next() {
            Some(Tok::Star | Tok::Mul) => NodeTest::Any,
            Some(Tok::Name(n)) => {
                if let Some((p, _)) = n.split_once(':') {
                    return err(format!("Prefix must resolve to a namespace: {p}"));
                }
                NodeTest::Name(n)
            }
            Some(Tok::Function(f)) => {
                self.expect(Tok::LParen)?;
                let t = match f.as_str() {
                    "node" => NodeTest::Node,
                    "text" => NodeTest::Text,
                    "comment" => NodeTest::Comment,
                    "processing-instruction" => match self.peek().cloned() {
                        Some(Tok::Literal(l)) => {
                            self.pos += 1;
                            NodeTest::Pi(Some(l))
                        }
                        _ => NodeTest::Pi(None),
                    },
                    other => return err(format!("Unknown nodetype: {other}")),
                };
                self.expect(Tok::RParen)?;
                t
            }
            other => return err(format!("A location step was expected following the '/' or '//' token: {other:?}")),
        };
        let mut predicates = Vec::new();
        while self.peek() == Some(&Tok::LBracket) {
            predicates.push(self.predicate()?);
        }
        Ok(Step { axis, test, predicates })
    }

    fn predicate(&mut self) -> R<Expr> {
        self.expect(Tok::LBracket)?;
        let e = self.expr()?;
        self.expect(Tok::RBracket)?;
        Ok(e)
    }
}

#[derive(Debug, Clone)]
enum Value {
    Nodes(Vec<usize>),
    Bool(bool),
    Num(f64),
    Str(String),
}

struct Ctx<'a> {
    doc: &'a Document,
    node: usize,
    position: usize,
    size: usize,
}

struct Eval<'a> {
    doc: &'a Document,
}

fn is_ns_decl(d: &NodeData) -> bool {
    d.name == "xmlns" || d.prefix.as_deref() == Some("xmlns")
}

impl<'a> Eval<'a> {
    fn data(&self, id: usize) -> &NodeData {
        &self.doc.nodes[id]
    }

    fn is_text(&self, id: usize) -> bool {
        matches!(self.data(id).kind, TEXT_NODE | CDATA_SECTION_NODE)
    }

    /// Children as XPath sees them: a run of text nodes is its first node.
    fn children(&self, id: usize) -> Vec<usize> {
        let d = self.data(id);
        if !matches!(d.kind, ELEMENT_NODE | DOCUMENT_NODE) {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut prev_text = false;
        for &c in &d.children {
            let t = self.is_text(c);
            if t && prev_text {
                continue;
            }
            if self.data(c).kind != DOCUMENT_TYPE_NODE {
                out.push(c);
            }
            prev_text = t;
        }
        out
    }

    fn parent(&self, id: usize) -> Option<usize> {
        self.data(id).parent
    }

    fn string_value(&self, id: usize) -> String {
        let d = self.data(id);
        match d.kind {
            ELEMENT_NODE | DOCUMENT_NODE => {
                let mut out = String::new();
                self.descendant_text(id, &mut out);
                out
            }
            TEXT_NODE | CDATA_SECTION_NODE => {
                let mut out = String::new();
                let p = d.parent.expect("text has a parent");
                let kids = &self.data(p).children;
                let start = kids.iter().position(|c| *c == id).unwrap_or(0);
                for &c in &kids[start..] {
                    if !self.is_text(c) {
                        break;
                    }
                    out.push_str(&self.data(c).value);
                }
                out
            }
            _ => d.value.clone(),
        }
    }

    fn descendant_text(&self, id: usize, out: &mut String) {
        for &c in &self.data(id).children {
            let d = self.data(c);
            match d.kind {
                TEXT_NODE | CDATA_SECTION_NODE => out.push_str(&d.value),
                ELEMENT_NODE => self.descendant_text(c, out),
                _ => {}
            }
        }
    }

    fn descendants(&self, id: usize, out: &mut Vec<usize>) {
        for c in self.children(id) {
            out.push(c);
            self.descendants(c, out);
        }
    }

    fn attributes(&self, id: usize) -> Vec<usize> {
        self.data(id).attributes.iter().copied().filter(|a| !is_ns_decl(self.data(*a))).collect()
    }

    fn namespaces(&self, id: usize) -> Vec<usize> {
        if self.data(id).kind != ELEMENT_NODE {
            return Vec::new();
        }
        let mut seen: Vec<String> = Vec::new();
        let mut out = Vec::new();
        let mut cur = Some(id);
        while let Some(e) = cur {
            if self.data(e).kind != ELEMENT_NODE {
                break;
            }
            for &a in &self.data(e).attributes {
                let d = self.data(a);
                if is_ns_decl(d) {
                    let prefix = if d.name == "xmlns" { String::new() } else { d.local_name.clone().unwrap_or_default() };
                    if !seen.contains(&prefix) {
                        seen.push(prefix);
                        if !d.value.is_empty() {
                            out.push(a);
                        }
                    }
                }
            }
            cur = self.parent(e);
        }
        out
    }

    fn siblings(&self, id: usize) -> (Vec<usize>, Vec<usize>) {
        let d = self.data(id);
        if matches!(d.kind, ATTRIBUTE_NODE) {
            return (Vec::new(), Vec::new());
        }
        let Some(p) = d.parent else { return (Vec::new(), Vec::new()) };
        let kids = self.children(p);
        let i = kids.iter().position(|c| *c == id).unwrap_or(0);
        (kids[..i].iter().rev().copied().collect(), kids[i + 1..].to_vec())
    }

    fn axis(&self, axis: Axis, id: usize) -> Vec<usize> {
        match axis {
            Axis::SelfOnly => vec![id],
            Axis::Child => self.children(id),
            Axis::Attribute => {
                if self.data(id).kind == ELEMENT_NODE {
                    self.attributes(id)
                } else {
                    Vec::new()
                }
            }
            Axis::Namespace => self.namespaces(id),
            Axis::Parent => self.parent(id).into_iter().collect(),
            Axis::Ancestor | Axis::AncestorOrSelf => {
                let mut out = if axis == Axis::AncestorOrSelf { vec![id] } else { Vec::new() };
                let mut cur = self.parent(id);
                while let Some(p) = cur {
                    out.push(p);
                    cur = self.parent(p);
                }
                out
            }
            Axis::Descendant | Axis::DescendantOrSelf => {
                let mut out = if axis == Axis::DescendantOrSelf { vec![id] } else { Vec::new() };
                self.descendants(id, &mut out);
                out
            }
            Axis::FollowingSibling => self.siblings(id).1,
            Axis::PrecedingSibling => self.siblings(id).0,
            Axis::Following => {
                let mut out = Vec::new();
                let mut cur = if self.data(id).kind == ATTRIBUTE_NODE { self.parent(id).map(|p| (p, true)) } else { Some((id, false)) };
                while let Some((n, from_attr)) = cur {
                    if from_attr {
                        // An attribute's following nodes start with its element's descendants.
                        self.descendants(n, &mut out);
                    }
                    for s in self.siblings(n).1 {
                        out.push(s);
                        self.descendants(s, &mut out);
                    }
                    cur = self.parent(n).map(|p| (p, false));
                }
                out
            }
            Axis::Preceding => {
                let target = if self.data(id).kind == ATTRIBUTE_NODE { self.parent(id).unwrap_or(id) } else { id };
                let mut ancestors = Vec::new();
                let mut cur = self.parent(target);
                while let Some(p) = cur {
                    ancestors.push(p);
                    cur = self.parent(p);
                }
                let mut all = Vec::new();
                self.descendants(0, &mut all);
                let order = |n: usize| n;
                let mut out: Vec<usize> = all.into_iter().filter(|n| order(*n) < order(target) && !ancestors.contains(n)).collect();
                out.reverse();
                out
            }
        }
    }

    fn principal(axis: Axis) -> i32 {
        match axis {
            Axis::Attribute => ATTRIBUTE_NODE,
            Axis::Namespace => -1,
            _ => ELEMENT_NODE,
        }
    }

    fn matches(&self, test: &NodeTest, axis: Axis, id: usize) -> bool {
        let d = self.data(id);
        match test {
            NodeTest::Node => true,
            NodeTest::Text => matches!(d.kind, TEXT_NODE | CDATA_SECTION_NODE),
            NodeTest::Comment => d.kind == COMMENT_NODE,
            NodeTest::Pi(target) => d.kind == PROCESSING_INSTRUCTION_NODE && target.as_ref().is_none_or(|t| *t == d.name),
            NodeTest::Any => {
                if axis == Axis::Namespace {
                    true
                } else {
                    d.kind == Self::principal(axis)
                }
            }
            NodeTest::Name(n) => {
                if axis == Axis::Namespace {
                    let prefix = if d.name == "xmlns" { "" } else { d.local_name.as_deref().unwrap_or("") };
                    return prefix == n;
                }
                d.kind == Self::principal(axis) && d.namespace_uri.is_none() && d.local_name.as_deref().unwrap_or(&d.name) == n
            }
        }
    }


    fn step(&self, step: &Step, input: &[usize]) -> R<Vec<usize>> {
        let mut out: Vec<usize> = Vec::new();
        for &n in input {
            let mut selected: Vec<usize> = self.axis(step.axis, n).into_iter().filter(|c| self.matches(&step.test, step.axis, *c)).collect();
            for p in &step.predicates {
                let size = selected.len();
                let mut kept = Vec::new();
                for (i, c) in selected.iter().enumerate() {
                    let ctx = Ctx { doc: self.doc, node: *c, position: i + 1, size };
                    if self.predicate_true(p, &ctx)? {
                        kept.push(*c);
                    }
                }
                selected = kept;
            }
            out.extend(selected);
        }
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    fn predicate_true(&self, p: &Expr, ctx: &Ctx) -> R<bool> {
        Ok(match self.eval(p, ctx)? {
            Value::Num(n) => n == ctx.position as f64,
            other => self.boolean(&other),
        })
    }

    fn boolean(&self, v: &Value) -> bool {
        match v {
            Value::Nodes(n) => !n.is_empty(),
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0 && !n.is_nan(),
            Value::Str(s) => !s.is_empty(),
        }
    }

    fn string(&self, v: &Value) -> String {
        match v {
            Value::Nodes(n) => n.first().map(|id| self.string_value(*id)).unwrap_or_default(),
            Value::Bool(b) => b.to_string(),
            Value::Num(n) => number_to_string(*n),
            Value::Str(s) => s.clone(),
        }
    }

    fn number(&self, v: &Value) -> f64 {
        match v {
            Value::Num(n) => *n,
            Value::Bool(b) => *b as i32 as f64,
            other => string_to_number(&self.string(other)),
        }
    }

    fn nodes(&self, v: Value) -> R<Vec<usize>> {
        match v {
            Value::Nodes(n) => Ok(n),
            other => err(format!("Can not convert #{} to a NodeList!", type_name(&other))),
        }
    }

    fn compare(&self, op: &Tok, l: &Value, r: &Value) -> bool {
        let atom = |a: &Value, b: &Value| -> bool {
            if matches!(op, Tok::Eq | Tok::Neq) {
                let eq = if matches!(a, Value::Bool(_)) || matches!(b, Value::Bool(_)) {
                    self.boolean(a) == self.boolean(b)
                } else if matches!(a, Value::Num(_)) || matches!(b, Value::Num(_)) {
                    self.number(a) == self.number(b)
                } else {
                    self.string(a) == self.string(b)
                };
                return if *op == Tok::Eq { eq } else { !eq };
            }
            let (x, y) = (self.number(a), self.number(b));
            match op {
                Tok::Lt => x < y,
                Tok::Le => x <= y,
                Tok::Gt => x > y,
                _ => x >= y,
            }
        };
        match (l, r) {
            (Value::Nodes(a), Value::Nodes(b)) => a.iter().any(|x| b.iter().any(|y| atom(&Value::Str(self.string_value(*x)), &Value::Str(self.string_value(*y))))),
            (Value::Nodes(a), other) => match other {
                Value::Bool(_) => atom(&Value::Bool(!a.is_empty()), other),
                Value::Num(_) => a.iter().any(|x| atom(&Value::Num(string_to_number(&self.string_value(*x))), other)),
                _ => a.iter().any(|x| atom(&Value::Str(self.string_value(*x)), other)),
            },
            (other, Value::Nodes(b)) => match other {
                Value::Bool(_) => atom(other, &Value::Bool(!b.is_empty())),
                Value::Num(_) => b.iter().any(|y| atom(other, &Value::Num(string_to_number(&self.string_value(*y))))),
                _ => b.iter().any(|y| atom(other, &Value::Str(self.string_value(*y)))),
            },
            (a, b) => atom(a, b),
        }
    }

    fn eval(&self, e: &Expr, ctx: &Ctx) -> R<Value> {
        Ok(match e {
            Expr::Or(l, r) => Value::Bool(self.boolean(&self.eval(l, ctx)?) || self.boolean(&self.eval(r, ctx)?)),
            Expr::And(l, r) => Value::Bool(self.boolean(&self.eval(l, ctx)?) && self.boolean(&self.eval(r, ctx)?)),
            Expr::Cmp(op, l, r) => {
                let (a, b) = (self.eval(l, ctx)?, self.eval(r, ctx)?);
                Value::Bool(self.compare(op, &a, &b))
            }
            Expr::Arith(op, l, r) => {
                let (a, b) = (self.number(&self.eval(l, ctx)?), self.number(&self.eval(r, ctx)?));
                Value::Num(match op {
                    Tok::Plus => a + b,
                    Tok::Minus => a - b,
                    Tok::Mul => a * b,
                    Tok::Div => a / b,
                    _ => a % b,
                })
            }
            Expr::Neg(x) => Value::Num(-self.number(&self.eval(x, ctx)?)),
            Expr::Union(l, r) => {
                let mut a = self.nodes(self.eval(l, ctx)?)?;
                a.extend(self.nodes(self.eval(r, ctx)?)?);
                a.sort_unstable();
                a.dedup();
                Value::Nodes(a)
            }
            Expr::Literal(s) => Value::Str(s.clone()),
            Expr::Number(n) => Value::Num(*n),
            Expr::Function(name, args) => self.function(name, args, ctx)?,
            Expr::Filter(primary, predicates) => {
                let mut nodes = self.nodes(self.eval(primary, ctx)?)?;
                for p in predicates {
                    let size = nodes.len();
                    let mut kept = Vec::new();
                    for (i, n) in nodes.iter().enumerate() {
                        if self.predicate_true(p, &Ctx { doc: self.doc, node: *n, position: i + 1, size })? {
                            kept.push(*n);
                        }
                    }
                    nodes = kept;
                }
                Value::Nodes(nodes)
            }
            Expr::Path { start, absolute, steps } => {
                let mut nodes = match start {
                    Some(s) => self.nodes(self.eval(s, ctx)?)?,
                    None if *absolute => vec![0],
                    None => vec![ctx.node],
                };
                for s in steps {
                    nodes = self.step(s, &nodes)?;
                }
                Value::Nodes(nodes)
            }
        })
    }

    fn function(&self, name: &str, args: &[Expr], ctx: &Ctx) -> R<Value> {
        let arity = |min: usize, max: usize| -> R<()> {
            if args.len() < min || args.len() > max {
                return err(format!("FuncName: {name}: wrong number of arguments"));
            }
            Ok(())
        };
        let arg = |i: usize| self.eval(&args[i], ctx);
        let str_arg = |i: usize| -> R<String> {
            if args.len() > i { Ok(self.string(&self.eval(&args[i], ctx)?)) } else { Ok(self.string_value(ctx.node)) }
        };
        let node_arg = |i: usize| -> R<Option<usize>> {
            if args.len() > i { Ok(self.nodes(self.eval(&args[i], ctx)?)?.first().copied()) } else { Ok(Some(ctx.node)) }
        };
        Ok(match name {
            "last" => {
                arity(0, 0)?;
                Value::Num(ctx.size as f64)
            }
            "position" => {
                arity(0, 0)?;
                Value::Num(ctx.position as f64)
            }
            "count" => {
                arity(1, 1)?;
                Value::Num(self.nodes(arg(0)?)?.len() as f64)
            }
            "id" => {
                arity(1, 1)?;
                Value::Nodes(Vec::new())
            }
            "local-name" | "name" | "namespace-uri" => {
                arity(0, 1)?;
                let Some(n) = node_arg(0)? else { return Ok(Value::Str(String::new())) };
                let d = self.data(n);
                Value::Str(match (name, d.kind) {
                    (_, ELEMENT_NODE | ATTRIBUTE_NODE) if is_ns_decl(d) && name != "namespace-uri" => {
                        if d.name == "xmlns" {
                            String::new()
                        } else {
                            d.local_name.clone().unwrap_or_default()
                        }
                    }
                    (_, ELEMENT_NODE | ATTRIBUTE_NODE) if is_ns_decl(d) => String::new(),
                    ("name", ELEMENT_NODE | ATTRIBUTE_NODE) => d.name.clone(),
                    ("local-name", ELEMENT_NODE | ATTRIBUTE_NODE) => d.local_name.clone().unwrap_or_else(|| d.name.clone()),
                    ("namespace-uri", ELEMENT_NODE | ATTRIBUTE_NODE) => d.namespace_uri.clone().unwrap_or_default(),
                    ("name" | "local-name", PROCESSING_INSTRUCTION_NODE) => d.name.clone(),
                    _ => String::new(),
                })
            }
            "string" => {
                arity(0, 1)?;
                Value::Str(str_arg(0)?)
            }
            "concat" => {
                if args.len() < 2 {
                    return err("FuncName: concat: wrong number of arguments");
                }
                let mut out = String::new();
                for i in 0..args.len() {
                    out.push_str(&str_arg(i)?);
                }
                Value::Str(out)
            }
            "starts-with" => {
                arity(2, 2)?;
                Value::Bool(str_arg(0)?.starts_with(&str_arg(1)?))
            }
            "contains" => {
                arity(2, 2)?;
                Value::Bool(str_arg(0)?.contains(&str_arg(1)?))
            }
            "substring-before" => {
                arity(2, 2)?;
                let (s, t) = (str_arg(0)?, str_arg(1)?);
                Value::Str(s.find(&t).map(|i| s[..i].to_string()).unwrap_or_default())
            }
            "substring-after" => {
                arity(2, 2)?;
                let (s, t) = (str_arg(0)?, str_arg(1)?);
                Value::Str(s.find(&t).map(|i| s[i + t.len()..].to_string()).unwrap_or_default())
            }
            "substring" => {
                arity(2, 3)?;
                let s: Vec<char> = str_arg(0)?.chars().collect();
                let start = round(self.number(&arg(1)?));
                let end = if args.len() == 3 { start + round(self.number(&arg(2)?)) } else { f64::INFINITY };
                let out: String = s.iter().enumerate().filter(|(i, _)| {
                    let p = (*i + 1) as f64;
                    p >= start && p < end
                }).map(|(_, c)| *c).collect();
                Value::Str(out)
            }
            "string-length" => {
                arity(0, 1)?;
                Value::Num(str_arg(0)?.chars().count() as f64)
            }
            "normalize-space" => {
                arity(0, 1)?;
                Value::Str(str_arg(0)?.split([' ', '\t', '\n', '\r']).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" "))
            }
            "translate" => {
                arity(3, 3)?;
                let (s, from, to) = (str_arg(0)?, str_arg(1)?, str_arg(2)?);
                let from: Vec<char> = from.chars().collect();
                let to: Vec<char> = to.chars().collect();
                Value::Str(s.chars().filter_map(|c| match from.iter().position(|f| *f == c) {
                    Some(i) => to.get(i).copied(),
                    None => Some(c),
                }).collect())
            }
            "boolean" => {
                arity(1, 1)?;
                Value::Bool(self.boolean(&arg(0)?))
            }
            "not" => {
                arity(1, 1)?;
                Value::Bool(!self.boolean(&arg(0)?))
            }
            "true" => {
                arity(0, 0)?;
                Value::Bool(true)
            }
            "false" => {
                arity(0, 0)?;
                Value::Bool(false)
            }
            "lang" => {
                arity(1, 1)?;
                let want = str_arg(0)?.to_lowercase();
                let mut cur = Some(ctx.node);
                let mut found = None;
                while let Some(n) = cur {
                    if let Some(a) = self.data(n).attributes.iter().find(|a| self.data(**a).name == "xml:lang") {
                        found = Some(self.data(*a).value.to_lowercase());
                        break;
                    }
                    cur = self.parent(n);
                }
                Value::Bool(found.is_some_and(|l| l == want || l.starts_with(&format!("{want}-"))))
            }
            "number" => {
                arity(0, 1)?;
                if args.is_empty() { Value::Num(string_to_number(&self.string_value(ctx.node))) } else { Value::Num(self.number(&arg(0)?)) }
            }
            "sum" => {
                arity(1, 1)?;
                Value::Num(self.nodes(arg(0)?)?.iter().map(|n| string_to_number(&self.string_value(*n))).sum())
            }
            "floor" => {
                arity(1, 1)?;
                Value::Num(self.number(&arg(0)?).floor())
            }
            "ceiling" => {
                arity(1, 1)?;
                Value::Num(self.number(&arg(0)?).ceil())
            }
            "round" => {
                arity(1, 1)?;
                Value::Num(round(self.number(&arg(0)?)))
            }
            other => return err(format!("Could not find function: {other}")),
        })
    }
}

/// XPath `round`: half up, `NaN` and infinities unchanged.
fn round(n: f64) -> f64 {
    if n.is_nan() || n.is_infinite() {
        return n;
    }
    if n == 0.0 {
        return n;
    }
    let r = (n + 0.5).floor();
    if r == 0.0 && n < 0.0 { -0.0 } else { r }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Nodes(_) => "NODESET",
        Value::Bool(_) => "BOOLEAN",
        Value::Num(_) => "NUMBER",
        Value::Str(_) => "STRING",
    }
}

/// XPath `number(string)`: optional whitespace, an optional minus, digits with an optional point.
pub fn string_to_number(s: &str) -> f64 {
    let t = s.trim_matches([' ', '\t', '\n', '\r']);
    let body = t.strip_prefix('-').unwrap_or(t);
    let valid = !body.is_empty() && body.chars().all(|c| c.is_ascii_digit() || c == '.') && body.chars().filter(|c| *c == '.').count() <= 1 && body != ".";
    if !valid {
        return f64::NAN;
    }
    t.parse().unwrap_or(f64::NAN)
}

/// XPath `string(number)`: integers without a point, no exponent notation.
pub fn number_to_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if n == n.trunc() && n.abs() < 1e18 {
        return format!("{}", n as i64);
    }
    let s = java_compat::double_to_string(n);
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r.to_string()),
        None => (false, s),
    };
    let (mantissa, exp) = match s.split_once('E') {
        Some((m, e)) => (m.to_string(), e.parse::<i32>().unwrap_or(0)),
        None => (s.clone(), 0),
    };
    let (int, frac) = mantissa.split_once('.').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or((mantissa.clone(), String::new()));
    let digits = format!("{int}{frac}");
    let point = int.len() as i32 + exp;
    let mut out = if point <= 0 {
        format!("0.{}{}", "0".repeat((-point) as usize), digits)
    } else if point as usize >= digits.len() {
        format!("{}{}", digits, "0".repeat(point as usize - digits.len()))
    } else {
        format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
    };
    if out.contains('.') {
        while out.ends_with('0') {
            out.pop();
        }
        if out.ends_with('.') {
            out.pop();
        }
    }
    let out = out.trim_start_matches('0');
    let out = if out.is_empty() || out.starts_with('.') { format!("0{out}") } else { out.to_string() };
    if neg { format!("-{out}") } else { out }
}

/// `XPath.evaluate(expression, context, NODESET)`: the selected nodes in document order.
pub fn select(expression: &str, context: &Node) -> Result<Vec<Node>, XPathError> {
    let toks = tokenize(expression)?;
    if toks.is_empty() {
        return err("Empty expression!");
    }
    let mut p = Parser { toks, pos: 0 };
    let e = p.expr()?;
    if p.pos < p.toks.len() {
        return err(format!("Extra illegal tokens: {:?}", &p.toks[p.pos..]));
    }
    let ev = Eval { doc: &context.doc };
    let ctx = Ctx { doc: &context.doc, node: context.id, position: 1, size: 1 };
    let nodes = ev.nodes(ev.eval(&e, &ctx)?)?;
    let _ = ctx.doc;
    Ok(nodes.into_iter().map(|id| Node { doc: Arc::clone(&context.doc), id }).collect())
}
