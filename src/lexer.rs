//! The token manager JavaCC generates from `Parser.jjt`: lexical states, longest-match rules (ties go to the rule
//! declared first), `MORE` / `SKIP` / `SPECIAL_TOKEN` semantics and the rule actions that push, pop and switch
//! states. Token images always come from the input (the generated `jjstrLiteralImages` table is empty), so a
//! `MORE` prefix such as `$` stays part of the next token (`${`, `$foo`).

use crate::VelocityError;

/// Lexical states, as named in the grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Default,
    PreDirective,
    Directive,
    PreReference,
    PreOldReference,
    Reference,
    RefModifier,
    OldRefModifier,
    RefMod2,
    RefMod3,
    RefIndex,
    AltVal,
    InSingleLineComment,
    InFormalComment,
    InMultiLineComment,
    InTextblock,
}

use State::*;

/// Token kinds, in declaration order (JavaCC's kind numbers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Eof,
    LoneSymbol,
    ZeroWidthWhitespace,
    IndexLBracket,
    LogicalOr2,
    Pipe,
    IndexRBracket,
    LBracket,
    RBracket,
    Comma,
    DoubleDot,
    Colon,
    LeftCurley,
    RightCurley,
    LParen,
    RParen,
    Refmod2RParen,
    EscapeDirective,
    SetDirective,
    SingleLineCommentStart,
    SingleLineComment,
    FormalComment,
    MultiLineComment,
    Textblock,
    Whitespace,
    Newline,
    Suffix,
    StringLiteral,
    True,
    False,
    Minus,
    Plus,
    Multiply,
    Divide,
    Modulus,
    LogicalAnd,
    LogicalOr,
    LogicalLt,
    LogicalLe,
    LogicalGt,
    LogicalGe,
    LogicalEquals,
    LogicalNotEquals,
    LogicalNot,
    Equals,
    End,
    IfDirective,
    ElseIf,
    Else,
    IntegerLiteral,
    FloatingPointLiteral,
    Word,
    BracketedWord,
    Identifier,
    OldIdentifier,
    Dot,
    LCurly,
    RCurly,
    DoubleEscape,
    Escape,
    Text,
    InlineText,
    EmptyIndex,
}

/// `Character.isWhitespace`: Unicode space separators except the no-break ones, and the ASCII separators `\t`,
/// `\n`, `\u{b}`, `\f`, `\r` and `\u{1c}`-`\u{1f}` (Rust's `char::is_whitespace` leaves out the last four).
pub(crate) fn java_is_whitespace(c: char) -> bool {
    matches!(c, '\u{9}'..='\u{d}' | '\u{1c}'..='\u{20}' | '\u{1680}' | '\u{2000}'..='\u{2006}' | '\u{2008}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{205f}' | '\u{3000}')
}

impl State {
    /// The lexical state's number in the generated `StandardParserConstants`.
    fn number(self) -> u32 {
        match self {
            PreDirective => 0,
            PreReference => 1,
            PreOldReference => 2,
            Reference => 3,
            RefModifier => 4,
            OldRefModifier => 5,
            RefMod3 => 6,
            RefIndex => 7,
            Directive => 8,
            RefMod2 => 9,
            Default => 10,
            InFormalComment => 13,
            InSingleLineComment => 14,
            AltVal => 15,
            InTextblock => 16,
            InMultiLineComment => 17,
        }
    }
}

/// The `ParseException` `StandardParser.parse` makes of a `TokenMgrError`: `"Lexical error: " + tme.toString()`,
/// with JavaCC 7.0.13's `TokenMgrError.LexicalErr` text. `found` is the UTF-16 unit the automaton could not take, or
/// `None` at the end of the input. `ParseErrorException` rewrites it ([`VelocityError::lexical_in`]).
fn lexical_error(state: State, line: usize, column: usize, found: Option<u16>, after: &str) -> VelocityError {
    let found = match found {
        None => "<EOF>".to_string(),
        Some(unit) => format!("'{}' ({unit}),", crate::error::add_escapes_utf16(&[unit])),
    };
    let after = if after.is_empty() { String::new() } else { format!(" after prefix \"{}\"", crate::error::add_escapes(after)) };
    let state = match state.number() {
        0 => String::new(),
        n => format!(" (in lexical state {n})"),
    };
    VelocityError::parse(format!("{}Lexical error at line {line}, column {column}.  Encountered: {found}{after}{state}", crate::error::LEXICAL_ERROR))
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: Kind,
    pub image: String,
    /// Images of the special tokens (reference / directive terminators) just before this one, oldest first.
    pub specials: Vec<String>,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Token(Kind),
    More,
    Skip,
    Special,
}

/// What a rule's action does.
#[derive(Clone, Copy, PartialEq)]
enum Action {
    None,
    LoneSymbol,
    IndexLBracket,
    PopState,
    Pipe,
    LeftCurley,
    RightCurley,
    LParen,
    RParen,
    Refmod2RParen,
    SetDirective,
    Dollar,
    TextblockStart,
    FormalCommentStart,
    MultiLineCommentStart,
    Hash,
    SingleLineCommentStart,
    CommentEnd,
    TextblockEnd,
    Newline,
    StringLiteral,
    SwitchToDirective,
    IntegerLiteral,
    FloatingPointLiteral,
    Identifier,
    Dot,
    LCurly,
    RCurly,
    Terminator,
}

type Matcher = fn(&[char], usize) -> Option<usize>;

struct Rule {
    class: Class,
    states: &'static [State],
    matcher: Matcher,
    action: Action,
    /// How many characters the generated automaton reads into this rule where the rule does not match: the longest
    /// beginning of the input that a longer input would still match. JavaCC reports a lexical error after the
    /// furthest of these (`after prefix`), at the character none of the rules could take.
    prefix: Prefix,
}

type Prefix = fn(&[char], usize) -> usize;

impl Rule {
    fn prefix(mut self, prefix: Prefix) -> Rule {
        self.prefix = prefix;
        self
    }
}

/// The longest common beginning of the input and one of `literals`.
fn literal_prefix(c: &[char], p: usize, literals: &[&str]) -> usize {
    literals.iter().map(|l| l.chars().zip(&c[p..]).take_while(|(a, b)| a == *b).count()).max().unwrap_or(0)
}

const ALL: &[State] = &[
    Default, PreDirective, Directive, PreReference, PreOldReference, Reference, RefModifier, OldRefModifier, RefMod2, RefMod3, RefIndex, AltVal,
    InSingleLineComment, InFormalComment, InMultiLineComment, InTextblock,
];
const PRE: &[State] = &[PreDirective, PreReference, PreOldReference];
const ZWW: &[State] = &[Reference, RefModifier, OldRefModifier, RefMod3, RefIndex, Directive, RefMod2, Default, InFormalComment, InSingleLineComment];
const REF_BRACKET: &[State] = &[Reference, RefModifier, OldRefModifier, RefMod3];
const DIR_EXPR: &[State] = &[Directive, RefMod2, AltVal];
const LPAREN: &[State] = &[Directive, RefModifier, OldRefModifier];
const SET: &[State] = &[Default, PreReference, PreOldReference, Reference, RefModifier, OldRefModifier, RefMod2, RefMod3];
const LINE_COMMENT: &[State] = &[Default, PreDirective, Directive, Reference, PreReference, PreOldReference, RefMod2, RefMod3, RefModifier, OldRefModifier];
const COMMENTS: &[State] = &[InSingleLineComment, InFormalComment, InMultiLineComment];
const WHITESPACE: &[State] = &[Default, RefIndex, RefMod2, Directive, AltVal];
const EXPR: &[State] = &[Directive, RefMod2, RefIndex, AltVal];
const BOOL: &[State] = &[Reference, Directive, RefModifier, OldRefModifier, RefMod2, RefIndex, AltVal];
const NUMBER: &[State] = &[PreDirective, Directive, RefMod2, RefIndex, AltVal];
const WORD: &[State] = &[PreDirective, Directive];
const DOT: &[State] = &[Reference, RefModifier, OldRefModifier, RefMod2, RefMod3];
const CURLY: &[State] = &[PreReference, PreOldReference, Reference, RefModifier, OldRefModifier, RefMod3];
const TERMINATOR: &[State] = &[PreReference, PreOldReference, Reference, RefModifier, OldRefModifier, RefMod3];
const EMPTY_INDEX: &[State] = &[Default, PreReference, PreOldReference];

fn lit(s: &'static str) -> impl Fn(&[char], usize) -> Option<usize> {
    move |c: &[char], p: usize| {
        let n = s.chars().count();
        (c.len() >= p + n && s.chars().zip(&c[p..]).all(|(a, b)| a == *b)).then_some(n)
    }
}

macro_rules! lits {
    ($($s:literal),+) => {
        |c: &[char], p: usize| [$(lit($s)(c, p)),+].into_iter().flatten().max()
    };
}

const EOF_CHAR: char = '\u{1c}';

fn is_text_char(c: char) -> bool {
    !matches!(c, '$' | '#' | '\\' | '\r' | '\n' | EOF_CHAR)
}

fn newline(c: &[char], p: usize) -> Option<usize> {
    match (c.get(p), c.get(p + 1)) {
        (Some('\r'), Some('\n')) => Some(2),
        (Some('\r' | '\n'), _) => Some(1),
        _ => None,
    }
}

fn whitespace(c: &[char], p: usize) -> Option<usize> {
    let n = c[p..].iter().take_while(|c| matches!(c, ' ' | '\t')).count();
    (n > 0).then_some(n)
}

fn suffix(c: &[char], p: usize) -> Option<usize> {
    let n = c[p..].iter().take_while(|c| matches!(c, ' ' | '\t')).count();
    newline(c, p + n).map(|l| n + l)
}

/// `TEXT`: a line with at least one non-blank character, its newline, and any further whole lines.
fn text(c: &[char], p: usize) -> Option<usize> {
    let line = c[p..].iter().take_while(|c| is_text_char(**c)).count();
    if !c[p..p + line].iter().any(|c| !matches!(c, ' ' | '\t')) {
        return None;
    }
    let mut end = p + line + newline(c, p + line)?;
    loop {
        let more = c[end..].iter().take_while(|c| is_text_char(**c)).count();
        match newline(c, end + more) {
            Some(l) => end += more + l,
            None => return Some(end - p),
        }
    }
}

fn inline_text(c: &[char], p: usize) -> Option<usize> {
    let n = c[p..].iter().take_while(|c| is_text_char(**c)).count();
    (n > 0).then_some(n)
}

fn is_letter(c: char) -> bool {
    c.is_ascii_alphabetic()
}

fn word_at(c: &[char], p: usize) -> Option<usize> {
    let first = *c.get(p)?;
    if !(is_letter(first) || first == '_' || first == '@') {
        return None;
    }
    Some(1 + c[p + 1..].iter().take_while(|c| c.is_ascii_alphanumeric() || **c == '_').count())
}

fn word(c: &[char], p: usize) -> Option<usize> {
    word_at(c, p)
}

fn bracketed_word(c: &[char], p: usize) -> Option<usize> {
    if c.get(p) != Some(&'{') {
        return None;
    }
    let n = word_at(c, p + 1)?;
    (c.get(p + 1 + n) == Some(&'}')).then_some(n + 2)
}

fn identifier(c: &[char], p: usize) -> Option<usize> {
    let first = *c.get(p)?;
    if !(is_letter(first) || first == '_') {
        return None;
    }
    Some(1 + c[p + 1..].iter().take_while(|c| c.is_ascii_alphanumeric() || **c == '_').count())
}

fn old_identifier(c: &[char], p: usize) -> Option<usize> {
    let first = *c.get(p)?;
    if !(is_letter(first) || first == '_') {
        return None;
    }
    Some(1 + c[p + 1..].iter().take_while(|c| c.is_ascii_alphanumeric() || **c == '_' || **c == '-').count())
}

fn dot(c: &[char], p: usize) -> Option<usize> {
    (c.get(p) == Some(&'.') && c.get(p + 1).is_some_and(|c| is_letter(*c) || *c == '_')).then_some(2)
}

fn digits(c: &[char], p: usize) -> usize {
    c[p..].iter().take_while(|c| c.is_ascii_digit()).count()
}

fn integer_literal(c: &[char], p: usize) -> Option<usize> {
    let sign = usize::from(c.get(p) == Some(&'-'));
    let d = digits(c, p + sign);
    if d == 0 {
        return None;
    }
    let end = p + sign + d;
    Some(if c.get(end) == Some(&'.') && c.get(end + 1) == Some(&'.') { end + 2 - p } else { end - p })
}

fn exponent(c: &[char], p: usize) -> Option<usize> {
    if !matches!(c.get(p), Some('e' | 'E')) {
        return None;
    }
    let sign = usize::from(matches!(c.get(p + 1), Some('+' | '-')));
    let d = digits(c, p + 1 + sign);
    (d > 0).then_some(1 + sign + d)
}

fn floating_point_literal(c: &[char], p: usize) -> Option<usize> {
    let sign = usize::from(c.get(p) == Some(&'-'));
    let start = p + sign;
    let int = digits(c, start);
    let mut best = None;
    if int > 0 && c.get(start + int) == Some(&'.') {
        let frac = digits(c, start + int + 1);
        let end = start + int + 1 + frac;
        best = Some(end + exponent(c, end).unwrap_or(0) - p);
    }
    if c.get(start) == Some(&'.') {
        let frac = digits(c, start + 1);
        if frac > 0 {
            let end = start + 1 + frac;
            best = best.max(Some(end + exponent(c, end).unwrap_or(0) - p));
        }
    }
    if int > 0 {
        if let Some(e) = exponent(c, start + int) {
            best = best.max(Some(start + int + e - p));
        }
    }
    best
}

/// `STRING_LITERAL`: `"…"` or `'…'`, where the quote doubles to escape itself; the longest valid match.
fn string_literal(c: &[char], p: usize) -> Option<usize> {
    let q = *c.get(p)?;
    if q != '"' && q != '\'' {
        return None;
    }
    let mut i = p + 1;
    let mut best = None;
    loop {
        match c.get(i) {
            None | Some(&EOF_CHAR) => return best,
            Some(ch) if *ch == q => {
                let run = c[i..].iter().take_while(|x| **x == q).count();
                if run % 2 == 1 {
                    return Some(i + run - p);
                }
                // An even run is doubled quotes inside the literal; ending after all but one of them is also valid.
                best = Some(i + run - 1 - p);
                i += run;
            }
            Some(_) => i += 1,
        }
    }
}

fn escape_directive(c: &[char], p: usize) -> Option<usize> {
    let mut i = p;
    while c.get(i) == Some(&'\\') && c.get(i + 1) == Some(&'\\') {
        i += 2;
    }
    // At least the final `\#`: an odd backslash before `#`.
    if c.get(i) != Some(&'\\') || c.get(i + 1) != Some(&'#') {
        return None;
    }
    let w = word(c, i + 2).or_else(|| bracketed_word(c, i + 2))?;
    Some(i + 2 + w - p)
}

fn set_directive(c: &[char], p: usize) -> Option<usize> {
    let n = lit("#set")(c, p).or_else(|| lit("#{set}")(c, p))?;
    let ws = c[p + n..].iter().take_while(|c| matches!(c, ' ' | '\t')).count();
    (c.get(p + n + ws) == Some(&'(')).then_some(n + ws + 1)
}

fn dollar(c: &[char], p: usize) -> Option<usize> {
    let b = c[p..].iter().take_while(|c| **c == '\\').count();
    (c.get(p + b) == Some(&'$')).then_some(b + 1)
}

fn dollar_bang(c: &[char], p: usize) -> Option<usize> {
    let n = dollar(c, p)?;
    let b = c[p + n..].iter().take_while(|c| **c == '\\').count();
    (c.get(p + n + b) == Some(&'!')).then_some(n + b + 1)
}

fn formal_comment_start(c: &[char], p: usize) -> Option<usize> {
    lit("#**")(c, p)?;
    c.get(p + 3).filter(|c| **c != '#' && **c != EOF_CHAR).map(|_| 4)
}

fn any(c: &[char], p: usize) -> Option<usize> {
    c.get(p).map(|_| 1)
}

fn any_but_eof(c: &[char], p: usize) -> Option<usize> {
    c.get(p).filter(|c| **c != EOF_CHAR).map(|_| 1)
}

fn empty_index(c: &[char], p: usize) -> Option<usize> {
    lits!("$[", "$![", "$\\![", "$.")(c, p)
}

/// `dollar` / `dollar_bang` read the backslashes in front of a `$` that is not there.
fn backslashes(c: &[char], p: usize) -> usize {
    c[p..].iter().take_while(|c| **c == '\\').count()
}

/// An unterminated string literal is read up to the end-of-input sentinel.
fn string_literal_prefix(c: &[char], p: usize) -> usize {
    match c.get(p) {
        Some('"' | '\'') => 1 + c[p + 1..].iter().take_while(|c| **c != EOF_CHAR).count(),
        _ => 0,
    }
}

/// `-`, `.` and `-.` begin a number that is not there.
fn number_prefix(c: &[char], p: usize) -> usize {
    let sign = usize::from(c.get(p) == Some(&'-'));
    sign + usize::from(c.get(p + sign) == Some(&'.'))
}

fn keyword(s: &'static str, alt: &'static str) -> impl Fn(&[char], usize) -> Option<usize> {
    move |c: &[char], p: usize| lit(s)(c, p).max(lit(alt)(c, p))
}

fn rules() -> Vec<Rule> {
    use Kind as K;
    let t = |kind: K, states: &'static [State], matcher: Matcher, action: Action| Rule { class: Class::Token(kind), states, matcher, action, prefix: |_, _| 0 };
    let more = |states: &'static [State], matcher: Matcher, action: Action| Rule { class: Class::More, states, matcher, action, prefix: |_, _| 0 };
    vec![
        t(K::LoneSymbol, PRE, |c, p| lit("\u{1c}")(c, p), Action::LoneSymbol),
        t(K::ZeroWidthWhitespace, ZWW, |c, p| lit("\u{1c}")(c, p), Action::None),
        t(K::IndexLBracket, REF_BRACKET, |c, p| lit("[")(c, p), Action::IndexLBracket),
        t(K::LogicalOr2, REF_BRACKET, |c, p| lit("||")(c, p), Action::PopState).prefix(|c, p| literal_prefix(c, p, &["||"])),
        t(K::Pipe, REF_BRACKET, |c, p| lit("|")(c, p), Action::Pipe),
        t(K::IndexRBracket, &[RefIndex], |c, p| lit("]")(c, p), Action::PopState),
        t(K::LBracket, DIR_EXPR, |c, p| lit("[")(c, p), Action::None),
        t(K::RBracket, DIR_EXPR, |c, p| lit("]")(c, p), Action::None),
        t(K::Comma, DIR_EXPR, |c, p| lit(",")(c, p), Action::None),
        t(K::DoubleDot, DIR_EXPR, |c, p| lit("..")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &[".."])),
        t(K::Colon, DIR_EXPR, |c, p| lit(":")(c, p), Action::None),
        t(K::LeftCurley, DIR_EXPR, |c, p| lit("{")(c, p), Action::LeftCurley),
        t(K::RightCurley, DIR_EXPR, |c, p| lit("}")(c, p), Action::RightCurley),
        t(K::LParen, LPAREN, |c, p| lit("(")(c, p), Action::LParen),
        t(K::RParen, &[Directive], |c, p| lit(")")(c, p), Action::RParen),
        t(K::Refmod2RParen, &[RefMod2], |c, p| lit(")")(c, p), Action::Refmod2RParen),
        t(K::EscapeDirective, &[Default], escape_directive, Action::None),
        t(K::SetDirective, SET, set_directive, Action::SetDirective),
        more(ALL, dollar, Action::Dollar).prefix(backslashes),
        more(ALL, dollar_bang, Action::Dollar).prefix(backslashes),
        more(ALL, |c, p| lit("#[[")(c, p), Action::TextblockStart),
        more(ALL, formal_comment_start, Action::FormalCommentStart),
        more(ALL, |c, p| lit("#*")(c, p), Action::MultiLineCommentStart),
        more(ALL, |c, p| lit("#")(c, p), Action::Hash),
        t(K::SingleLineCommentStart, LINE_COMMENT, |c, p| lit("##")(c, p), Action::SingleLineCommentStart),
        t(K::SingleLineComment, &[InSingleLineComment], newline, Action::CommentEnd),
        t(K::FormalComment, &[InFormalComment], |c, p| lit("*#")(c, p), Action::CommentEnd),
        t(K::MultiLineComment, &[InMultiLineComment], |c, p| lit("*#")(c, p), Action::CommentEnd),
        t(K::Textblock, &[InTextblock], |c, p| lit("]]#")(c, p), Action::TextblockEnd),
        Rule { class: Class::Skip, states: COMMENTS, matcher: any, action: Action::None, prefix: |_, _| 0 },
        more(&[InTextblock], any_but_eof, Action::None),
        t(K::Whitespace, WHITESPACE, whitespace, Action::None),
        t(K::Newline, WHITESPACE, newline, Action::Newline),
        t(K::Suffix, PRE, suffix, Action::PopState).prefix(|c, p| c[p..].iter().take_while(|c| matches!(c, ' ' | '\t')).count()),
        t(K::StringLiteral, EXPR, string_literal, Action::StringLiteral).prefix(string_literal_prefix),
        t(K::True, BOOL, |c, p| lit("true")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &["true"])),
        t(K::False, BOOL, |c, p| lit("false")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &["false"])),
        t(K::Minus, EXPR, |c, p| lit("-")(c, p), Action::None),
        t(K::Plus, EXPR, |c, p| lit("+")(c, p), Action::None),
        t(K::Multiply, EXPR, |c, p| lit("*")(c, p), Action::None),
        t(K::Divide, EXPR, |c, p| lit("/")(c, p), Action::None),
        t(K::Modulus, EXPR, |c, p| lit("%")(c, p), Action::None),
        t(K::LogicalAnd, EXPR, |c, p| keyword("&&", "and")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &["&&", "and"])),
        t(K::LogicalOr, EXPR, |c, p| keyword("||", "or")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &["||", "or"])),
        t(K::LogicalLt, EXPR, |c, p| keyword("<", "lt")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &["<", "lt"])),
        t(K::LogicalLe, EXPR, |c, p| keyword("<=", "le")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &["<=", "le"])),
        t(K::LogicalGt, EXPR, |c, p| keyword(">", "gt")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &[">", "gt"])),
        t(K::LogicalGe, EXPR, |c, p| keyword(">=", "ge")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &[">=", "ge"])),
        t(K::LogicalEquals, EXPR, |c, p| keyword("==", "eq")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &["==", "eq"])),
        t(K::LogicalNotEquals, EXPR, |c, p| keyword("!=", "ne")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &["!=", "ne"])),
        t(K::LogicalNot, EXPR, |c, p| keyword("!", "not")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &["!", "not"])),
        t(K::Equals, EXPR, |c, p| lit("=")(c, p), Action::None),
        t(K::End, &[PreDirective], |c, p| keyword("end", "{end}")(c, p), Action::PopState).prefix(|c, p| literal_prefix(c, p, &["end", "{end}"])),
        t(K::IfDirective, &[PreDirective], |c, p| keyword("if", "{if}")(c, p), Action::SwitchToDirective).prefix(|c, p| literal_prefix(c, p, &["if", "{if}"])),
        t(K::ElseIf, &[PreDirective], |c, p| keyword("elseif", "{elseif}")(c, p), Action::SwitchToDirective).prefix(|c, p| literal_prefix(c, p, &["elseif", "{elseif}"])),
        t(K::Else, &[PreDirective], |c, p| keyword("else", "{else}")(c, p), Action::PopState).prefix(|c, p| literal_prefix(c, p, &["else", "{else}"])),
        t(K::IntegerLiteral, NUMBER, integer_literal, Action::IntegerLiteral).prefix(number_prefix),
        t(K::FloatingPointLiteral, NUMBER, floating_point_literal, Action::FloatingPointLiteral).prefix(number_prefix),
        t(K::Word, WORD, word, Action::None),
        t(K::BracketedWord, WORD, bracketed_word, Action::None).prefix(|c, p| if c.get(p) == Some(&'{') { 1 + word_at(c, p + 1).unwrap_or(0) } else { 0 }),
        t(K::Identifier, &[PreReference, RefModifier, RefMod2], identifier, Action::Identifier),
        t(K::OldIdentifier, &[PreOldReference, OldRefModifier], old_identifier, Action::Identifier),
        t(K::Dot, DOT, dot, Action::Dot).prefix(|c, p| usize::from(c.get(p) == Some(&'.'))),
        t(K::LCurly, CURLY, |c, p| lit("{")(c, p), Action::LCurly),
        t(K::RCurly, CURLY, |c, p| lit("}")(c, p), Action::RCurly),
        Rule { class: Class::Special, states: TERMINATOR, matcher: any, action: Action::Terminator, prefix: |_, _| 0 },
        Rule { class: Class::Special, states: &[PreDirective], matcher: any, action: Action::Terminator, prefix: |_, _| 0 },
        t(K::DoubleEscape, &[Default], |c, p| lit("\\\\")(c, p), Action::None).prefix(|c, p| literal_prefix(c, p, &["\\\\"])),
        t(K::Escape, &[Default], |c, p| lit("\\")(c, p), Action::None),
        t(K::Text, &[Default], text, Action::None),
        t(K::InlineText, &[Default], inline_text, Action::None),
        t(K::EmptyIndex, EMPTY_INDEX, empty_index, Action::None).prefix(|c, p| literal_prefix(c, p, &["$[", "$![", "$\\![", "$."])),
    ]
}

static RULES: std::sync::LazyLock<Vec<Rule>> = std::sync::LazyLock::new(rules);

#[derive(Clone)]
struct Saved {
    lparen: i32,
    rparen: i32,
    curly: i32,
    state: State,
}

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    pub state: State,
    stack: Vec<Saved>,
    lparen: i32,
    rparen: i32,
    curly: i32,
    in_comment: bool,
    in_set: bool,
    /// `PARSER_HYPHEN_ALLOWED`: `$a-b` is one identifier.
    hyphen: bool,
}

impl Lexer {
    pub fn new(source: &str, hyphen: bool) -> Lexer {
        let mut chars: Vec<char> = source.chars().collect();
        // `VelocityCharStream` sends a zero-width whitespace just before EOF.
        chars.push(EOF_CHAR);
        Lexer { chars, pos: 0, state: Default, stack: Vec::new(), lparen: 0, rparen: 0, curly: 0, in_comment: false, in_set: false, hyphen }
    }

    pub fn switch_to(&mut self, state: State) {
        self.state = state;
    }

    pub fn push(&mut self) {
        self.stack.push(Saved { lparen: self.lparen, rparen: self.rparen, curly: self.curly, state: self.state });
        self.lparen = 0;
        self.curly = 0;
    }

    pub fn pop(&mut self) -> bool {
        match self.stack.pop() {
            Some(s) => {
                self.lparen = s.lparen;
                self.rparen = s.rparen;
                self.curly = s.curly;
                self.state = s.state;
                true
            }
            None => {
                self.lparen = 0;
                self.state = Default;
                false
            }
        }
    }

    pub fn set_in_set(&mut self, v: bool) {
        self.in_set = v;
    }

    /// Characters not yet consumed, for the parser's `isLeftParenthesis` / `isRightParenthesis` / `isAssignment`
    /// peeks at the raw stream.
    pub fn rest(&self) -> &[char] {
        &self.chars[self.pos..]
    }

    /// `VelocityCharStream.UpdateLineColumn` over the characters before `pos` and the one at `pos`: the line and
    /// column of that character (a tab ends at the next multiple of 8, a lone `\r` ends a line, a character outside the BMP takes two columns). Past the end: the
    /// column after the last character.
    fn line_col(&self, pos: usize) -> (usize, usize) {
        let (mut line, mut column) = (1usize, 0usize);
        let (mut prev_cr, mut prev_lf) = (false, false);
        for &c in self.chars.iter().take(pos.saturating_add(1)) {
            column += 1;
            if prev_lf {
                prev_lf = false;
                column = 1;
                line += 1;
            } else if prev_cr {
                prev_cr = false;
                if c == '\n' {
                    prev_lf = true;
                } else {
                    column = 1;
                    line += 1;
                }
            }
            match c {
                '\r' => prev_cr = true,
                '\n' => prev_lf = true,
                '\t' => {
                    column -= 1;
                    column += 8 - (column & 7);
                }
                // Java reads UTF-16 units: the second one of a surrogate pair takes a column of its own.
                c if c.len_utf16() == 2 => column += 1,
                _ => {}
            }
        }
        if pos >= self.chars.len() {
            column += 1;
        }
        (line, column)
    }

    fn rparen_handler(&mut self) {
        let mut closed = self.in_comment;
        while !closed {
            if self.lparen > 0 {
                if self.lparen == self.rparen + 1 {
                    self.pop();
                } else {
                    self.rparen += 1;
                }
                closed = true;
            } else if !self.pop() {
                break;
            }
        }
    }

    fn is_pre(&self) -> bool {
        matches!(self.state, PreReference | PreDirective | PreOldReference)
    }

    /// Runs a rule's action; `image` is the token image so far (for `TOKEN` rules that edit it).
    fn act(&mut self, action: Action, image: &mut String) {
        match action {
            Action::None => {}
            Action::LoneSymbol | Action::PopState | Action::TextblockEnd => {
                if action == Action::TextblockEnd {
                    self.in_comment = false;
                }
                self.pop();
            }
            Action::IndexLBracket => {
                self.push();
                self.switch_to(RefIndex);
            }
            Action::Pipe => {
                if self.curly == 1 {
                    self.switch_to(AltVal);
                } else {
                    self.pop();
                }
            }
            Action::LeftCurley => self.curly += 1,
            Action::RightCurley => {
                self.curly -= 1;
                if self.state == AltVal && self.curly == 0 {
                    self.pop();
                }
            }
            Action::LParen => {
                if !self.in_comment {
                    self.lparen += 1;
                }
                if matches!(self.state, RefModifier | OldRefModifier) {
                    self.switch_to(RefMod2);
                }
            }
            Action::RParen => self.rparen_handler(),
            Action::Refmod2RParen => self.switch_to(RefMod3),
            Action::SetDirective => {
                if !self.in_comment {
                    self.push();
                    self.in_set = true;
                    self.switch_to(Directive);
                    self.lparen += 1;
                }
            }
            Action::Dollar => {
                if !self.in_comment {
                    if matches!(self.state, Reference | PreReference | PreOldReference) {
                        self.pop();
                    }
                    let pre = if self.hyphen { PreOldReference } else { PreReference };
                    if !self.is_pre() {
                        self.push();
                    }
                    self.switch_to(pre);
                }
            }
            Action::TextblockStart | Action::FormalCommentStart | Action::MultiLineCommentStart => {
                if action == Action::FormalCommentStart && !self.in_comment {
                    self.pos -= 1;
                }
                if !self.in_comment {
                    self.in_comment = true;
                    if !self.is_pre() {
                        self.push();
                    }
                    self.switch_to(match action {
                        Action::TextblockStart => InTextblock,
                        Action::FormalCommentStart => InFormalComment,
                        _ => InMultiLineComment,
                    });
                }
            }
            Action::Hash => {
                if !self.in_comment {
                    if matches!(self.state, Reference | PreReference | PreOldReference | RefModifier | OldRefModifier) {
                        self.pop();
                    }
                    if !self.is_pre() {
                        if self.state == RefMod3 {
                            self.switch_to(Default);
                        }
                        self.push();
                    }
                    self.switch_to(PreDirective);
                }
            }
            Action::SingleLineCommentStart => {
                if !self.in_comment {
                    if matches!(self.state, Reference | PreReference | PreOldReference) {
                        self.pop();
                    }
                    self.in_comment = true;
                    self.push();
                    self.switch_to(InSingleLineComment);
                }
            }
            Action::CommentEnd => {
                self.in_comment = false;
                self.pop();
                if matches!(self.state, Reference | RefMod3) {
                    self.pop();
                }
            }
            Action::Newline => self.in_set = false,
            Action::StringLiteral => {
                if self.state == Directive && !self.in_set && self.lparen == 0 {
                    self.pop();
                }
            }
            Action::SwitchToDirective => self.switch_to(Directive),
            Action::IntegerLiteral => {
                if image.ends_with("..") {
                    self.pos -= 2;
                    image.truncate(image.len() - 2);
                }
                if self.lparen == 0 && !self.in_set && !matches!(self.state, RefMod2 | RefIndex | AltVal) {
                    self.pop();
                }
            }
            Action::FloatingPointLiteral => {
                if self.lparen == 0 && !self.in_set && !matches!(self.state, RefMod2 | AltVal) {
                    self.pop();
                }
            }
            Action::Identifier => {
                if matches!(self.state, PreReference | PreOldReference) {
                    self.switch_to(Reference);
                }
            }
            Action::Dot => {
                self.pos -= 1;
                // The image is just `.`, whatever `MORE` text preceded it.
                *image = ".".into();
                self.switch_to(if self.hyphen { OldRefModifier } else { RefModifier });
            }
            Action::LCurly => self.curly += 1,
            Action::RCurly => {
                while self.curly == 0 && self.state != Default {
                    self.pop();
                }
                if !matches!(self.state, Default | Directive | RefMod2) {
                    self.pop();
                }
            }
            Action::Terminator => {
                self.pos -= 1;
                self.pop();
            }
        }
    }

    /// The `TokenMgrError` when no rule matches at `self.pos` (the token, with its `MORE` text, began at `start`).
    /// The automaton reads on as long as some rule could still match (`Rule::prefix`) and fails at the next
    /// character: there, with the characters it read in this attempt as the prefix when there were any besides the
    /// failing one (`curPos > 1`). Failing at the end-of-input sentinel, or reading past it, is `<EOF>` one column
    /// after the sentinel, the sentinel itself part of the prefix.
    fn no_match(&self, start: usize) -> VelocityError {
        let p = self.pos;
        let len = self.chars.len();
        let read = RULES.iter().filter(|r| r.states.contains(&self.state)).map(|r| (r.prefix)(&self.chars, p)).max().unwrap_or(0).min(len - p);
        let at = p + read;
        if at + 1 >= len {
            let (line, column) = self.line_col(len - 1);
            let after: String = if len - p > 1 { self.chars[start..].iter().collect() } else { String::new() };
            return lexical_error(self.state, line, column + 1, None, &after);
        }
        let c = self.chars[at];
        let (line, mut column) = self.line_col(at);
        let mut units = [0u16; 2];
        if c.encode_utf16(&mut units).len() == 2 {
            // `line_col` gives the column of the second UTF-16 unit; the automaton fails at the first.
            column -= 1;
        }
        let after: String = if read > 0 { self.chars[start..at].iter().collect() } else { String::new() };
        lexical_error(self.state, line, column, Some(units[0]), &after)
    }

    pub fn next_token(&mut self) -> Result<Token, VelocityError> {
        let mut specials = Vec::new();
        'token: loop {
            let start = self.pos;
            if start >= self.chars.len() {
                // The generated token manager begins the EOF token at the last character read (the sentinel).
                let (line, column) = self.line_col(self.chars.len() - 1);
                return Ok(Token { kind: Kind::Eof, image: String::new(), specials, line, column });
            }
            loop {
                let mut best: Option<(usize, usize)> = None;
                for (i, r) in RULES.iter().enumerate() {
                    if !r.states.contains(&self.state) {
                        continue;
                    }
                    if let Some(n) = (r.matcher)(&self.chars, self.pos) {
                        if n > 0 && best.is_none_or(|(_, b)| n > b) {
                            best = Some((i, n));
                        }
                    }
                }
                let Some((i, n)) = best else {
                    return Err(self.no_match(start));
                };
                let rule = &RULES[i];
                self.pos += n;
                let mut image: String = self.chars[start..self.pos].iter().collect();
                match rule.class {
                    Class::More => {
                        self.act(rule.action, &mut image);
                        if self.pos >= self.chars.len() {
                            // The next character cannot be read: `curPos` is 0, so no prefix.
                            let (line, column) = self.line_col(self.chars.len() - 1);
                            return Err(lexical_error(self.state, line, column + 1, None, ""));
                        }
                    }
                    Class::Skip => {
                        self.act(rule.action, &mut image);
                        continue 'token;
                    }
                    Class::Special => {
                        self.act(rule.action, &mut image);
                        specials.push(image);
                        continue 'token;
                    }
                    Class::Token(kind) => {
                        let (line, column) = self.line_col(start);
                        self.act(rule.action, &mut image);
                        return Ok(Token { kind, image, specials, line, column });
                    }
                }
            }
        }
    }
}
