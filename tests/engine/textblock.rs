//! `TextblockTestCase`, `HyphenInIdentifiersTestCase`, `UnicodeEscapeTestCase`.

use std::borrow::Cow;

use crate::base::*;

/// `ASTTextblock.START` / `END`.
const START: &str = "#[[";
const END: &str = "]]#";
const PARTIAL_START: &str = "#[";
const PARTIAL_END: &str = "]#";
const END_OF_START: &str = "[";
const START_OF_END: &str = "]";

fn textblock(s: &str) -> String {
    format!("{START}{s}{END}")
}

fn assert_textblock_eval_equals(s: &str) {
    Base::new().assert_eval_equals(s, &textblock(s));
}

const AJAX: &str = "var myId = 'someID';$('#test).append($.template('<div id=\"${myId}\"></div>').apply({myId: myId}));";

#[test]
fn test_textblock_ajaxcode() {
    Base::new().assert_eval_equals(&format!("{AJAX} 123"), &format!("{} #foreach($i in [1..3])$i#end", textblock(AJAX)));
}

#[test]
fn test_loose_textblock_end() {
    Base::new().assert_eval_equals(&format!("{END} "), &format!("{END} "));
}

#[test]
fn test_textblock_start_in_textblock() {
    assert_textblock_eval_equals(START);
}

#[test]
fn test_textblock_end_between_two_textblock_halves() {
    Base::new().assert_eval_equals(&format!(" {END}  "), &format!("{}{END} {}", textblock(" "), textblock(" ")));
}

#[test]
fn test_zerolength_textblock() {
    assert_textblock_eval_equals("");
}

#[test]
fn test_textblock_inside_foreach_loop() {
    Base::new().assert_eval_equals(&format!("1 {AJAX}2 {AJAX}3 {AJAX}"), &format!("#foreach($i in [1..3])$i {}#end", textblock(AJAX)));
}

#[test]
fn test_single_hash_inside_textblock() {
    assert_textblock_eval_equals(" # ");
}

#[test]
fn test_dollar_inside_textblock() {
    assert_textblock_eval_equals("$");
}

#[test]
fn test_textblock_inside_comment() {
    Base::new().assert_eval_equals("", &format!("#* comment {} *#", textblock("FOOBAR")));
}

#[test]
fn test_partial_start_end_tokens_inside_textblock() {
    assert_textblock_eval_equals(&format!("{PARTIAL_START}foo{PARTIAL_END}"));
}

#[test]
fn test_dupe_token_chars() {
    assert_textblock_eval_equals(&format!("{END_OF_START}{START_OF_END}"));
    assert_textblock_eval_equals(&format!("{END_OF_START}{END_OF_START}{START_OF_END}{START_OF_END}"));
    assert_textblock_eval_equals(&format!("{END_OF_START}{END_OF_START}#{START_OF_END}{START_OF_END}"));
}

#[test]
fn test_server_side_include_escaping() {
    assert_textblock_eval_equals("<!--#include file=\"wisdom.inc\"--> ");
}

#[test]
fn test_line_comment_inside_textblock() {
    assert_textblock_eval_equals("##x");
}

fn hyphen() -> Base {
    let mut b = Base::new();
    b.add_property("parser.allow_hyphen_in_identifiers", true);
    b
}

#[test]
fn test_hyphen() {
    hyphen().assert_eval_equals("6", "#set($var-1 = 7)#set($var-2 = $var-1 - 1)$var-2");
}

#[test]
fn test_hyphen_in_collection() {
    hyphen().assert_eval_equals("foofoofoo", "#set($map = {'name-with-hyphen':'foo'})$map.name-with-hyphen${map.name-with-hyphen}${map.get('name-with-hyphen')}");
}

#[test]
fn test_unicode_escape() {
    Base::new().assert_eval_equals("a", "#set($v = \"\\u0061\")$v");
}

#[track_caller]
fn assert_unescape(expected: &str, escaped: &str) {
    let unescaped = velocity::string_literal_unescape(escaped);
    assert_eq!(unescaped, expected);
    if escaped == expected {
        assert!(matches!(unescaped, Cow::Borrowed(u) if std::ptr::eq(u, escaped)), "the same string comes back");
    }
}

#[test]
fn test_ast_string_literal_unescape() {
    assert_unescape("", "");
    assert_unescape("bebe", "bebe");
    assert_unescape("as\\nsd", "as\\nsd");
    assert_unescape("a", "\\u0061");
    assert_unescape("abc", "\\u0061bc");
    assert_unescape("\u{0061}bc\u{0064}", "\\u0061bc\\u0064");
    assert_unescape("z\u{0061}bc\u{0064}f", "z\\u0061bc\\u0064f");
}
