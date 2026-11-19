//! JavaCC's expected-token bookkeeping for Velocity's parser: the `ParseException` the generated `StandardParser`
//! throws, with its `Was expecting one of:` list.
//!
//! [`crate::parser`] is a hand-written port of the productions and does not keep the state the generated code does to
//! build that list (`jj_la1`, `jj_gen`, `jj_kind`, the saved look-aheads `jj_2_rtns`). When it fails on a token, this
//! module replays the template through a recognizer that follows the generated `StandardParser.java` production by
//! production (syntax and the semantic actions that steer parsing and lexing, no tree), up to the same failure, then
//! runs the generated `generateParseException`: the `jj_la1` sites evaluated at the failing position, the expected
//! kind of a failed `jj_consume_token`, and `jj_rescan_token`'s re-run of the look-aheads whose scan reached the
//! failing token. The scanners and the token tables are generated (`jj_scan.rs`, `generators/gen_jj_scan.py`).
//!
//! The site numbers in the productions (`la1[n]`) are those of the generated parser: `la1[n] = jj_gen` where the
//! generated code assigns `jj_la1[n] = jj_gen`, i.e. where a choice or an optional part found no matching token.

use crate::error::{LOCATION, add_escapes};
use crate::jj_scan::*;
use crate::lexer::{Kind, Lexer, State, Token};
use crate::parser::{DirectiveType, Settings};
use crate::VelocityError;

/// Why a scan or a production stopped.
pub(crate) enum Stop {
    /// `LookaheadSuccess`: the scan consumed its whole look-ahead.
    Success,
    /// The lexer failed (`TokenMgrError`), e.g. while a look-ahead scanned ahead.
    Lex(Box<VelocityError>),
    /// A `ParseException` raised by `generateParseException` (the state is kept).
    Parse,
    /// A custom exception of an action (`MacroParseException`): not a JavaCC expected-token list either.
    Other,
}

/// The result of a scanner: `true` when the scan failed.
pub(crate) type Res = Result<bool, Stop>;
type P<T> = Result<T, Stop>;

/// A saved look-ahead (`JJCalls`).
#[derive(Clone, Copy, Default)]
struct Call {
    jgen: i32,
    first: isize,
    arg: i32,
}

/// A parse failure the generated parser would have reported.
pub(crate) struct Failure {
    /// `TemplateParseException.getMessage()` with [`LOCATION`] where the template name and position go.
    pub message: String,
    pub line: usize,
    pub column: usize,
    /// How many tokens had been consumed (`jj_gen`): equals the replaying parser's when the replays agree.
    pub consumed: usize,
}

/// `StandardParserConstants` kind of a lexer token.
fn jkind(k: Kind) -> u8 {
    match k {
        Kind::Eof => EOF,
        Kind::LoneSymbol => LONE_SYMBOL,
        Kind::ZeroWidthWhitespace => ZERO_WIDTH_WHITESPACE,
        Kind::IndexLBracket => INDEX_LBRACKET,
        Kind::LogicalOr2 => LOGICAL_OR_2,
        Kind::Pipe => PIPE,
        Kind::IndexRBracket => INDEX_RBRACKET,
        Kind::LBracket => LBRACKET,
        Kind::RBracket => RBRACKET,
        Kind::Comma => COMMA,
        Kind::DoubleDot => DOUBLEDOT,
        Kind::Colon => COLON,
        Kind::LeftCurley => LEFT_CURLEY,
        Kind::RightCurley => RIGHT_CURLEY,
        Kind::LParen => LPAREN,
        Kind::RParen => RPAREN,
        Kind::Refmod2RParen => REFMOD2_RPAREN,
        Kind::EscapeDirective => ESCAPE_DIRECTIVE,
        Kind::SetDirective => SET_DIRECTIVE,
        Kind::SingleLineCommentStart => SINGLE_LINE_COMMENT_START,
        Kind::SingleLineComment => SINGLE_LINE_COMMENT,
        Kind::FormalComment => FORMAL_COMMENT,
        Kind::MultiLineComment => MULTI_LINE_COMMENT,
        Kind::Textblock => TEXTBLOCK,
        Kind::Whitespace => WHITESPACE,
        Kind::Newline => NEWLINE,
        Kind::Suffix => SUFFIX,
        Kind::StringLiteral => STRING_LITERAL,
        Kind::True => TRUE,
        Kind::False => FALSE,
        Kind::Minus => MINUS,
        Kind::Plus => PLUS,
        Kind::Multiply => MULTIPLY,
        Kind::Divide => DIVIDE,
        Kind::Modulus => MODULUS,
        Kind::LogicalAnd => LOGICAL_AND,
        Kind::LogicalOr => LOGICAL_OR,
        Kind::LogicalLt => LOGICAL_LT,
        Kind::LogicalLe => LOGICAL_LE,
        Kind::LogicalGt => LOGICAL_GT,
        Kind::LogicalGe => LOGICAL_GE,
        Kind::LogicalEquals => LOGICAL_EQUALS,
        Kind::LogicalNotEquals => LOGICAL_NOT_EQUALS,
        Kind::LogicalNot => LOGICAL_NOT,
        Kind::Equals => EQUALS,
        Kind::End => END,
        Kind::IfDirective => IF_DIRECTIVE,
        Kind::ElseIf => ELSEIF,
        Kind::Else => ELSE,
        Kind::IntegerLiteral => INTEGER_LITERAL,
        Kind::FloatingPointLiteral => FLOATING_POINT_LITERAL,
        Kind::Word => WORD,
        Kind::BracketedWord => BRACKETED_WORD,
        Kind::Identifier => IDENTIFIER,
        Kind::OldIdentifier => OLD_IDENTIFIER,
        Kind::Dot => DOT,
        Kind::LCurly => LCURLY,
        Kind::RCurly => RCURLY,
        Kind::DoubleEscape => DOUBLE_ESCAPE,
        Kind::Escape => ESCAPE,
        Kind::Text => TEXT,
        Kind::InlineText => INLINE_TEXT,
        Kind::EmptyIndex => EMPTY_INDEX,
    }
}

/// The generated parser's state.
pub(crate) struct Jj<'a> {
    lex: Lexer,
    /// Every token fetched so far.
    toks: Vec<Token>,
    offset: Option<(usize, usize)>,
    settings: &'a Settings<'a>,
    /// Index of `token`, the last consumed token (`-1` before the first).
    pub(crate) token: isize,
    pub(crate) scanpos: isize,
    lastpos: isize,
    la: i32,
    pub(crate) looking_ahead: bool,
    pub(crate) sem_la: bool,
    rescan: bool,
    jgen: i32,
    la1: [i32; 107],
    kind: i32,
    rtns: Vec<Vec<Call>>,
    expentries: Vec<Vec<u8>>,
    lasttokens: [i32; 100],
    endpos: usize,
}

const MAX: i32 = i32::MAX;

impl<'a> Jj<'a> {
    fn new(source: &str, settings: &'a Settings<'a>, offset: Option<(usize, usize)>) -> Jj<'a> {
        Jj {
            lex: Lexer::new(source, settings.hyphen_allowed),
            toks: Vec::new(),
            offset,
            settings,
            token: -1,
            scanpos: -1,
            lastpos: -1,
            la: 0,
            looking_ahead: false,
            sem_la: false,
            rescan: false,
            jgen: 0,
            la1: [-1; 107],
            kind: -1,
            rtns: vec![vec![Call::default()]; LOOKAHEADS],
            expentries: Vec::new(),
            lasttokens: [0; 100],
            endpos: 0,
        }
    }

    // ---- tokens --------------------------------------------------------------------------------------------

    /// `token_source.getNextToken()` appended after the last fetched token, up to index `i`.
    fn fetch(&mut self, i: isize) -> Result<(), Stop> {
        while (self.toks.len() as isize) <= i {
            let mut t = self.lex.next_token().map_err(|e| Stop::Lex(Box::new(e)))?;
            if let Some((line, column)) = self.offset {
                if t.line == 1 {
                    t.column += column;
                }
                t.line += line - 1;
            }
            self.toks.push(t);
        }
        Ok(())
    }

    /// `(jj_ntk == -1) ? jj_ntk_f() : jj_ntk`: the kind of `token.next`.
    fn ntk(&mut self) -> P<u8> {
        self.fetch(self.token + 1)?;
        Ok(jkind(self.toks[(self.token + 1) as usize].kind))
    }

    /// `getToken(index).kind`.
    fn get_token(&mut self, index: isize) -> P<u8> {
        let mut t = if self.looking_ahead { self.scanpos } else { self.token };
        for _ in 0..index {
            self.fetch(t + 1)?;
            t += 1;
        }
        Ok(jkind(self.toks[t as usize].kind))
    }

    /// `jj_consume_token(kind)`: the consumed token's index, or the exception.
    fn consume(&mut self, kind: u8) -> P<isize> {
        self.fetch(self.token + 1)?;
        if jkind(self.toks[(self.token + 1) as usize].kind) == kind {
            self.token += 1;
            self.jgen += 1;
            return Ok(self.token);
        }
        self.kind = kind as i32;
        Err(Stop::Parse)
    }

    /// `jj_consume_token(-1); throw new ParseException();`: nothing matches.
    fn nothing<T>(&mut self) -> P<T> {
        self.fetch(self.token + 1)?;
        self.kind = -1;
        Err(Stop::Parse)
    }

    // ---- scanning ------------------------------------------------------------------------------------------

    /// `jj_scan_token(kind)`: `true` when the next token is not `kind`.
    pub(crate) fn scan_token(&mut self, kind: u8) -> Res {
        if self.scanpos == self.lastpos {
            self.la = self.la.wrapping_sub(1);
            let next = self.scanpos + 1;
            self.fetch(next)?;
            self.lastpos = next;
            self.scanpos = next;
        } else {
            self.scanpos += 1;
        }
        if self.rescan && self.scanpos >= self.token {
            let i = (self.scanpos - self.token) as usize;
            self.add_error_token(kind, i);
        }
        if jkind(self.toks[self.scanpos as usize].kind) != kind {
            return Ok(true);
        }
        if self.la == 0 && self.scanpos == self.lastpos {
            return Err(Stop::Success);
        }
        Ok(false)
    }

    /// `jj_2_N(xla)`: the look-ahead succeeded.
    fn jj_2(&mut self, n: usize, xla: i32) -> P<bool> {
        self.la = xla;
        self.lastpos = self.token;
        self.scanpos = self.token;
        let r = match self.jj_3(n) {
            Ok(failed) => Ok(!failed),
            Err(Stop::Success) => Ok(true),
            Err(e) => Err(e),
        };
        self.jj_save(n - 1, xla);
        r
    }

    fn jj_save(&mut self, index: usize, xla: i32) {
        let chain = &mut self.rtns[index];
        let mut i = 0;
        while chain[i].jgen > self.jgen {
            if i + 1 == chain.len() {
                chain.push(Call::default());
                i += 1;
                break;
            }
            i += 1;
        }
        chain[i] = Call { jgen: self.jgen.wrapping_add(xla).wrapping_sub(self.la), first: self.token, arg: xla };
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
    pub(crate) fn is_right_parenthesis(&mut self) -> Result<bool, Stop> {
        match self.get_token(1)? {
            RPAREN => return Ok(true),
            WHITESPACE | NEWLINE => {}
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

    // ---- the exception -------------------------------------------------------------------------------------

    fn add_error_token(&mut self, kind: u8, pos: usize) {
        if pos >= 100 {
            return;
        }
        if pos == self.endpos + 1 {
            self.lasttokens[self.endpos] = kind as i32;
            self.endpos += 1;
        } else if self.endpos != 0 {
            let entry: Vec<u8> = self.lasttokens[..self.endpos].iter().map(|&k| k as u8).collect();
            // JavaCC 7.0.13 adds the entry only when an equal one is already listed (and then once more).
            if self.expentries.contains(&entry) {
                self.expentries.push(entry);
            }
            if pos != 0 {
                self.endpos = pos;
                self.lasttokens[pos - 1] = kind as i32;
            }
        }
    }

    fn rescan_token(&mut self) {
        self.rescan = true;
        for i in 0..LOOKAHEADS {
            let mut k = 0;
            while k < self.rtns[i].len() {
                let p = self.rtns[i][k];
                if p.jgen > self.jgen {
                    self.la = p.arg;
                    self.lastpos = p.first;
                    self.scanpos = p.first;
                    if let Err(Stop::Success) = self.jj_3(i + 1) {
                        // `catch (LookaheadSuccess ls)` surrounds the whole chain of this look-ahead.
                        break;
                    }
                }
                k += 1;
            }
        }
        self.rescan = false;
    }

    /// `generateParseException()` and `TemplateParseException.getMessage()`.
    fn exception(&mut self) -> Failure {
        self.expentries.clear();
        let mut la1tokens = [false; 96];
        if self.kind >= 0 {
            la1tokens[self.kind as usize] = true;
            self.kind = -1;
        }
        for i in 0..107 {
            if self.la1[i] == self.jgen {
                for j in 0..32 {
                    if LA1_0[i] & (1 << j) != 0 {
                        la1tokens[j] = true;
                    }
                    if LA1_1[i] & (1 << j) != 0 {
                        la1tokens[32 + j] = true;
                    }
                    if LA1_2[i] & (1 << j) != 0 {
                        la1tokens[64 + j] = true;
                    }
                }
            }
        }
        for (i, on) in la1tokens.iter().enumerate().take(81) {
            if *on {
                self.expentries.push(vec![i as u8]);
            }
        }
        self.endpos = 0;
        self.rescan_token();
        self.add_error_token(0, 0);

        let mut max_size = 0;
        let mut expected = String::new();
        for seq in &self.expentries {
            max_size = max_size.max(seq.len());
            for &i in seq {
                expected.push_str(TOKEN_IMAGE[i as usize]);
                expected.push(' ');
            }
            if seq[seq.len() - 1] != 0 {
                expected.push_str("...");
            }
            expected.push_str("\n    ");
        }
        let first = (self.token + 1) as usize;
        let mut message = String::from("Encountered \"");
        for i in 0..max_size {
            if i != 0 {
                message.push(' ');
            }
            let Some(t) = self.toks.get(first + i) else { break };
            if t.kind == Kind::Eof {
                message.push_str(TOKEN_IMAGE[0]);
                break;
            }
            message.push_str(&add_escapes(&t.image));
        }
        message.push_str("\" at ");
        message.push_str(LOCATION);
        message.push('\n');
        message.push_str(if self.expentries.len() == 1 { "Was expecting:\n    " } else { "Was expecting one of:\n    " });
        message.push_str(&expected);
        let t = &self.toks[first];
        Failure { message, line: t.line, column: t.column, consumed: self.jgen as usize }
    }

    // ---- the productions -----------------------------------------------------------------------------------

    /// `( <WHITESPACE> | <NEWLINE> )*` with the loop's two choice sites.
    fn ws_star(&mut self, loop_site: usize, choice_site: usize) -> P<()> {
        loop {
            match self.ntk()? {
                WHITESPACE | NEWLINE => {}
                _ => {
                    self.la1[loop_site] = self.jgen;
                    return Ok(());
                }
            }
            match self.ntk()? {
                WHITESPACE => {
                    self.consume(WHITESPACE)?;
                }
                NEWLINE => {
                    self.consume(NEWLINE)?;
                }
                _ => {
                    self.la1[choice_site] = self.jgen;
                    return self.nothing();
                }
            }
        }
    }

    /// `[ <WHITESPACE> ]` with its choice site.
    fn opt_ws(&mut self, site: usize) -> P<()> {
        match self.ntk()? {
            WHITESPACE => {
                self.consume(WHITESPACE)?;
            }
            _ => self.la1[site] = self.jgen,
        }
        Ok(())
    }

    /// `process()`.
    fn process(&mut self) -> P<()> {
        let mut after_newline = true;
        while self.get_token(1)? != EOF {
            after_newline = self.statement(after_newline)?;
        }
        self.consume(EOF)?;
        Ok(())
    }

    /// `Statement(afterNewline)`.
    fn statement(&mut self, after_newline: bool) -> P<bool> {
        let mut after_newline = after_newline;
        if self.get_token(1)? == IF_DIRECTIVE || after_newline && self.get_token(1)? == WHITESPACE && self.get_token(2)? == IF_DIRECTIVE {
            return self.if_statement(after_newline);
        }
        if self.jj_2(1, 2)? {
            self.reference()?;
            return Ok(false);
        }
        if self.jj_2(2, 2)? {
            return self.comment();
        }
        match self.ntk()? {
            TEXTBLOCK => {
                self.consume(TEXTBLOCK)?;
                return Ok(false);
            }
            _ => self.la1[1] = self.jgen,
        }
        if self.get_token(1)? == SET_DIRECTIVE || after_newline && self.get_token(1)? == WHITESPACE && self.get_token(2)? == SET_DIRECTIVE {
            return self.set_directive(after_newline);
        }
        match self.ntk()? {
            ESCAPE_DIRECTIVE => {
                self.consume(ESCAPE_DIRECTIVE)?;
                return Ok(false);
            }
            DOUBLE_ESCAPE => {
                self.escape()?;
                return Ok(false);
            }
            _ => self.la1[2] = self.jgen,
        }
        if self.get_token(1)? == WORD || self.get_token(1)? == BRACKETED_WORD || after_newline && self.get_token(1)? == WHITESPACE && (self.get_token(2)? == WORD || self.get_token(2)? == BRACKETED_WORD) {
            return self.directive(after_newline);
        }
        match self.ntk()? {
            LONE_SYMBOL | PIPE | LPAREN | RPAREN | STRING_LITERAL | INTEGER_LITERAL | FLOATING_POINT_LITERAL | DOT | LCURLY | RCURLY | ESCAPE | TEXT | EMPTY_INDEX => self.text(),
            NEWLINE => {
                self.consume(NEWLINE)?;
                Ok(true)
            }
            INLINE_TEXT => {
                self.consume(INLINE_TEXT)?;
                after_newline = false;
                match self.ntk()? {
                    TEXT => {
                        self.consume(TEXT)?;
                        after_newline = true;
                    }
                    _ => self.la1[0] = self.jgen,
                }
                Ok(after_newline)
            }
            WHITESPACE => {
                self.consume(WHITESPACE)?;
                Ok(false)
            }
            SUFFIX => {
                self.consume(SUFFIX)?;
                Ok(true)
            }
            _ => {
                self.la1[3] = self.jgen;
                if self.jj_2(3, 2)? {
                    // EndingZeroWidthWhitespace
                    self.consume(ZERO_WIDTH_WHITESPACE)?;
                    self.consume(EOF)?;
                    return Ok(after_newline);
                }
                match self.ntk()? {
                    LOGICAL_OR_2 => {
                        self.consume(LOGICAL_OR_2)?;
                        Ok(after_newline)
                    }
                    ZERO_WIDTH_WHITESPACE => {
                        self.consume(ZERO_WIDTH_WHITESPACE)?;
                        Ok(false)
                    }
                    _ => {
                        self.la1[4] = self.jgen;
                        self.nothing()
                    }
                }
            }
        }
    }

    /// `Escape()`.
    fn escape(&mut self) -> P<()> {
        loop {
            self.consume(DOUBLE_ESCAPE)?;
            if !self.jj_2(4, 2)? {
                return Ok(());
            }
        }
    }

    /// `Comment()`: whether it ended a line.
    fn comment(&mut self) -> P<bool> {
        match self.ntk()? {
            SINGLE_LINE_COMMENT_START => {
                self.consume(SINGLE_LINE_COMMENT_START)?;
                match self.ntk()? {
                    SINGLE_LINE_COMMENT => {
                        self.consume(SINGLE_LINE_COMMENT)?;
                    }
                    _ => self.la1[5] = self.jgen,
                }
                Ok(true)
            }
            MULTI_LINE_COMMENT => {
                self.consume(MULTI_LINE_COMMENT)?;
                Ok(false)
            }
            FORMAL_COMMENT => {
                self.consume(FORMAL_COMMENT)?;
                Ok(false)
            }
            _ => {
                self.la1[6] = self.jgen;
                self.nothing()
            }
        }
    }

    /// `Identifier()`.
    fn identifier(&mut self) -> P<()> {
        match self.ntk()? {
            IDENTIFIER => {
                self.consume(IDENTIFIER)?;
            }
            OLD_IDENTIFIER => {
                self.consume(OLD_IDENTIFIER)?;
            }
            _ => {
                self.la1[7] = self.jgen;
                return self.nothing();
            }
        }
        Ok(())
    }

    /// `DirectiveArg()`.
    fn directive_arg(&mut self) -> P<()> {
        match self.ntk()? {
            IDENTIFIER | OLD_IDENTIFIER | LCURLY => self.reference(),
            WORD => {
                self.consume(WORD)?;
                Ok(())
            }
            STRING_LITERAL => {
                self.consume(STRING_LITERAL)?;
                Ok(())
            }
            INTEGER_LITERAL => {
                self.consume(INTEGER_LITERAL)?;
                Ok(())
            }
            _ => {
                self.la1[8] = self.jgen;
                if self.jj_2(5, MAX)? {
                    return self.integer_range();
                }
                match self.ntk()? {
                    FLOATING_POINT_LITERAL => {
                        self.consume(FLOATING_POINT_LITERAL)?;
                        Ok(())
                    }
                    LEFT_CURLEY => self.map(),
                    LBRACKET => self.object_array(),
                    TRUE => {
                        self.consume(TRUE)?;
                        Ok(())
                    }
                    FALSE => {
                        self.consume(FALSE)?;
                        Ok(())
                    }
                    _ => {
                        self.la1[9] = self.jgen;
                        self.nothing()
                    }
                }
            }
        }
    }

    /// `Directive(afterNewline)`.
    fn directive(&mut self, after_newline: bool) -> P<bool> {
        let newline_at_start = after_newline;
        let mut after_newline;
        self.opt_ws(10)?;
        let id = match self.ntk()? {
            WORD => self.consume(WORD)?,
            BRACKETED_WORD => self.consume(BRACKETED_WORD)?,
            _ => {
                self.la1[11] = self.jgen;
                return self.nothing();
            }
        };
        let image = self.toks[id as usize].image.clone();
        let p = image.rfind('#').unwrap_or(0);
        let name = if self.toks[id as usize].kind == Kind::BracketedWord { image[p + 2..image.len() - 1].to_string() } else { image[p + 1..].to_string() };
        let d = (self.settings.directive)(&name);
        let is_macro = name == "macro";
        let directive_type = match d {
            Some(t) => t,
            None if name.starts_with('@') => DirectiveType::Block,
            None => DirectiveType::Line,
        };
        let is_vm = d.is_none() && !name.starts_with('@') && (self.settings.is_macro)(&name);
        self.lex.switch_to(State::Directive);
        let mut has_parentheses = false;
        if self.is_left_parenthesis() {
            self.ws_star(12, 13)?;
            self.consume(LPAREN)?;
            loop {
                if self.is_right_parenthesis()? {
                    break;
                }
                self.ws_star(14, 15)?;
                match self.ntk()? {
                    COMMA => {
                        self.consume(COMMA)?;
                        self.ws_star(16, 17)?;
                    }
                    _ => self.la1[18] = self.jgen,
                }
                if self.jj_2(6, 1)? {
                    if is_macro && self.is_assignment() {
                        self.reference()?;
                        self.ws_star(19, 20)?;
                        self.consume(EQUALS)?;
                        self.ws_star(21, 22)?;
                    }
                    if self.is_right_parenthesis()? {
                        return self.nothing();
                    }
                    let first = self.token + 1;
                    self.directive_arg()?;
                    if d.is_none() && self.toks[first as usize].kind == Kind::Word && is_vm {
                        return Err(Stop::Other);
                    }
                } else {
                    match self.ntk()? {
                        SINGLE_LINE_COMMENT_START => {
                            if !is_macro {
                                return Err(Stop::Other);
                            }
                            self.consume(SINGLE_LINE_COMMENT_START)?;
                            match self.ntk()? {
                                SINGLE_LINE_COMMENT => {
                                    self.consume(SINGLE_LINE_COMMENT)?;
                                }
                                _ => self.la1[23] = self.jgen,
                            }
                        }
                        _ => {
                            self.la1[24] = self.jgen;
                            return self.nothing();
                        }
                    }
                }
            }
            self.ws_star(25, 26)?;
            self.consume(RPAREN)?;
            has_parentheses = true;
        } else {
            self.lex.pop();
        }
        after_newline = false;
        let bc = self.settings.bc;
        if self.jj_2(7, 2)? && (directive_type != DirectiveType::Line || newline_at_start && !bc || bc && has_parentheses || matches!(name.as_str(), "include" | "parse")) {
            self.opt_ws(27)?;
            self.consume(NEWLINE)?;
            after_newline = true;
        }
        if directive_type == DirectiveType::Line {
            return Ok(after_newline);
        }
        loop {
            if self.get_token(1)? != END && self.get_token(1)? != ELSE && (!after_newline || self.get_token(1)? != WHITESPACE || self.get_token(2)? != END && self.get_token(2)? != ELSE) {
                after_newline = self.statement(after_newline)?;
            } else {
                break;
            }
        }
        if self.jj_2(8, 1)? && after_newline {
            self.consume(WHITESPACE)?;
        }
        if d.is_some() && name == "foreach" && self.get_token(1)? == ELSE {
            self.consume(ELSE)?;
            if self.jj_2(9, 2)? {
                self.opt_ws(28)?;
                self.consume(NEWLINE)?;
                after_newline = true;
            }
            loop {
                if self.get_token(1)? != END && (!after_newline || self.get_token(1)? != WHITESPACE || self.get_token(2)? != END) {
                    after_newline = self.statement(after_newline)?;
                } else {
                    break;
                }
            }
        }
        if self.jj_2(10, 1)? && after_newline {
            self.consume(WHITESPACE)?;
        }
        self.consume(END)?;
        after_newline = false;
        if self.jj_2(11, 2)? && (newline_at_start || bc) {
            self.opt_ws(29)?;
            self.consume(NEWLINE)?;
            after_newline = true;
        }
        Ok(after_newline)
    }

    /// `Map()`.
    fn map(&mut self) -> P<()> {
        self.consume(LEFT_CURLEY)?;
        if self.jj_2(12, MAX)? {
            self.parameter()?;
            self.consume(COLON)?;
            self.parameter()?;
            loop {
                match self.ntk()? {
                    COMMA => {}
                    _ => {
                        self.la1[30] = self.jgen;
                        break;
                    }
                }
                self.consume(COMMA)?;
                self.parameter()?;
                self.consume(COLON)?;
                self.parameter()?;
            }
        } else {
            self.ws_star(31, 32)?;
        }
        match self.ntk()? {
            RIGHT_CURLEY => {
                self.consume(RIGHT_CURLEY)?;
            }
            RCURLY => {
                self.consume(RCURLY)?;
            }
            _ => {
                self.la1[33] = self.jgen;
                return self.nothing();
            }
        }
        Ok(())
    }

    /// `ObjectArray()`.
    fn object_array(&mut self) -> P<()> {
        self.consume(LBRACKET)?;
        match self.ntk()? {
            LBRACKET | LEFT_CURLEY | WHITESPACE | NEWLINE | STRING_LITERAL | TRUE | FALSE | INTEGER_LITERAL | FLOATING_POINT_LITERAL | IDENTIFIER | OLD_IDENTIFIER | LCURLY => {
                self.parameter()?;
                loop {
                    match self.ntk()? {
                        COMMA => {}
                        _ => {
                            self.la1[34] = self.jgen;
                            break;
                        }
                    }
                    self.consume(COMMA)?;
                    self.parameter()?;
                }
            }
            _ => self.la1[35] = self.jgen,
        }
        self.consume(RBRACKET)?;
        Ok(())
    }

    /// A range bound: `Reference() | IntegerLiteral()`.
    fn range_bound(&mut self, site: usize) -> P<()> {
        match self.ntk()? {
            IDENTIFIER | OLD_IDENTIFIER | LCURLY => self.reference(),
            INTEGER_LITERAL => {
                self.consume(INTEGER_LITERAL)?;
                Ok(())
            }
            _ => {
                self.la1[site] = self.jgen;
                self.nothing()
            }
        }
    }

    /// `IntegerRange()`.
    fn integer_range(&mut self) -> P<()> {
        self.consume(LBRACKET)?;
        self.ws_star(36, 37)?;
        self.range_bound(38)?;
        self.ws_star(39, 40)?;
        self.consume(DOUBLEDOT)?;
        self.ws_star(41, 42)?;
        self.range_bound(43)?;
        self.ws_star(44, 45)?;
        self.consume(RBRACKET)?;
        Ok(())
    }

    /// `IndexParameter()`.
    fn index_parameter(&mut self) -> P<()> {
        self.ws_star(46, 47)?;
        self.expression()?;
        self.ws_star(48, 49)
    }

    /// `Parameter()`.
    fn parameter(&mut self) -> P<()> {
        self.ws_star(50, 51)?;
        match self.ntk()? {
            STRING_LITERAL => {
                self.consume(STRING_LITERAL)?;
            }
            INTEGER_LITERAL => {
                self.consume(INTEGER_LITERAL)?;
            }
            _ => {
                self.la1[52] = self.jgen;
                if self.jj_2(13, MAX)? {
                    self.integer_range()?;
                } else {
                    match self.ntk()? {
                        LEFT_CURLEY => self.map()?,
                        LBRACKET => self.object_array()?,
                        TRUE => {
                            self.consume(TRUE)?;
                        }
                        FALSE => {
                            self.consume(FALSE)?;
                        }
                        IDENTIFIER | OLD_IDENTIFIER | LCURLY => self.reference()?,
                        FLOATING_POINT_LITERAL => {
                            self.consume(FLOATING_POINT_LITERAL)?;
                        }
                        _ => {
                            self.la1[53] = self.jgen;
                            return self.nothing();
                        }
                    }
                }
            }
        }
        self.ws_star(54, 55)
    }

    /// `Method()`.
    fn method(&mut self) -> P<()> {
        self.identifier()?;
        self.consume(LPAREN)?;
        match self.ntk()? {
            LBRACKET | LEFT_CURLEY | LPAREN | WHITESPACE | NEWLINE | STRING_LITERAL | TRUE | FALSE | MINUS | LOGICAL_NOT | INTEGER_LITERAL | FLOATING_POINT_LITERAL | IDENTIFIER | OLD_IDENTIFIER | LCURLY => {
                self.expression()?;
                loop {
                    match self.ntk()? {
                        COMMA => {}
                        _ => {
                            self.la1[56] = self.jgen;
                            break;
                        }
                    }
                    self.consume(COMMA)?;
                    self.expression()?;
                }
            }
            _ => self.la1[57] = self.jgen,
        }
        self.consume(REFMOD2_RPAREN)?;
        Ok(())
    }

    /// `Index()`.
    fn index(&mut self) -> P<()> {
        self.consume(INDEX_LBRACKET)?;
        self.index_parameter()?;
        self.consume(INDEX_RBRACKET)?;
        Ok(())
    }

    /// `( Index() )*`.
    fn indexes(&mut self, site: usize) -> P<()> {
        loop {
            match self.ntk()? {
                INDEX_LBRACKET => {}
                _ => {
                    self.la1[site] = self.jgen;
                    return Ok(());
                }
            }
            self.index()?;
        }
    }

    /// `( LOOKAHEAD(2) <DOT> ( LOOKAHEAD(3) Method() | Identifier() ) ( Index() )* )*`.
    fn dotted(&mut self, dot_look: usize, method_look: usize, identifier_site: usize, index_site: usize) -> P<()> {
        loop {
            if !self.jj_2(dot_look, 2)? {
                return Ok(());
            }
            self.consume(DOT)?;
            if self.jj_2(method_look, 3)? {
                self.method()?;
            } else {
                match self.ntk()? {
                    IDENTIFIER | OLD_IDENTIFIER => self.identifier()?,
                    _ => {
                        self.la1[identifier_site] = self.jgen;
                        return self.nothing();
                    }
                }
            }
            self.indexes(index_site)?;
        }
    }

    /// `Reference()`.
    fn reference(&mut self) -> P<()> {
        match self.ntk()? {
            IDENTIFIER | OLD_IDENTIFIER => {
                match self.ntk()? {
                    IDENTIFIER => {
                        self.consume(IDENTIFIER)?;
                    }
                    OLD_IDENTIFIER => {
                        self.consume(OLD_IDENTIFIER)?;
                    }
                    _ => {
                        self.la1[58] = self.jgen;
                        return self.nothing();
                    }
                }
                self.indexes(59)?;
                self.dotted(14, 15, 60, 61)
            }
            LCURLY => {
                self.consume(LCURLY)?;
                match self.ntk()? {
                    IDENTIFIER => {
                        self.consume(IDENTIFIER)?;
                    }
                    OLD_IDENTIFIER => {
                        self.consume(OLD_IDENTIFIER)?;
                    }
                    _ => {
                        self.la1[62] = self.jgen;
                        return self.nothing();
                    }
                }
                self.indexes(63)?;
                self.dotted(16, 17, 64, 65)?;
                match self.ntk()? {
                    PIPE => {
                        self.consume(PIPE)?;
                        self.expression()?;
                    }
                    _ => self.la1[66] = self.jgen,
                }
                match self.ntk()? {
                    RCURLY => {
                        self.consume(RCURLY)?;
                    }
                    RIGHT_CURLEY => {
                        self.consume(RIGHT_CURLEY)?;
                    }
                    _ => {
                        self.la1[67] = self.jgen;
                        return self.nothing();
                    }
                }
                Ok(())
            }
            _ => {
                self.la1[68] = self.jgen;
                self.nothing()
            }
        }
    }

    /// `Text()`: whether the last token read was a newline-ending text.
    fn text(&mut self) -> P<bool> {
        match self.ntk()? {
            TEXT => {
                self.consume(TEXT)?;
                Ok(true)
            }
            k @ (DOT | RPAREN | LPAREN | INTEGER_LITERAL | FLOATING_POINT_LITERAL | STRING_LITERAL | ESCAPE | LCURLY | RCURLY | EMPTY_INDEX | PIPE | LONE_SYMBOL) => {
                self.consume(k)?;
                Ok(false)
            }
            _ => {
                self.la1[69] = self.jgen;
                self.nothing()
            }
        }
    }

    /// `[ LOOKAHEAD(2, ...) [ <WHITESPACE> ] <NEWLINE> ]` after a directive's head: whether the newline was read.
    fn head_newline(&mut self, look: usize, site: usize) -> P<bool> {
        if self.jj_2(look, 2)? {
            self.opt_ws(site)?;
            self.consume(NEWLINE)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// `IfStatement(afterNewline)`.
    fn if_statement(&mut self, after_newline: bool) -> P<bool> {
        let newline_at_start = after_newline;
        let mut after_newline = after_newline;
        self.opt_ws(70)?;
        self.consume(IF_DIRECTIVE)?;
        self.ws_star(71, 72)?;
        self.consume(LPAREN)?;
        self.expression()?;
        self.consume(RPAREN)?;
        if self.head_newline(18, 73)? {
            after_newline = true;
        }
        after_newline = self.statements_until_branch(after_newline)?;
        let mut last_ends_with_newline = after_newline;
        if self.get_token(1)? == ELSEIF || (after_newline && self.get_token(1)? == WHITESPACE && self.get_token(2)? == ELSEIF) {
            loop {
                last_ends_with_newline = self.else_if_statement(after_newline)?;
                after_newline = last_ends_with_newline;
                if !(self.get_token(1)? == ELSEIF || (after_newline && self.get_token(1)? == WHITESPACE && self.get_token(2)? == ELSEIF)) {
                    break;
                }
            }
        }
        if self.get_token(1)? == ELSE || (after_newline && self.get_token(1)? == WHITESPACE && self.get_token(2)? == ELSE) {
            last_ends_with_newline = self.else_statement(after_newline)?;
            after_newline = last_ends_with_newline;
        }
        let _ = last_ends_with_newline;
        if self.jj_2(19, 1)? && after_newline {
            self.consume(WHITESPACE)?;
        }
        self.consume(END)?;
        after_newline = false;
        if self.jj_2(20, 2)? && (newline_at_start || self.settings.bc) {
            self.opt_ws(74)?;
            self.consume(NEWLINE)?;
            after_newline = true;
        }
        Ok(after_newline)
    }

    /// `( LOOKAHEAD({ not #elseif, #else, #end ... }) Statement(afterNewline) )*` of an `#if` or `#elseif` block.
    fn statements_until_branch(&mut self, after_newline: bool) -> P<bool> {
        let mut after_newline = after_newline;
        loop {
            let k1 = self.get_token(1)?;
            if (k1 != ELSEIF && k1 != ELSE && k1 != END) && (!after_newline || self.get_token(1)? != WHITESPACE || (self.get_token(2)? != ELSEIF && self.get_token(2)? != ELSE && self.get_token(2)? != END)) {
                after_newline = self.statement(after_newline)?;
            } else {
                return Ok(after_newline);
            }
        }
    }

    /// `ElseStatement(previousBlock, afterNewline)`: whether its block ended with a newline.
    fn else_statement(&mut self, after_newline: bool) -> P<bool> {
        let mut after_newline = after_newline;
        self.opt_ws(75)?;
        self.consume(ELSE)?;
        if self.head_newline(21, 76)? {
            after_newline = true;
        }
        loop {
            if self.get_token(1)? != END && (!after_newline || self.get_token(1)? != WHITESPACE || self.get_token(2)? != END) {
                after_newline = self.statement(after_newline)?;
            } else {
                return Ok(after_newline);
            }
        }
    }

    /// `ElseIfStatement(previousBlock, afterNewline)`.
    fn else_if_statement(&mut self, after_newline: bool) -> P<bool> {
        let mut after_newline = after_newline;
        self.opt_ws(77)?;
        self.consume(ELSEIF)?;
        self.ws_star(78, 79)?;
        self.consume(LPAREN)?;
        self.expression()?;
        self.consume(RPAREN)?;
        if self.head_newline(22, 80)? {
            after_newline = true;
        }
        self.statements_until_branch(after_newline)
    }

    /// `SetDirective(afterNewline)`: whether it ended with a newline.
    fn set_directive(&mut self, after_newline: bool) -> P<bool> {
        let mut ends_with_newline = false;
        self.opt_ws(81)?;
        self.consume(SET_DIRECTIVE)?;
        self.ws_star(82, 83)?;
        self.reference()?;
        self.ws_star(84, 85)?;
        self.consume(EQUALS)?;
        self.expression()?;
        self.consume(RPAREN)?;
        self.lex.set_in_set(false);
        if self.jj_2(23, 2)? && (after_newline || self.settings.bc) {
            self.opt_ws(86)?;
            self.consume(NEWLINE)?;
            ends_with_newline = true;
        }
        Ok(ends_with_newline)
    }

    // ---- expressions ---------------------------------------------------------------------------------------

    fn expression(&mut self) -> P<()> {
        self.conditional_or()
    }

    fn conditional_or(&mut self) -> P<()> {
        self.conditional_and()?;
        loop {
            match self.ntk()? {
                LOGICAL_OR_2 | LOGICAL_OR => {}
                _ => {
                    self.la1[87] = self.jgen;
                    return Ok(());
                }
            }
            match self.ntk()? {
                LOGICAL_OR => {
                    self.consume(LOGICAL_OR)?;
                }
                LOGICAL_OR_2 => {
                    self.consume(LOGICAL_OR_2)?;
                }
                _ => {
                    self.la1[88] = self.jgen;
                    return self.nothing();
                }
            }
            self.conditional_and()?;
        }
    }

    fn conditional_and(&mut self) -> P<()> {
        self.equality()?;
        loop {
            match self.ntk()? {
                LOGICAL_AND => {}
                _ => {
                    self.la1[89] = self.jgen;
                    return Ok(());
                }
            }
            self.consume(LOGICAL_AND)?;
            self.equality()?;
        }
    }

    fn equality(&mut self) -> P<()> {
        self.relational()?;
        loop {
            match self.ntk()? {
                LOGICAL_EQUALS | LOGICAL_NOT_EQUALS => {}
                _ => {
                    self.la1[90] = self.jgen;
                    return Ok(());
                }
            }
            match self.ntk()? {
                LOGICAL_EQUALS => {
                    self.consume(LOGICAL_EQUALS)?;
                    self.relational()?;
                }
                LOGICAL_NOT_EQUALS => {
                    self.consume(LOGICAL_NOT_EQUALS)?;
                    self.relational()?;
                }
                _ => {
                    self.la1[91] = self.jgen;
                    return self.nothing();
                }
            }
        }
    }

    fn relational(&mut self) -> P<()> {
        self.additive()?;
        loop {
            match self.ntk()? {
                LOGICAL_LT | LOGICAL_LE | LOGICAL_GT | LOGICAL_GE => {}
                _ => {
                    self.la1[92] = self.jgen;
                    return Ok(());
                }
            }
            match self.ntk()? {
                k @ (LOGICAL_LT | LOGICAL_GT | LOGICAL_LE | LOGICAL_GE) => {
                    self.consume(k)?;
                    self.additive()?;
                }
                _ => {
                    self.la1[93] = self.jgen;
                    return self.nothing();
                }
            }
        }
    }

    fn additive(&mut self) -> P<()> {
        self.multiplicative()?;
        loop {
            match self.ntk()? {
                MINUS | PLUS => {}
                _ => {
                    self.la1[94] = self.jgen;
                    return Ok(());
                }
            }
            match self.ntk()? {
                k @ (PLUS | MINUS) => {
                    self.consume(k)?;
                    self.multiplicative()?;
                }
                _ => {
                    self.la1[95] = self.jgen;
                    return self.nothing();
                }
            }
        }
    }

    fn multiplicative(&mut self) -> P<()> {
        self.unary()?;
        loop {
            match self.ntk()? {
                MULTIPLY | DIVIDE | MODULUS => {}
                _ => {
                    self.la1[96] = self.jgen;
                    return Ok(());
                }
            }
            match self.ntk()? {
                k @ (MULTIPLY | DIVIDE | MODULUS) => {
                    self.consume(k)?;
                    self.unary()?;
                }
                _ => {
                    self.la1[97] = self.jgen;
                    return self.nothing();
                }
            }
        }
    }

    fn unary(&mut self) -> P<()> {
        self.ws_star(98, 99)?;
        match self.ntk()? {
            LOGICAL_NOT => {
                self.consume(LOGICAL_NOT)?;
                self.unary()
            }
            MINUS => {
                self.consume(MINUS)?;
                self.primary()
            }
            LBRACKET | LEFT_CURLEY | LPAREN | WHITESPACE | NEWLINE | STRING_LITERAL | TRUE | FALSE | INTEGER_LITERAL | FLOATING_POINT_LITERAL | IDENTIFIER | OLD_IDENTIFIER | LCURLY => self.primary(),
            _ => {
                self.la1[100] = self.jgen;
                self.nothing()
            }
        }
    }

    fn primary(&mut self) -> P<()> {
        self.ws_star(101, 102)?;
        match self.ntk()? {
            STRING_LITERAL => {
                self.consume(STRING_LITERAL)?;
            }
            IDENTIFIER | OLD_IDENTIFIER | LCURLY => self.reference()?,
            INTEGER_LITERAL => {
                self.consume(INTEGER_LITERAL)?;
            }
            _ => {
                self.la1[103] = self.jgen;
                if self.jj_2(24, MAX)? {
                    self.integer_range()?;
                } else {
                    match self.ntk()? {
                        FLOATING_POINT_LITERAL => {
                            self.consume(FLOATING_POINT_LITERAL)?;
                        }
                        LEFT_CURLEY => self.map()?,
                        LBRACKET => self.object_array()?,
                        TRUE => {
                            self.consume(TRUE)?;
                        }
                        FALSE => {
                            self.consume(FALSE)?;
                        }
                        LPAREN => {
                            self.consume(LPAREN)?;
                            self.expression()?;
                            self.consume(RPAREN)?;
                        }
                        _ => {
                            self.la1[104] = self.jgen;
                            return self.nothing();
                        }
                    }
                }
            }
        }
        self.ws_star(105, 106)
    }
}

/// How the generated parser fails on a template.
pub(crate) enum Outcome {
    /// A `ParseException` with its expected tokens.
    Exception(Failure),
    /// A `TokenMgrError`: a look-ahead scan reached a token the lexer cannot read before the parser failed.
    Lexical(VelocityError),
}

/// Replays `source` through the generated parser's productions and returns how it fails. `None` when the replay
/// succeeds or fails with a custom exception (`MacroParseException`), which the parser reports itself.
pub(crate) fn parse_exception(source: &str, settings: &Settings<'_>, offset: Option<(usize, usize)>) -> Option<Outcome> {
    let mut jj = Jj::new(source, settings, offset);
    match jj.process() {
        Err(Stop::Parse) => Some(Outcome::Exception(jj.exception())),
        Err(Stop::Lex(e)) => Some(Outcome::Lexical(*e)),
        _ => None,
    }
}
