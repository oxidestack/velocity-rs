//! The productions of `Parser.jjt`, with their lookaheads, building the tree [`crate::ast`] describes. Tokens are
//! fetched lazily, exactly when JavaCC's generated parser would fetch them, because some productions switch the
//! lexer's state and peek at the raw character stream.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use java_compat::lang::JValue;

use crate::ast::*;
use crate::lexer::{Kind, Lexer, State, Token};
use crate::VelocityError;

/// A directive's `getType()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectiveType {
    /// `BLOCK`: the directive has a body up to `#end`.
    Block,
    /// `LINE`: the directive has no body.
    Line,
}

/// The built-in directives (`directive.properties`).
pub fn builtin_directive_type(name: &str) -> Option<DirectiveType> {
    match name {
        "foreach" | "macro" | "define" => Some(DirectiveType::Block),
        "include" | "parse" | "evaluate" | "break" | "stop" => Some(DirectiveType::Line),
        _ => None,
    }
}

/// What the parser asks the runtime (`rsvc`): the configuration it depends on, the directives and the macros.
pub struct Settings<'a> {
    pub hyphen_allowed: bool,
    /// `space.gobbling = bc`, which changes some lookaheads.
    pub bc: bool,
    pub gobbling: crate::config::SpaceGobbling,
    /// `runtime.strict_mode.escape`: a backslash before any `#name` is absorbed.
    pub strict_escape: bool,
    pub directive: &'a dyn Fn(&str) -> Option<DirectiveType>,
    pub is_macro: &'a dyn Fn(&str) -> bool,
}

static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

/// How often a parse failure could not be rebuilt by [`crate::jj`] and lost its expected-token list.
pub static JJ_FALLBACKS: AtomicUsize = AtomicUsize::new(0);

pub struct Parser<'a> {
    lex: Lexer,
    /// The template text, for [`crate::jj`] to replay when parsing fails.
    source: String,
    /// Every token fetched so far; `cur` indexes the next one to consume.
    toks: Vec<Token>,
    cur: usize,
    /// Macros defined earlier in this template (`macroNames`), for escape processing.
    macro_names: HashSet<String>,
    settings: &'a Settings<'a>,
    /// For an interpolated string literal: its line and column in the enclosing template (`adjTokenLineNums`).
    offset: Option<(usize, usize)>,
    /// The first interpolated string literal that did not parse: `ASTStringLiteral.init` raises it once the whole
    /// template has parsed, so a syntax error elsewhere in the template comes first.
    deferred: Option<VelocityError>,
    /// The message of the last [`Parser::encountered`] failure and the token count it happened at: the failure the
    /// parse ends with, unless a speculative parse dropped it, gets its expected tokens from [`crate::jj`] then.
    pending: Option<(String, usize)>,
}

type R<T> = Result<T, VelocityError>;

/// `TemplateParseException.getMessage()` for the token the parser could not use.
fn encountered(image: &str, line: usize, column: usize) -> VelocityError {
    let image = if image.is_empty() { "<EOF>".to_string() } else { crate::error::add_escapes(image) };
    VelocityError::parse_at(format!("Encountered \"{image}\" at {}\nWas expecting one of:\n    ", crate::error::LOCATION), line as i32, column as i32)
}

fn is_ws(k: Kind) -> bool {
    matches!(k, Kind::Whitespace | Kind::Newline)
}

fn is_identifier(k: Kind) -> bool {
    matches!(k, Kind::Identifier | Kind::OldIdentifier)
}

/// `NodeUtils.tokenLiteral`: a token's image with the `$`, `#` and escaped `$` of the special tokens before it.
fn token_literal(t: &Token) -> String {
    if t.kind == Kind::MultiLineComment {
        return String::new();
    }
    if t.specials.is_empty() || t.specials.last().is_some_and(|s| s.starts_with("##")) {
        return t.image.clone();
    }
    let mut special = String::new();
    for st in &t.specials {
        let chars: Vec<char> = st.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '#' || c == '$' {
                special.push(c);
            }
            if c == '\\' {
                let mut j = i;
                let mut term = false;
                while j < chars.len() {
                    let cc = chars[j];
                    j += 1;
                    if cc == '\\' {
                        continue;
                    }
                    term = cc == '$';
                    break;
                }
                if term {
                    special.extend(&chars[i..j]);
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
    }
    if special.is_empty() { t.image.clone() } else { special + &t.image }
}

/// The text before the last `#` of a token image: `$`s and `#`s a `MORE` rule prefixed.
fn hash_prefix(image: &str) -> String {
    match image.rfind('#') {
        Some(p) if p > 0 => image[..p].to_string(),
        _ => String::new(),
    }
}

impl<'a> Parser<'a> {
    pub fn new(source: &str, settings: &'a Settings<'a>) -> Parser<'a> {
        Parser { lex: Lexer::new(source, settings.hyphen_allowed), source: source.to_string(), toks: Vec::new(), cur: 0, macro_names: HashSet::new(), settings, offset: None, deferred: None, pending: None }
    }

    /// A parser for an interpolated string literal at `line`, `column`: its tokens move by the literal's line, and
    /// those on its first line also by the literal's column.
    pub fn nested(source: &str, settings: &'a Settings<'a>, line: usize, column: usize) -> Parser<'a> {
        Parser { offset: Some((line, column)), ..Parser::new(source, settings) }
    }

    fn directive_type(&self, name: &str) -> Option<DirectiveType> {
        (self.settings.directive)(name)
    }

    /// `rsvc.isVelocimacro(name)`: macros already known to the engine.
    fn is_macro(&self, name: &str) -> bool {
        (self.settings.is_macro)(name)
    }

    fn peek(&mut self, k: usize) -> R<&Token> {
        while self.toks.len() < self.cur + k {
            let mut t = self.lex.next_token()?;
            if let Some((line, column)) = self.offset {
                if t.line == 1 {
                    t.column += column;
                }
                t.line += line - 1;
            }
            self.toks.push(t);
        }
        Ok(&self.toks[self.cur + k - 1])
    }

    fn kind(&mut self, k: usize) -> R<Kind> {
        Ok(self.peek(k)?.kind)
    }

    fn consume(&mut self, kind: Kind) -> R<usize> {
        let t = self.peek(1)?;
        if t.kind != kind {
            let (line, column, image) = (t.line, t.column, t.image.clone());
            return Err(self.encountered(&image, line, column));
        }
        self.cur += 1;
        Ok(self.cur - 1)
    }

    fn consume_any(&mut self) -> usize {
        self.cur += 1;
        self.cur - 1
    }

    fn image(&self, i: usize) -> String {
        self.toks[i].image.clone()
    }

    fn pos(&self, i: usize) -> Pos {
        Pos { line: self.toks[i].line as i32, column: self.toks[i].column as i32 }
    }

    fn fail(&mut self) -> VelocityError {
        match self.peek(1) {
            Ok(t) => {
                let (image, line, column) = (t.image.clone(), t.line, t.column);
                self.encountered(&image, line, column)
            }
            Err(e) => e,
        }
    }

    /// A `ParseException` at the token the parser could not use. Speculative parses (look-aheads written as a trial
    /// parse) drop these, so the expected tokens are added only to the failure the parse ends with
    /// ([`Parser::process`]).
    fn encountered(&mut self, image: &str, line: usize, column: usize) -> VelocityError {
        let e = encountered(image, line, column);
        self.pending = Some((e.message.clone(), self.cur));
        e
    }

    /// The failure the generated parser throws where `e` was raised: the replay through [`crate::jj`] when it agrees
    /// with this parser on where it happened, else `e`, whose message lacks the expected tokens.
    fn with_expected_tokens(&mut self, e: VelocityError, consumed: usize) -> VelocityError {
        match crate::jj::parse_exception(&self.source, self.settings, self.offset) {
            Some(crate::jj::Outcome::Exception(f)) if f.consumed == consumed => VelocityError::parse_at(f.message, f.line as i32, f.column as i32),
            // A look-ahead of the generated parser lexed ahead into a token the lexer rejects, before this failure.
            Some(crate::jj::Outcome::Lexical(le)) => le,
            _ => {
                JJ_FALLBACKS.fetch_add(1, Ordering::Relaxed);
                e
            }
        }
    }

    fn skip_ws(&mut self) -> R<()> {
        while is_ws(self.kind(1)?) {
            self.consume_any();
        }
        Ok(())
    }

    /// `[ <WHITESPACE> ] <NEWLINE>` under `LOOKAHEAD(2)`: the consumed text.
    fn optional_line_end(&mut self) -> R<Option<String>> {
        let k1 = self.kind(1)?;
        if k1 == Kind::Newline {
            let i = self.consume_any();
            return Ok(Some(self.image(i)));
        }
        if k1 == Kind::Whitespace && self.kind(2)? == Kind::Newline {
            let a = self.consume_any();
            let b = self.consume_any();
            return Ok(Some(self.image(a) + &self.image(b)));
        }
        Ok(None)
    }

    /// `process()`.
    pub fn process(mut self) -> R<Block> {
        match self.process_statements() {
            Err(e) if self.pending.as_ref().is_some_and(|(m, _)| *m == e.message) => {
                let consumed = self.pending.as_ref().map_or(0, |(_, c)| *c);
                Err(self.with_expected_tokens(e, consumed))
            }
            r => r,
        }
    }

    fn process_statements(&mut self) -> R<Block> {
        let mut after_newline = true;
        let mut nodes = Vec::new();
        while self.kind(1)? != Kind::Eof {
            let (node, nl) = self.statement(after_newline)?;
            after_newline = nl;
            if let Some(n) = node {
                nodes.push(n);
            }
        }
        if let Some(e) = self.deferred.take() {
            return Err(e);
        }
        Ok(Block { nodes, ..Block::default() })
    }

    fn text_node(&self, first: usize, last: usize) -> Node {
        Node::Text(self.toks[first..=last].iter().map(token_literal).collect())
    }

    /// `Statement(afterNewline)`: the node (none for a trailing zero-width whitespace) and the new `afterNewline`.
    fn statement(&mut self, after_newline: bool) -> R<(Option<Node>, bool)> {
        let k1 = self.kind(1)?;
        if k1 == Kind::IfDirective || after_newline && k1 == Kind::Whitespace && self.kind(2)? == Kind::IfDirective {
            let (n, nl) = self.if_statement(after_newline)?;
            return Ok((Some(n), nl));
        }
        if self.lookahead_reference()? {
            let r = self.reference()?;
            return Ok((Some(Node::Reference(Box::new(r))), false));
        }
        if self.lookahead_comment()? {
            let (text, nl) = self.comment()?;
            return Ok((Some(Node::Text(text)), nl));
        }
        let k1 = self.kind(1)?;
        if k1 == Kind::Textblock {
            let i = self.consume_any();
            let image = self.image(i);
            let inner = image.strip_prefix("#[[").and_then(|s| s.strip_suffix("]]#")).unwrap_or_default().to_string();
            // `MORE` prefixes before `#[[` are part of the image but not of the text block.
            let inner = match image.find("#[[") {
                Some(p) if p > 0 => image[p + 3..image.len() - 3].to_string(),
                _ => inner,
            };
            return Ok((Some(Node::Text(inner)), false));
        }
        if k1 == Kind::SetDirective || after_newline && k1 == Kind::Whitespace && self.kind(2)? == Kind::SetDirective {
            let (n, nl) = self.set_directive(after_newline)?;
            return Ok((Some(n), nl));
        }
        if k1 == Kind::EscapeDirective {
            let i = self.consume_any();
            return Ok((Some(Node::Text(self.escaped_directive(&self.image(i)))), false));
        }
        if k1 == Kind::DoubleEscape {
            return Ok((Some(Node::Text(self.escape()?)), false));
        }
        let word = |k: Kind| matches!(k, Kind::Word | Kind::BracketedWord);
        if word(k1) || after_newline && k1 == Kind::Whitespace && word(self.kind(2)?) {
            let (n, nl) = self.directive(after_newline)?;
            return Ok((Some(n), nl));
        }
        match k1 {
            Kind::Text => {
                let i = self.consume_any();
                Ok((Some(self.text_node(i, i)), true))
            }
            Kind::Dot
            | Kind::RParen
            | Kind::LParen
            | Kind::IntegerLiteral
            | Kind::FloatingPointLiteral
            | Kind::StringLiteral
            | Kind::Escape
            | Kind::LCurly
            | Kind::RCurly
            | Kind::EmptyIndex
            | Kind::Pipe => {
                let i = self.consume_any();
                Ok((Some(self.text_node(i, i)), false))
            }
            Kind::LoneSymbol => {
                let i = self.consume_any();
                let mut text = token_literal(&self.toks[i]);
                text.pop();
                Ok((Some(Node::Text(text)), false))
            }
            Kind::Newline => {
                let i = self.consume_any();
                Ok((Some(self.text_node(i, i)), true))
            }
            Kind::InlineText => {
                let first = self.consume_any();
                let mut last = first;
                let mut nl = false;
                if self.kind(1)? == Kind::Text {
                    last = self.consume_any();
                    nl = true;
                }
                Ok((Some(self.text_node(first, last)), nl))
            }
            Kind::Whitespace => {
                let i = self.consume_any();
                Ok((Some(self.text_node(i, i)), false))
            }
            Kind::Suffix => {
                let i = self.consume_any();
                Ok((Some(self.text_node(i, i)), true))
            }
            Kind::ZeroWidthWhitespace if self.kind(2)? == Kind::Eof => {
                self.consume_any();
                self.consume_any();
                Ok((None, after_newline))
            }
            Kind::LogicalOr2 => {
                let i = self.consume_any();
                Ok((Some(self.text_node(i, i)), after_newline))
            }
            Kind::ZeroWidthWhitespace => {
                let i = self.consume_any();
                Ok((Some(self.text_node(i, i)), false))
            }
            _ => Err(self.fail()),
        }
    }

    /// `LOOKAHEAD(2) Reference()`.
    fn lookahead_reference(&mut self) -> R<bool> {
        let k1 = self.kind(1)?;
        if is_identifier(k1) {
            // The scan also reads the next token, looking for an index or a dot.
            self.peek(2)?;
            return Ok(true);
        }
        Ok(k1 == Kind::LCurly && is_identifier(self.kind(2)?))
    }

    /// `LOOKAHEAD(2) Comment()`.
    fn lookahead_comment(&mut self) -> R<bool> {
        match self.kind(1)? {
            Kind::SingleLineCommentStart => {
                self.peek(2)?;
                Ok(true)
            }
            Kind::MultiLineComment | Kind::FormalComment => Ok(true),
            _ => Ok(false),
        }
    }

    /// `Comment()`: the text before the comment marker (`MORE` prefixes), and whether it ended a line.
    fn comment(&mut self) -> R<(String, bool)> {
        let k = self.kind(1)?;
        let i = self.consume_any();
        let image = self.image(i);
        let nl = if k == Kind::SingleLineCommentStart {
            if self.kind(1)? == Kind::SingleLineComment {
                self.consume_any();
            }
            true
        } else {
            false
        };
        let loc = match (image.find("##"), image.find("#*")) {
            (None, None) => None,
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (Some(a), Some(_)) => Some(a),
        };
        Ok((loc.map(|l| image[..l].to_string()).unwrap_or_default(), nl))
    }

    /// `escapedDirective(image)`.
    fn escaped_directive(&self, image: &str) -> String {
        let last = image.rfind('\\').expect("an escaped directive has a backslash");
        let directive = &image[last + 1..];
        let mut tag = &directive[1..];
        if tag.starts_with('{') {
            tag = &tag[1..tag.len() - 1];
        }
        let recognized = self.settings.strict_escape
            || self.directive_type(tag).is_some()
            || self.macro_names.contains(tag)
            || self.is_macro(tag)
            || matches!(tag, "if" | "end" | "set" | "else" | "elseif");
        if recognized { format!("{}{directive}", &image[..last / 2]) } else { image.to_string() }
    }

    /// `Escape()`: a run of `\\` pairs.
    fn escape(&mut self) -> R<String> {
        let mut count = 0;
        let mut last = 0;
        while self.kind(1)? == Kind::DoubleEscape {
            // `LOOKAHEAD(2)` on each pair.
            self.peek(2)?;
            last = self.consume_any();
            count += 1;
        }
        let next = self.toks.get(last + 1).cloned();
        let control = match next {
            Some(t) => {
                matches!(t.kind, Kind::IfDirective | Kind::Else | Kind::ElseIf | Kind::End) || {
                    let tag: String = t.image.chars().skip(1).collect();
                    self.settings.strict_escape || self.directive_type(&tag).is_some() || self.macro_names.contains(&tag) || self.is_macro(&tag)
                }
            }
            None => false,
        };
        Ok(if control { "\\".repeat(count) } else { "\\\\".repeat(count) })
    }

    // ---- directives ----------------------------------------------------------------------------------------

    fn if_statement(&mut self, after_newline: bool) -> R<(Node, bool)> {
        let newline_at_start = after_newline;
        let mut after_newline = after_newline;
        let mut prefix = String::new();
        if self.kind(1)? == Kind::Whitespace {
            let i = self.consume_any();
            prefix = self.image(i);
        }
        let if_token = self.consume(Kind::IfDirective)?;
        let more_prefix = hash_prefix(&self.image(if_token));
        self.skip_ws()?;
        self.consume(Kind::LParen)?;
        let condition = self.expression()?;
        self.consume(Kind::RParen)?;
        let mut then = Block::default();
        if let Some(p) = self.optional_line_end()? {
            prefix_of(&mut then, p);
            after_newline = true;
        }
        let stops = [Kind::ElseIf, Kind::Else, Kind::End];
        after_newline = self.statements(&mut then, after_newline, &stops)?;
        let mut else_ifs: Vec<(Expr, Block)> = Vec::new();
        let mut otherwise = None;
        loop {
            let k1 = self.kind(1)?;
            if !(k1 == Kind::ElseIf || after_newline && k1 == Kind::Whitespace && self.kind(2)? == Kind::ElseIf) {
                break;
            }
            let previous = else_ifs.last_mut().map(|(_, b)| b).unwrap_or(&mut then);
            if self.kind(1)? == Kind::Whitespace {
                let i = self.consume_any();
                previous.postfix = self.image(i);
            }
            let t = self.consume(Kind::ElseIf)?;
            let more = hash_prefix(&self.image(t));
            let previous = else_ifs.last_mut().map(|(_, b)| b).unwrap_or(&mut then);
            previous.more_postfix = more;
            self.skip_ws()?;
            self.consume(Kind::LParen)?;
            let cond = self.expression()?;
            self.consume(Kind::RParen)?;
            let mut block = Block::default();
            if let Some(p) = self.optional_line_end()? {
                prefix_of(&mut block, p);
                after_newline = true;
            }
            after_newline = self.statements(&mut block, after_newline, &stops)?;
            else_ifs.push((cond, block));
        }
        {
            let k1 = self.kind(1)?;
            if k1 == Kind::Else || after_newline && k1 == Kind::Whitespace && self.kind(2)? == Kind::Else {
                let previous = else_ifs.last_mut().map(|(_, b)| b).unwrap_or(&mut then);
                if self.kind(1)? == Kind::Whitespace {
                    let i = self.consume_any();
                    previous.postfix = self.image(i);
                }
                let t = self.consume(Kind::Else)?;
                let more = hash_prefix(&self.image(t));
                let previous = else_ifs.last_mut().map(|(_, b)| b).unwrap_or(&mut then);
                previous.more_postfix = more;
                let mut block = Block::default();
                if let Some(p) = self.optional_line_end()? {
                    prefix_of(&mut block, p);
                    after_newline = true;
                }
                after_newline = self.statements(&mut block, after_newline, &[Kind::End])?;
                otherwise = Some(block);
            }
        }
        let last_block: &mut Block = match (&mut otherwise, else_ifs.last_mut()) {
            (Some(b), _) => b,
            (None, Some((_, b))) => b,
            (None, None) => &mut then,
        };
        if after_newline && self.kind(1)? == Kind::Whitespace {
            let i = self.consume_any();
            last_block.postfix = self.image(i);
        }
        let end = self.consume(Kind::End)?;
        last_block.more_postfix = hash_prefix(&self.image(end));
        after_newline = false;
        let mut postfix = String::new();
        if newline_at_start || self.settings.bc {
            if let Some(p) = self.optional_line_end()? {
                postfix = p;
                after_newline = true;
            }
        }
        Ok((Node::If(Box::new(If { condition, then, else_ifs, otherwise, prefix, postfix, more_prefix })), after_newline))
    }

    /// Statements of a block until one of `stops` (optionally after whitespace at a line start).
    fn statements(&mut self, block: &mut Block, mut after_newline: bool, stops: &[Kind]) -> R<bool> {
        loop {
            let k1 = self.kind(1)?;
            if stops.contains(&k1) || k1 == Kind::Eof && stops.is_empty() {
                break;
            }
            if after_newline && k1 == Kind::Whitespace && stops.contains(&self.kind(2)?) {
                break;
            }
            if k1 == Kind::Eof {
                // `#end` missing: the production fails on EOF.
                return Err(self.fail());
            }
            let (node, nl) = self.statement(after_newline)?;
            after_newline = nl;
            if let Some(n) = node {
                block.nodes.push(n);
            }
        }
        Ok(after_newline)
    }

    fn set_directive(&mut self, after_newline: bool) -> R<(Node, bool)> {
        let mut prefix = String::new();
        if self.kind(1)? == Kind::Whitespace {
            let i = self.consume_any();
            prefix = self.image(i);
        }
        let t = self.consume(Kind::SetDirective)?;
        let more_prefix = hash_prefix(&self.image(t));
        let pos = self.pos(t);
        self.skip_ws()?;
        let left = self.reference()?;
        self.skip_ws()?;
        self.consume(Kind::Equals)?;
        let right = self.expression()?;
        let right_last_image = self.toks[..self.cur].iter().rev().find(|t| !is_ws(t.kind)).map(|t| t.image.clone()).unwrap_or_default();
        self.consume(Kind::RParen)?;
        self.lex.set_in_set(false);
        let mut postfix = String::new();
        let mut ends_with_newline = false;
        if after_newline || self.settings.bc {
            if let Some(p) = self.optional_line_end()? {
                postfix = p;
                ends_with_newline = true;
            }
        }
        Ok((Node::Set(Box::new(Set { left, right, right_last_image, pos, prefix, postfix, more_prefix })), ends_with_newline))
    }

    /// `MacroParseException(msg, template, id)`: the message, then the template and the position of the token after
    /// `id`.
    fn macro_parse_exception(&self, msg: &str, id: usize) -> VelocityError {
        let at = self.pos(if id + 1 < self.toks.len() { id + 1 } else { id });
        VelocityError::parse_at(format!("{msg} at {}\n", crate::error::LOCATION), at.line, at.column)
    }

    /// `isLeftParenthesis()`: optional whitespace then `(` in the raw stream.
    fn is_left_parenthesis(&self) -> bool {
        for c in self.lex.rest() {
            match c {
                '(' => return true,
                ' ' | '\n' | '\r' | '\t' => {}
                _ => return false,
            }
        }
        false
    }

    /// `isRightParenthesis()`.
    fn is_right_parenthesis(&mut self) -> R<bool> {
        match self.kind(1)? {
            Kind::RParen => return Ok(true),
            Kind::Whitespace | Kind::Newline => {}
            _ => return Ok(false),
        }
        for c in self.lex.rest() {
            match c {
                ')' => return Ok(true),
                ' ' | '\n' | '\r' | '\t' => {}
                _ => return Ok(false),
            }
        }
        Ok(false)
    }

    /// `isAssignment()`: in a macro definition, `$arg = default`.
    fn is_assignment(&self) -> bool {
        if self.lex.state != State::Reference {
            return false;
        }
        // `isAssignment()` reads whitespace (`Character.isWhitespace`, the end-of-template sentinel included) until the
        // first other character; the end of the input leaves the `try` by its `IOException` and answers `true`.
        self.lex.rest().iter().find(|c| !crate::lexer::java_is_whitespace(**c)).is_none_or(|c| *c == '=')
    }

    fn directive(&mut self, after_newline: bool) -> R<(Node, bool)> {
        let newline_at_start = after_newline;
        let mut after_newline;
        let mut prefix = String::new();
        let first = self.cur;
        if self.kind(1)? == Kind::Whitespace {
            let i = self.consume_any();
            prefix = self.image(i);
        }
        let k = self.kind(1)?;
        let id = self.consume(k)?;
        let id_image = self.image(id);
        let p = id_image.rfind('#').unwrap_or(0);
        let name = if k == Kind::BracketedWord { id_image[p + 2..id_image.len() - 1].to_string() } else { id_image[p + 1..].to_string() };
        let more_prefix = hash_prefix(&id_image);
        let known = self.directive_type(&name);
        let is_macro = name == "macro";
        let directive_kind = match known {
            Some(t) => t,
            None if name.starts_with('@') => DirectiveType::Block,
            None => DirectiveType::Line,
        };
        let is_vm = known.is_none() && !name.starts_with('@') && self.is_macro(&name);
        let (line, column) = (self.toks[id].line, self.toks[id].column);
        self.lex.switch_to(State::Directive);
        let mut args = Vec::new();
        let mut arg_types = Vec::new();
        let mut arg_pos = Vec::new();
        let mut has_parentheses = false;
        if self.is_left_parenthesis() {
            self.skip_ws()?;
            self.consume(Kind::LParen)?;
            loop {
                if self.is_right_parenthesis()? {
                    break;
                }
                self.skip_ws()?;
                if self.kind(1)? == Kind::Comma {
                    self.consume_any();
                    self.skip_ws()?;
                }
                if self.kind(1)? == Kind::SingleLineCommentStart {
                    if !is_macro {
                        return Err(self.macro_parse_exception(&format!("A Line comment is not allowed in {id_image} arguments"), id));
                    }
                    self.consume_any();
                    if self.kind(1)? == Kind::SingleLineComment {
                        self.consume_any();
                    }
                    continue;
                }
                if is_macro && self.is_assignment() {
                    self.peek(1)?;
                    arg_pos.push(self.pos(self.cur));
                    let r = self.reference()?;
                    self.skip_ws()?;
                    self.consume(Kind::Equals)?;
                    self.skip_ws()?;
                    args.push(Expr::Reference(Box::new(r)));
                    arg_types.push(ArgType::DirectiveAssign);
                }
                if self.is_right_parenthesis()? {
                    // `LOOKAHEAD({ !isRightParenthesis() })` before the argument failed: the choice has no match.
                    return Err(self.fail());
                }
                self.peek(1)?;
                let at = self.pos(self.cur);
                let (arg, t) = self.directive_arg()?;
                arg_pos.push(at);
                if known.is_none() && t == ArgType::Word && is_vm {
                    return Err(self.macro_parse_exception(&format!("Invalid argument {} in macro call {id_image}", arg_types.len() + 1), id));
                }
                args.push(arg);
                arg_types.push(t);
            }
            self.skip_ws()?;
            self.consume(Kind::RParen)?;
            has_parentheses = true;
        } else {
            self.lex.pop();
        }
        after_newline = false;
        let mut postfix = String::new();
        let mut block_prefix = String::new();
        let bc = self.settings.bc;
        if directive_kind != DirectiveType::Line || newline_at_start && !bc || bc && has_parentheses || matches!(name.as_str(), "include" | "parse") {
            if let Some(p) = self.optional_line_end()? {
                after_newline = true;
                if directive_kind == DirectiveType::Line {
                    postfix = p;
                } else {
                    block_prefix = p;
                }
            }
        }
        if known.is_some() {
            // `MacroParseException(msg, template, id)` reports `id.next`.
            let name_next = if id + 1 < self.toks.len() { self.pos(id + 1) } else { self.pos(id) };
            check_args(&name, &arg_types, name_next)?;
        }
        let mut directive = Directive {
            name,
            args,
            arg_types,
            arg_pos,
            macro_call_closed: has_parentheses,
            block: None,
            else_block: None,
            prefix,
            postfix,
            more_prefix,
            macro_literal: String::new(),
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            line,
            column,
        };
        if directive_kind == DirectiveType::Line {
            directive.macro_literal = self.macro_literal(first, &directive);
            return Ok((Node::Directive(Box::new(directive)), after_newline));
        }
        self.peek(1)?;
        let mut block = Block { prefix: block_prefix, start: self.pos(self.cur), ..Block::default() };
        after_newline = self.statements(&mut block, after_newline, &[Kind::End, Kind::Else])?;
        if after_newline && self.kind(1)? == Kind::Whitespace {
            let i = self.consume_any();
            block.postfix = self.image(i);
        }
        let mut else_block = None;
        if directive.name == "foreach" && self.kind(1)? == Kind::Else {
            let e = self.consume_any();
            let mut eb = Block::default();
            if let Some(p) = self.optional_line_end()? {
                eb.prefix = p;
                after_newline = true;
            }
            after_newline = self.statements(&mut eb, after_newline, &[Kind::End])?;
            block.more_postfix = hash_prefix(&self.image(e));
            else_block = Some(eb);
        }
        {
            let last_block = else_block.as_mut().unwrap_or(&mut block);
            if after_newline && self.kind(1)? == Kind::Whitespace {
                let i = self.consume_any();
                last_block.postfix = self.image(i);
            }
        }
        let end = self.consume(Kind::End)?;
        after_newline = false;
        if newline_at_start || self.settings.bc {
            if let Some(p) = self.optional_line_end()? {
                directive.postfix = p;
                after_newline = true;
            }
        }
        {
            let last_block = else_block.as_mut().unwrap_or(&mut block);
            let more = hash_prefix(&self.image(end));
            if !more.is_empty() {
                last_block.more_postfix = more;
            }
        }
        if is_macro {
            if let Some(Expr::Word(w)) = directive.args.first() {
                self.macro_names.insert(w.clone());
            }
        }
        directive.macro_call_closed = true;
        directive.block = Some(std::sync::Arc::new(block));
        directive.else_block = else_block.map(std::sync::Arc::new);
        directive.macro_literal = self.macro_literal(first, &directive);
        Ok((Node::Directive(Box::new(directive)), after_newline))
    }

    /// `RuntimeMacro.getLiteral()`: the directive's tokens from its `#`, preceded by the whitespace prefix when no
    /// `MORE` text came before the `#` and the node would hide it, without the postfix when the node writes it.
    fn macro_literal(&self, first: usize, d: &Directive) -> String {
        use crate::config::SpaceGobbling;
        let gobbling = self.settings.gobbling;
        let last = self.cur - 1;
        let mut out = String::new();
        let mut pos: Option<usize> = None;
        for i in first..last {
            let image = &self.toks[i].image;
            if pos.is_none() {
                pos = image.rfind('#');
            }
            match pos {
                Some(p) => {
                    out.push_str(&image[p..]);
                    pos = Some(0);
                }
                None if d.more_prefix.is_empty() && gobbling >= SpaceGobbling::Lines => out.push_str(image),
                None => {}
            }
        }
        if first <= last {
            let image = &self.toks[last].image;
            if let Some(p) = pos.or_else(|| image.rfind('#')) {
                out.push_str(&image[p..]);
            }
        }
        if (!d.more_prefix.is_empty() || gobbling == SpaceGobbling::None) && out.ends_with(&d.postfix) {
            out.truncate(out.len() - d.postfix.len());
        }
        out
    }

    fn directive_arg(&mut self) -> R<(Expr, ArgType)> {
        match self.kind(1)? {
            k if is_identifier(k) || k == Kind::LCurly => Ok((Expr::Reference(Box::new(self.reference()?)), ArgType::Reference)),
            Kind::Word => {
                let i = self.consume_any();
                Ok((Expr::Word(self.image(i)), ArgType::Word))
            }
            Kind::StringLiteral => Ok((self.string_literal()?, ArgType::StringLiteral)),
            Kind::IntegerLiteral => Ok((self.integer_literal()?, ArgType::IntegerLiteral)),
            Kind::LBracket if self.lookahead_range()? => Ok((self.integer_range()?, ArgType::IntegerRange)),
            Kind::FloatingPointLiteral => Ok((self.float_literal()?, ArgType::FloatingPointLiteral)),
            Kind::LeftCurley => Ok((self.map()?, ArgType::Map)),
            Kind::LBracket => Ok((self.object_array()?, ArgType::ObjectArray)),
            Kind::True => {
                self.consume_any();
                Ok((Expr::True, ArgType::True))
            }
            Kind::False => {
                self.consume_any();
                Ok((Expr::False, ArgType::False))
            }
            _ => Err(self.fail()),
        }
    }

    // ---- references ----------------------------------------------------------------------------------------

    fn reference(&mut self) -> R<Reference> {
        let first = self.cur;
        let mut parts = Vec::new();
        let mut alternate = None;
        let formal = self.kind(1)? == Kind::LCurly;
        if formal {
            self.consume_any();
        }
        let k = self.kind(1)?;
        if !is_identifier(k) {
            return Err(self.fail());
        }
        let root_token = self.consume_any();
        self.indexes(&mut parts)?;
        loop {
            // `LOOKAHEAD(2) <DOT> (Method() | Identifier())`.
            if self.kind(1)? != Kind::Dot || !is_identifier(self.kind(2)?) {
                break;
            }
            self.consume_any();
            let name_token = self.consume_any();
            let name = self.image(name_token);
            // `LOOKAHEAD(3) Method()`: `(` then something an argument list can start with.
            if self.kind(1)? == Kind::LParen && starts_method_arguments(self.kind(2)?) {
                self.peek(2)?;
                self.consume_any();
                let mut args = Vec::new();
                if self.kind(1)? != Kind::Refmod2RParen {
                    args.push(self.expression()?);
                    while self.kind(1)? == Kind::Comma {
                        self.consume_any();
                        args.push(self.expression()?);
                    }
                }
                self.consume(Kind::Refmod2RParen)?;
                parts.push(RefPart::Method(name, args, self.pos(name_token)));
            } else {
                parts.push(RefPart::Identifier(name, self.pos(name_token)));
            }
            self.indexes(&mut parts)?;
        }
        if formal {
            if self.kind(1)? == Kind::Pipe {
                self.consume_any();
                alternate = Some(Box::new(Expr::Group(Box::new(self.expression()?))));
            }
            match self.kind(1)? {
                Kind::RCurly | Kind::RightCurley => {
                    self.consume_any();
                }
                _ => return Err(self.fail()),
            }
        }
        let last = self.cur - 1;
        let _ = root_token;
        Ok(self.init_reference(first, last, parts, alternate))
    }

    fn indexes(&mut self, parts: &mut Vec<RefPart>) -> R<()> {
        while self.kind(1)? == Kind::IndexLBracket {
            let bracket = self.consume_any();
            self.skip_ws()?;
            let e = self.expression()?;
            self.skip_ws()?;
            self.consume(Kind::IndexRBracket)?;
            parts.push(RefPart::Index(e, self.pos(bracket)));
        }
        Ok(())
    }

    /// `ASTReference.init()` / `getRoot()`: the reference type, root name, escape and null-rendering strings. Mutates
    /// the first token's image as upstream does, so later literals see the stripped image.
    fn init_reference(&mut self, first: usize, last: usize, parts: Vec<RefPart>, alternate: Option<Box<Expr>>) -> Reference {
        let mut r = Reference {
            pos: self.pos(first),
            if_test: false,
            ref_type: RefType::Unset,
            root: String::new(),
            null_string: String::new(),
            escaped: false,
            esc_prefix: String::new(),
            more_prefix: String::new(),
            computable: true,
            parts,
            alternate,
            literal: String::new(),
            first_image: String::new(),
        };
        let literal = |p: &Parser| p.toks[first..=last].iter().map(token_literal).collect::<String>();
        let image = self.toks[first].image.clone();
        if let Some(_slashbang) = image.find("\\!") {
            match image.find('$') {
                None => {
                    r.computable = false;
                    r.null_string = image.clone();
                    r.root = image;
                }
                Some(mut i) => {
                    let chars: Vec<char> = image.chars().collect();
                    let byte = |ci: usize| chars[..ci].iter().collect::<String>().len();
                    let mut ci = image[..i].chars().count();
                    while ci < chars.len() && chars[ci] != '\\' {
                        ci += 1;
                    }
                    let start = ci;
                    let mut count = 0;
                    while ci < chars.len() && chars[ci] == '\\' {
                        ci += 1;
                        count += 1;
                    }
                    i = byte(start);
                    let mut s = image[..i].to_string();
                    s.push_str(&"\\".repeat(count - 1));
                    s.push_str(&image[byte(start + count)..]);
                    r.null_string = s.clone();
                    r.computable = false;
                    r.root = s;
                }
            }
            r.literal = literal(self);
            r.first_image = self.toks[first].image.clone();
            return r;
        }
        let mut image = image;
        if image.starts_with('\\') {
            let n = image.chars().take_while(|c| *c == '\\').count();
            r.escaped = n % 2 != 0;
            r.esc_prefix = "\\".repeat(n / 2);
            image = image[n..].to_string();
        }
        if let Some(loc) = image.rfind('$').filter(|l| *l > 0) {
            r.more_prefix = image[..loc].to_string();
            image = image[loc..].to_string();
        }
        self.toks[first].image = image.clone();
        r.null_string = literal(self);
        r.literal = r.null_string.clone();
        r.first_image = image.clone();
        if let Some(quiet) = image.strip_prefix("$!") {
            r.ref_type = RefType::Quiet;
            if !r.escaped {
                r.null_string = String::new();
            }
            r.root = if quiet.starts_with('{') { self.toks[first + 1].image.clone() } else { quiet.to_string() };
        } else if image == "${" {
            r.ref_type = RefType::Formal;
            r.root = self.toks[first + 1].image.clone();
        } else if let Some(rest) = image.strip_prefix('$') {
            r.ref_type = RefType::Normal;
            r.root = rest.to_string();
        } else {
            r.ref_type = RefType::Runt;
            r.root = image;
        }
        r
    }

    // ---- expressions ---------------------------------------------------------------------------------------

    pub fn expression(&mut self) -> R<Expr> {
        self.or_expression()
    }

    fn or_expression(&mut self) -> R<Expr> {
        let mut left = self.and_expression()?;
        while matches!(self.kind(1)?, Kind::LogicalOr | Kind::LogicalOr2) {
            self.consume_any();
            // The node starts at the token after the operator.
            self.peek(1)?;
            let at = self.pos(self.cur);
            let right = self.and_expression()?;
            left = Expr::Binary(BinOp::Or, Box::new(left), Box::new(right), at);
        }
        Ok(left)
    }

    fn and_expression(&mut self) -> R<Expr> {
        let mut left = self.equality_expression()?;
        while self.kind(1)? == Kind::LogicalAnd {
            self.consume_any();
            // The node starts at the token after the operator.
            self.peek(1)?;
            let at = self.pos(self.cur);
            let right = self.equality_expression()?;
            left = Expr::Binary(BinOp::And, Box::new(left), Box::new(right), at);
        }
        Ok(left)
    }

    fn equality_expression(&mut self) -> R<Expr> {
        let mut left = self.relational_expression()?;
        loop {
            let op = match self.kind(1)? {
                Kind::LogicalEquals => BinOp::Eq,
                Kind::LogicalNotEquals => BinOp::Ne,
                _ => return Ok(left),
            };
            self.consume_any();
            // The node starts at the token after the operator.
            self.peek(1)?;
            let at = self.pos(self.cur);
            let right = self.relational_expression()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right), at);
        }
    }

    fn relational_expression(&mut self) -> R<Expr> {
        let mut left = self.additive_expression()?;
        loop {
            let op = match self.kind(1)? {
                Kind::LogicalLt => BinOp::Lt,
                Kind::LogicalGt => BinOp::Gt,
                Kind::LogicalLe => BinOp::Le,
                Kind::LogicalGe => BinOp::Ge,
                _ => return Ok(left),
            };
            self.consume_any();
            // The node starts at the token after the operator.
            self.peek(1)?;
            let at = self.pos(self.cur);
            let right = self.additive_expression()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right), at);
        }
    }

    fn additive_expression(&mut self) -> R<Expr> {
        let mut left = self.multiplicative_expression()?;
        loop {
            let op = match self.kind(1)? {
                Kind::Plus => BinOp::Add,
                Kind::Minus => BinOp::Sub,
                _ => return Ok(left),
            };
            self.consume_any();
            // The node starts at the token after the operator.
            self.peek(1)?;
            let at = self.pos(self.cur);
            let right = self.multiplicative_expression()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right), at);
        }
    }

    fn multiplicative_expression(&mut self) -> R<Expr> {
        let mut left = self.unary_expression()?;
        loop {
            let op = match self.kind(1)? {
                Kind::Multiply => BinOp::Mul,
                Kind::Divide => BinOp::Div,
                Kind::Modulus => BinOp::Mod,
                _ => return Ok(left),
            };
            self.consume_any();
            // The node starts at the token after the operator.
            self.peek(1)?;
            let at = self.pos(self.cur);
            let right = self.unary_expression()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right), at);
        }
    }

    fn unary_expression(&mut self) -> R<Expr> {
        self.skip_ws()?;
        match self.kind(1)? {
            Kind::LogicalNot => {
                self.consume_any();
                Ok(Expr::Not(Box::new(self.unary_expression()?)))
            }
            Kind::Minus => {
                self.consume_any();
                Ok(Expr::Negate(Box::new(self.primary_expression()?)))
            }
            _ => self.primary_expression(),
        }
    }

    fn primary_expression(&mut self) -> R<Expr> {
        self.skip_ws()?;
        let e = match self.kind(1)? {
            Kind::StringLiteral => self.string_literal()?,
            k if is_identifier(k) || k == Kind::LCurly => Expr::Reference(Box::new(self.reference()?)),
            Kind::IntegerLiteral => self.integer_literal()?,
            Kind::LBracket if self.lookahead_range()? => self.integer_range()?,
            Kind::FloatingPointLiteral => self.float_literal()?,
            Kind::LeftCurley => self.map()?,
            Kind::LBracket => self.object_array()?,
            Kind::True => {
                self.consume_any();
                Expr::True
            }
            Kind::False => {
                self.consume_any();
                Expr::False
            }
            Kind::LParen => {
                self.consume_any();
                let e = self.expression()?;
                self.consume(Kind::RParen)?;
                Expr::Group(Box::new(e))
            }
            _ => return Err(self.fail()),
        };
        self.skip_ws()?;
        Ok(e)
    }

    /// `LOOKAHEAD(<LBRACKET> (ws)* (Reference() | IntegerLiteral()) (ws)* <DOUBLEDOT>)`: a speculative parse.
    fn lookahead_range(&mut self) -> R<bool> {
        let saved = self.cur;
        let ok = (|| -> R<bool> {
            self.consume(Kind::LBracket)?;
            self.skip_ws()?;
            match self.kind(1)? {
                Kind::IntegerLiteral => {
                    self.consume_any();
                }
                k if is_identifier(k) || k == Kind::LCurly => {
                    self.reference()?;
                }
                _ => return Ok(false),
            }
            self.skip_ws()?;
            Ok(self.kind(1)? == Kind::DoubleDot)
        })();
        self.cur = saved;
        match ok {
            Ok(b) => Ok(b),
            Err(e) if e.is_parse() => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn integer_range(&mut self) -> R<Expr> {
        self.consume(Kind::LBracket)?;
        self.skip_ws()?;
        let left = self.range_end()?;
        self.skip_ws()?;
        self.consume(Kind::DoubleDot)?;
        self.skip_ws()?;
        let right = self.range_end()?;
        self.skip_ws()?;
        self.consume(Kind::RBracket)?;
        Ok(Expr::Range(Box::new(left), Box::new(right)))
    }

    fn range_end(&mut self) -> R<Expr> {
        match self.kind(1)? {
            Kind::IntegerLiteral => self.integer_literal(),
            _ => Ok(Expr::Reference(Box::new(self.reference()?))),
        }
    }

    /// `Parameter()`.
    fn parameter(&mut self) -> R<Expr> {
        self.skip_ws()?;
        let e = match self.kind(1)? {
            Kind::StringLiteral => self.string_literal()?,
            Kind::IntegerLiteral => self.integer_literal()?,
            Kind::LBracket if self.lookahead_range()? => self.integer_range()?,
            Kind::LeftCurley => self.map()?,
            Kind::LBracket => self.object_array()?,
            Kind::True => {
                self.consume_any();
                Expr::True
            }
            Kind::False => {
                self.consume_any();
                Expr::False
            }
            k if is_identifier(k) || k == Kind::LCurly => Expr::Reference(Box::new(self.reference()?)),
            Kind::FloatingPointLiteral => self.float_literal()?,
            _ => return Err(self.fail()),
        };
        self.skip_ws()?;
        Ok(e)
    }

    fn map(&mut self) -> R<Expr> {
        self.consume(Kind::LeftCurley)?;
        let mut entries = Vec::new();
        // `LOOKAHEAD((ws)* Parameter() <COLON>)`.
        let saved = self.cur;
        let has_entries = match self.parameter() {
            Ok(_) => self.kind(1)? == Kind::Colon,
            Err(e) if e.is_parse() => false,
            Err(e) => return Err(e),
        };
        self.cur = saved;
        if has_entries {
            loop {
                let k = self.parameter()?;
                self.consume(Kind::Colon)?;
                let v = self.parameter()?;
                entries.push((k, v));
                if self.kind(1)? != Kind::Comma {
                    break;
                }
                self.consume_any();
            }
        } else {
            self.skip_ws()?;
        }
        match self.kind(1)? {
            Kind::RightCurley | Kind::RCurly => {
                self.consume_any();
            }
            _ => return Err(self.fail()),
        }
        Ok(Expr::Map(entries))
    }

    fn object_array(&mut self) -> R<Expr> {
        self.consume(Kind::LBracket)?;
        let mut items = Vec::new();
        if self.kind(1)? != Kind::RBracket {
            items.push(self.parameter()?);
            while self.kind(1)? == Kind::Comma {
                self.consume_any();
                items.push(self.parameter()?);
            }
        }
        self.consume(Kind::RBracket)?;
        Ok(Expr::List(items))
    }

    fn string_literal(&mut self) -> R<Expr> {
        let i = self.consume(Kind::StringLiteral)?;
        let img = self.image(i);
        let quote = img.chars().next().unwrap_or('"');
        let inner = &img[1..img.len() - 1];
        let mut image = if quote == '"' { unescape(inner).into_owned() } else { inner.to_string() };
        let doubled = format!("{quote}{quote}");
        if image.contains(quote) {
            image = image.replace(&doubled, &quote.to_string());
        }
        let interpolated = if quote == '"' && (img.contains('$') || img.contains('#')) {
            let (line, column) = (self.toks[i].line, self.toks[i].column);
            match Parser::nested(&image, self.settings, line, column).process() {
                Ok(block) => Some(block),
                // `ASTStringLiteral.init`, which runs after the whole template has parsed.
                Err(_) => {
                    if self.deferred.is_none() {
                        self.deferred = Some(VelocityError::parse_at(format!("Failed to parse String literal at {}", crate::error::LOCATION), line as i32, column as i32));
                    }
                    None
                }
            }
        } else {
            None
        };
        Ok(Expr::Str { image, literal: img, interpolated })
    }

    fn integer_literal(&mut self) -> R<Expr> {
        let i = self.consume(Kind::IntegerLiteral)?;
        let img = self.image(i);
        let v = if let Ok(n) = img.parse::<i32>() {
            JValue::Int(n)
        } else if let Ok(n) = img.parse::<i64>() {
            JValue::Long(n)
        } else {
            JValue::BigInteger(img.trim_start_matches('+').into())
        };
        Ok(Expr::Integer(v, img))
    }

    fn float_literal(&mut self) -> R<Expr> {
        let i = self.consume(Kind::FloatingPointLiteral)?;
        let img = self.image(i);
        Ok(Expr::Float(JValue::Double(img.parse().unwrap_or(0.0)), img))
    }
}

fn prefix_of(block: &mut Block, prefix: String) {
    block.prefix = prefix;
}

/// `ASTStringLiteral.unescape`: only `\uXXXX` escapes (UTF-16 units, so surrogate pairs join) are decoded; the
/// string itself when it has none.
pub fn unescape(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains("\\u") {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut units: Vec<u16> = Vec::with_capacity(s.len());
    let mut rest = s;
    while let Some(u) = rest.find("\\u") {
        units.extend(rest[..u].encode_utf16());
        match rest.get(u + 2..u + 6).and_then(|h| u16::from_str_radix(h, 16).ok()) {
            Some(c) => {
                units.push(c);
                rest = &rest[u + 6..];
            }
            None => {
                // The lexer only lets through complete escapes.
                units.extend(rest[u..u + 2].encode_utf16());
                rest = &rest[u + 2..];
            }
        }
    }
    units.extend(rest.encode_utf16());
    std::borrow::Cow::Owned(String::from_utf16_lossy(&units))
}

/// The first tokens of `[ Expression() ( <COMMA> Expression() )* ] <REFMOD2_RPAREN>`.
fn starts_method_arguments(k: Kind) -> bool {
    is_ws(k)
        || is_identifier(k)
        || matches!(
            k,
            Kind::LogicalNot
                | Kind::Minus
                | Kind::StringLiteral
                | Kind::LCurly
                | Kind::IntegerLiteral
                | Kind::LBracket
                | Kind::FloatingPointLiteral
                | Kind::LeftCurley
                | Kind::True
                | Kind::False
                | Kind::LParen
                | Kind::Refmod2RParen
        )
}

/// Each directive's `checkArgs`: a `MacroParseException` located at the token after the directive's name.
fn check_args(name: &str, types: &[ArgType], at: Pos) -> R<()> {
    let e = |m: &str| Err(VelocityError::parse_at(format!("{m} at {}\n", crate::error::LOCATION), at.line, at.column));
    match name {
        "foreach" => {
            if types.len() < 3 {
                return e("Too few arguments to the #foreach directive");
            }
            if types[0] != ArgType::Reference {
                return e("Expected argument 1 of #foreach to be a reference");
            }
            if types[1] != ArgType::Word {
                return e("Expected word 'in' at argument position 2 in #foreach");
            }
            if types[2] == ArgType::Word {
                return e("Argument 3 of #foreach is of the wrong type");
            }
        }
        "macro" => {
            if types.is_empty() {
                return e("A macro definition requires at least a macro name");
            }
            if types[0] != ArgType::Word {
                return e("Macro argument 1 must be a token without surrounding ' or \"");
            }
            let mut consecutive = false;
            let mut i = 1;
            while i < types.len() {
                if types[i] == ArgType::DirectiveAssign {
                    i += 1;
                    consecutive = true;
                } else if types[i] != ArgType::Reference {
                    return e(&format!("Macro argument {} must be a reference", i + 1));
                } else if consecutive {
                    return e("Macro non-default argument follows a default argument at ");
                }
                i += 1;
            }
        }
        "define" => {
            if types.len() != 1 {
                return e("The #define directive requires one argument");
            }
            if types[0] == ArgType::Word {
                return e("The argument to #define is of the wrong type");
            }
        }
        "parse" => {
            if types.len() != 1 {
                return e("The #parse directive requires one argument");
            }
            if types[0] == ArgType::Word {
                return e("The argument to #parse is of the wrong type");
            }
        }
        "break" if types.len() > 1 => return e("The #break directive takes only a single, optional Scope argument"),
        "stop" if types.len() > 1 => return e("The #stop directive only accepts a single message parameter"),
        _ => {}
    }
    Ok(())
}
