#!/usr/bin/env python3
"""Generates malformed templates for the parse-error vectors: mutations (truncations, deletions, substitutions,
insertions) of seed templates taken from tests/cases/*.json, the upstream test resources and a hand-written list.
Deterministic. Usage: gen_parse_error_cases.py OUT.json [MAX_CASES [SEED [RANDOM_MUTATIONS_PER_SEED]]]. The outputs are parsed with the
real Velocity 2.4.1 by generators/record_parse_errors.py."""
import json, os, random, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

HAND = [
    "#set($a = 1)", "#set($a = [1,2,3])", "#set($a = {'k': 'v', 'n': [1, 2]})", "#set($a = [1..5])", "#set($a = $b.c($d, 'e'))",
    "#if($a == 1)x#elseif($b)y#else z#end", "#foreach($i in [1..3])$i#between,#else none#end", "#foreach($i in $list)$i#end",
    "#macro(m $a $b)$a$b#end#m(1 2)", "#macro(m)x#end #m()", "#@m($a)body#end", "#define($b)x#end$b",
    "#include('a.vm')", "#parse('a.vm')", "#evaluate('#set($x = 1)')", "#break", "#stop", "#[[ text ]]#", "#* c *# x", "## c\nx",
    "$a.b.c", "${a.b}", "$!a", "$!{a}", "$a[0]", "$a['k']", "$a.b(1, 2)", "$a.b($c.d, [1], {'a': 1})", "${a.b[0].c(1)}",
    "$a.b[0]", "$a[$b]", "$a.b($c)[0]", "$a|'d'", "${a|'d'}", "$a.b|$c",
    "#set($a = 1 + 2 * 3 - 4 / 5 % 6)", "#set($a = ($b && $c) || !$d)", "#set($a = $b == $c || $d != $e)",
    "#set($a = $b < $c && $d >= $e && $f > $g && $h <= $i)", "#if($a && $b || $c)x#end", "#if(!$a)x#end", "#if($a)x#end",
    "#set($a = \"x $b y\")", "#set($a = 'x')", "#set($a = 1.5)", "#set($a = -1)", "#set($a = true)", "#set($a = false)",
    "#{if}($a)x#{end}", "#{set}($a = 1)", "#{foreach}($i in $l)x#{end}", "\\#if x", "\\\\#if x", "\\$a \\\\$a",
    "#foreach($i in $list)\n  $i\n#end\n", "  #set($a = 1)\n  #if($a)\n  x\n  #end\n", "#if($a)\nx\n#else\ny\n#end",
    "#m($a, $b)", "#m($a $b)", "#@m($a $b)x#end", "#macro(m $a $b = 1)x#end", "#macro(m $a, $b)x#end",
    "#set($a.b = 1)", "#set($a[0] = 1)", "#set($a.b(1) = 1)", "#set(a = 1)", "#set($a == 1)", "#set($a)", "#set()",
    "#if()x#end", "#if($a x#end", "#foreach()x#end", "#foreach($i)x#end", "#foreach($i in)x#end", "#foreach($i of $l)x#end",
    "$a.", "$a..b", "$a.b.", "${a", "${a.}", "$a[", "$a[]", "$a[1", "$a.b(", "$a.b(1", "$a.b(1,", "$a.b(,)", "$a.b(1,,2)",
    "#set($a = [", "#set($a = [1,", "#set($a = [1 2])", "#set($a = {", "#set($a = {'k'", "#set($a = {'k':", "#set($a = {'k': 1,",
    "#set($a = {k: 1})", "#set($a = {'k' 1})", "#set($a = [1..])", "#set($a = [..2])", "#set($a = [1.2])", "#set($a = 1 +)",
    "#set($a = (1)", "#set($a = 1)", "#set($a = ((1))", "#set($a = !)", "#set($a = $b ||)", "#set($a = $b &&)", "#set($a = $b ==)",
    "#else", "#end", "#elseif($a)", "#if($a)x#else", "#if($a)x#elseif", "#if($a)x#elseif()y#end", "#if($a)x#else#else y#end",
    "#macro", "#macro(", "#macro()x#end", "#macro(m", "#macro(m $a", "#macro($m)x#end", "#macro(m 1)x#end", "#macro(m $a)x",
    "#foreach($i in $l)x", "#define", "#define()x#end", "#define($a", "#include", "#include(", "#include()", "#include('a', 'b'", "#parse()",
    "#evaluate()", "#evaluate(", "#break(", "#stop(", "#break($a", "#@", "#@m", "#@m(", "#@m($a)x", "##", "#*", "#* x", "#[[", "#[[ x",
    "#set($a = \"", "#set($a = 'x", "$a.b(\"x", "#if('x", "#if(\"x\"", "#set($a = 1)#set($b = ", "x #set", "x #set(", "x ## y\n#set($a=",
    "#{", "#{if", "#{if}", "#{if}(", "#{end}", "#{foo}", "#foo", "#foo(", "#foo($a", "#foo($a b)", "#foo(,)", "#foo($a,)", "#foo(1 2 3)",
    "#foo([1..3])", "#foo({'a':1})", "#foo(['a'])", "#foo(true false)", "#foo(-1)", "#foo(1.5)", "#foo(\"x\")", "#foo('x')", "#foo(x)",
    "#foo(#bar)", "#foo(\n  $a\n  $b\n)", "#foo( $a , $b )", "#foo($a)\n#end", "$a.b[0](1)", "$a[0][1]", "$a[0].b[1]", "$a.b.c[", "${a.b.c",
    "${a[0]", "${a b}", "${a.b(}", "$a.b(1))", "$a)", "(", ")", "{", "}", "[", "]", "|", "||", "\\", "\\\\", "$", "#", "$!", "$!{", "${", "$[",
    "x $", "x #", "x $!", "x ${a", "$a|", "$a|'d", "${a|", "$a.b|", "$a[0]|", "$a.b(1)|'x'",
    # line endings, tabs and non-ASCII text move the reported columns and lines
    "#if($a)\r\nx\r\n#else", "a\r$b.(", "#set($a =\r\n", "a\r\rb\r$a.", "\t#if(", "#if(\t$a", "a\tb\t$c.(", "\t\t#set($a = ",
    "\u00e9$a.b(", "#if($\u00e9", "\u00e9\u00e9\n#if(", "\U0001f600#if(", "\U0001f600\U0001f600$a.(", "x\u2028y\n$a.(", "$a.b(\u00a0)", "#set($a\u3000=\u3000", "#macro(m $a\u3000",
]

SEPARATORS = [")", "(", "]", "[", "}", "{", "$", "#", "'", '"', ",", ".", "..", ":", "=", "==", "!", "&&", "||", "+", "-", "*", "/", "%", "#end", "#else", " ", "\n", "$a", "1", "#if($a)", "\\", "#*", "##", "#[[", "$!", "${", "|"]


def seeds():
    out = list(HAND)
    for f in ("core.json", "tools.json"):
        for c in json.load(open(os.path.join(ROOT, "tests/cases", f))):
            out.append(c["template"])
    up = os.path.join(ROOT, "upstream/velocity-engine-core/src/test")
    for d, _, files in os.walk(up):
        for n in sorted(files):
            if n.endswith((".vm", ".vtl", ".txt")):
                try:
                    t = open(os.path.join(d, n), encoding="utf-8").read()
                except Exception:
                    continue
                if 0 < len(t) <= 300:
                    out.append(t)
    seen, res = set(), []
    for s in out:
        if s not in seen and len(s) <= 300:
            seen.add(s)
            res.append(s)
    return res


def mutations(s, rng, per):
    out = []
    n = len(s)
    for i in range(1, n):
        out.append(s[:i])  # truncations
    for i in range(n):
        out.append(s[:i] + s[i + 1:])  # deletions
    for _ in range(per):
        i = rng.randrange(n + 1)
        out.append(s[:i] + rng.choice(SEPARATORS) + s[i:])  # insertions
        if n:
            j = rng.randrange(n)
            out.append(s[:j] + rng.choice(SEPARATORS) + s[j + 1:])  # substitutions
    return out


def main():
    outp = sys.argv[1]
    limit = int(sys.argv[2]) if len(sys.argv) > 2 else 20000
    seed = int(sys.argv[3]) if len(sys.argv) > 3 else 231
    per = int(sys.argv[4]) if len(sys.argv) > 4 else 6
    rng = random.Random(seed)
    cases, seen = [], set()
    for s in seeds():
        cases.append(s)
        for m in mutations(s, rng, per):
            if m not in seen and m != s:
                seen.add(m)
                cases.append(m)
    rng.shuffle(cases)
    cases = cases[:limit]
    json.dump([{"template": t} for t in cases], open(outp, "w"))
    print(len(cases), "cases")


if __name__ == "__main__":
    main()
