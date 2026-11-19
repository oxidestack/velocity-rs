#!/usr/bin/env python3
"""Records tests/cases/parse_errors.json and parse_errors.expected.json from the real Velocity 2.4.1.

  1. gen_parse_error_cases.py writes ~40k malformed templates (deterministic);
  2. parse_errors.sh (ParseErrors.java: Velocity 2.4.1 from Maven Central, MockServer's configuration, a fresh engine
     per template so recorded cases do not depend on each other) parses them;
  3. a deterministic, diverse subset of the failures is kept: every hand-written seed that fails to parse, and per
     distinct "Was expecting" list / first-token pair up to PER_GROUP mutations, plus samples of the other parse errors (not the lexical ones).

Run from the repository root: python3 generators/record_parse_errors.py. Regenerating reproduces the committed
files byte for byte (the generator and the selection use fixed seeds, the reference is deterministic)."""
import base64, collections, json, os, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PER_GROUP = 3
OTHER_PER_KIND = 12


def main():
    with tempfile.TemporaryDirectory() as tmp:
        big = os.path.join(tmp, "big.json")
        subprocess.run([sys.executable, os.path.join(ROOT, "generators/gen_parse_error_cases.py"), big, "200000"], check=True, stdout=subprocess.DEVNULL)
        cases = json.load(open(big))
        lines = "".join(base64.b64encode(c["template"].encode("utf-8")).decode() + "\n" for c in cases)
        out = subprocess.run([os.path.join(ROOT, "generators/parse_errors.sh")], input=lines.encode(), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=True).stdout
        results = []
        for line in out.decode().split("\n"):
            if line:
                r = base64.b64decode(line).decode("utf-8")
                results.append({} if r == "O" else {"error": r[1:]})
    assert len(results) == len(cases)
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import gen_parse_error_cases
    hand = set(gen_parse_error_cases.HAND)
    picked, groups, kinds = [], collections.Counter(), collections.Counter()
    for c, r in zip(cases, results):
        e = r.get("error")
        if not e or not e.startswith("org.apache.velocity.exception.ParseErrorException"):
            continue
        t = c["template"]
        if "Was expecting" in e:
            first = e.split('Encountered "', 1)[1].split('" at ', 1)[0]
            key = (e.split("Was expecting", 1)[1], first if len(first) <= 3 else "<long>")
            if t in hand or groups[key] < PER_GROUP:
                groups[key] += 1
                picked.append((t, e))
        elif "Lexical error" not in e:  # TokenMgrError messages (lexer), not part of the expected-token lists
            kind = " ".join(e.split("\n")[0].split(" ")[:5])
            if t in hand or kinds[kind] < OTHER_PER_KIND:
                kinds[kind] += 1
                picked.append((t, e))
    picked.sort(key=lambda p: p[0])
    with open(os.path.join(ROOT, "tests/cases/parse_errors.json"), "w") as f:
        json.dump([{"template": t} for t, _ in picked], f, indent="\t", ensure_ascii=True)
        f.write("\n")
    with open(os.path.join(ROOT, "tests/cases/parse_errors.expected.json"), "w") as f:
        json.dump([{"error": e} for _, e in picked], f, indent="\t", ensure_ascii=True)
        f.write("\n")
    print(len(picked), "cases,", len(groups), "expected-list groups")


main()
