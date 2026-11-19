#!/usr/bin/env python3
"""Records tests/cases/lexical_errors.json and lexical_errors.expected.json from the real Velocity 2.4.1: templates the
lexer cannot read (`TokenMgrError`, reported as `Lexical error, ...`).

  1. gen_parse_error_cases.py writes the same malformed templates record_parse_errors.py starts from, plus HAND below;
  2. parse_errors.sh runs them through `evaluate` (the error names no template: `*unset*`; inside `#evaluate` the raw
     `TokenMgrError` text);
  3. a deterministic, diverse subset of the lexical errors is kept: every HAND template, and up to PER_GROUP per
     (message form, lexical state, EOF or character class, prefix class);
  4. the kept templates are run again through `getTemplate` (`parse_errors.sh template`), where the error names the
     template.

Run from the repository root: python3 generators/record_lexical_errors.py. Regenerating reproduces the committed files
byte for byte."""
import base64, collections, json, os, re, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PER_GROUP = 3

HAND = [
    # #evaluate passes the ParseException text on unchanged
    "#evaluate('#if(\"')", "#evaluate('$a.b(\\'x')", "#evaluate('#set($a = 1 &: 2)')", "#evaluate('x\n#if($a.|')",
    # characters outside the BMP: Java's token manager reads UTF-16 units
    "#if($\U0001f600)", "$a[\U0001f600]", "#set($a = \U0001f600)", "\U0001f600\U0001f600#if(.a)", "$a.b(\U0001f600.)",
    # a prefix that itself reads like the location ParseErrorException looks for
    "#set($a = \"line 9, column 9. x", "#set($a = 'line 12, column 34.", "#if(\"x line 1, column 2.)", "#set($a = \"Lexical error TokenMgrError line 3, column 4.",
    # tabs, line ends and escapes in front of the failure
    "\t#if($a &: $b)", "x\r\n#if(.a)", "a\rb\r#set($a = .x)", "#set($a = \"\t\n", "#set($a = \"\\", "#set($a = \"x\r",
    # unterminated blocks and comments
    "#[[ x", "#[[", "x #[[ y\n z", "${a|(", "$a[0(]", "${a|'b'(", "#if($a == \"x)", "#set($a = 'x",
]


def run(templates, *args):
    lines = "".join(base64.b64encode(t.encode("utf-8")).decode() + "\n" for t in templates)
    out = subprocess.run([os.path.join(ROOT, "generators/parse_errors.sh"), *args], input=lines.encode(), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=True).stdout
    results = []
    for line in out.decode().split("\n"):
        if line:
            r = base64.b64decode(line).decode("utf-8")
            results.append(None if r == "O" else r[1:])
    assert len(results) == len(templates)
    return results


def char_class(c):
    if c.isascii() and c.isalpha():
        return "letter"
    if c.isascii() and c.isdigit():
        return "digit"
    return c if c.isascii() else "non-ascii"


def key(error):
    form = "raw" if "TokenMgrError" in error else "located"
    m = re.search(r"Encountered: (<EOF>|'((?:[^'\\]|\\.)*)' \((\d+)\),)( after prefix \"((?:[^\"\\]|\\.)*)\")?( \(in lexical state (\d+)\))?", error)
    if not m:
        return ("unmatched",)  # the prefix reads like a location (HAND)
    found = "EOF" if m.group(1) == "<EOF>" else char_class(chr(int(m.group(3))))
    prefix = m.group(5)
    prefix = "none" if prefix is None else char_class(prefix[0]) + ("-long" if len(prefix) > 2 else "")
    return (form, m.group(7), found, prefix)


def main():
    with tempfile.TemporaryDirectory() as tmp:
        big = os.path.join(tmp, "big.json")
        subprocess.run([sys.executable, os.path.join(ROOT, "generators/gen_parse_error_cases.py"), big, "200000"], check=True, stdout=subprocess.DEVNULL)
        cases = [c["template"] for c in json.load(open(big))]
    evaluated = run(cases + HAND)
    picked, groups = {}, collections.Counter()
    for t, e in zip(cases + HAND, evaluated):
        if not e or "Lexical error" not in e or t in picked:
            continue
        k = key(e)
        if t in HAND or groups[k] < PER_GROUP:
            groups[k] += 1
            picked[t] = e
    missing = [t for t in HAND if t not in picked]
    assert not missing, f"hand-written templates without a lexical error: {missing}"
    templates = sorted(picked)
    via_template = run(templates, "template")
    with open(os.path.join(ROOT, "tests/cases/lexical_errors.json"), "w") as f:
        json.dump([{"template": t} for t in templates], f, indent="\t", ensure_ascii=True)
        f.write("\n")
    with open(os.path.join(ROOT, "tests/cases/lexical_errors.expected.json"), "w") as f:
        json.dump([{"evaluate": picked[t], "template": g} for t, g in zip(templates, via_template)], f, indent="\t", ensure_ascii=True)
        f.write("\n")
    print(len(templates), "cases,", len(groups), "groups")


main()
