#!/usr/bin/env python3
"""The publication gate: nothing private reaches a pushed tree or commit message — no company's name, nothing of the
author's machine or environment, no internal host, no local path, and (the worklog rule) nothing of the worklog.

    python3 tools/public/public_check.py                     the working copy of every tracked file, the messages of
                                                             the commits no remote has, and HEAD against the archive
                                                             (`make public-check`)
    python3 tools/public/public_check.py --accept <file> <n> prints the accepted-hits entry for line n of a file
    python3 tools/public/public_check.py --pre-push <remote> the same for each ref git's pre-push hook names on stdin
                                                             (`<local ref> <local sha> <remote ref> <remote sha>`):
                                                             the pushed commit's tree, the pushed commits' messages

Two sets of patterns, each a ripgrep regular expression matched case-insensitively:

- the denylist, which names the private things and so is never tracked: `$PUBLIC_DENYLIST`, default
  `~/.config/oxidestack/public-denylist` (one per organisation: the organisation's other projects share the same
  private names); one pattern per line, `#` starts a comment, blank lines ignored. A missing or empty denylist fails
  the check; `PUBLIC_DENYLIST=none` runs the built-in half alone (a stranger's clone, a fork's CI);
- the built-in local paths and internal host names (BUILT_IN), generic, naming nothing. A placeholder passes:
  `/Users/…`, `/home/<user>/…`, `~/…`, `~/Projects/…` — what follows the directory is not a path component. Only a
  home directory counts: `/Users` or `/home` that ends an identifier or follows `}` (`/scim/v2/Users/1`,
  `{basePath}/Users/{id}`, `/users/1`) is a route, not a path; `/Users` is matched with its capital U.

Verbatim third-party content (a vendored upstream file) cannot be scrubbed. `tools/public/exempt-paths.txt` lists its
path globs, each with its reason; those files skip the built-in patterns and the worklog rule, never the denylist.

Verbatim or recorded data can carry a denylist hit that is public (a city, a company or a bot name in a data set of
upstream's). `tools/public/accepted-hits.txt` lists such lines: `<path>  <sha256 of the line>  # <reason>`, made by
`--accept <file> <n>`. The entry names no pattern, and a line that changes no longer matches it. Only denylist hits are
accepted that way. The recorded test data (tests/scenarios, tests/files) skips the internal-host pattern, since it names
the hosts upstream's tests resolve; an internal-host match followed by `.word` or `(` (a Java property key, a package,
a field) is code, not a host, and so is one in a source file outside a string literal and a comment (a field access).
The home of an image's own user (IMAGE_HOMES: `/home/nonroot`) names no person and is not a local path.

The worklog rule: the worklog — the tickets, the decision records, the research notes, the ticket tree and the agents'
files — lives in a private repository of its own, and the code repository never names it. With the rule on, a tracked
worklog path (WORKLOG_DIRS, WORKLOG_FILES, an agents' instruction file anywhere) fails, and so does a reference to the
worklog (REFERENCES) in a tracked file or a checked commit message: a ticket, decision record or research number
(`ticket NNNN`, `Ticket NNNN:`, `tickets NNNN and NNNN`, `ADR NNNN`, `research NNNN`, and in an identifier
`adr_NNNN`, `TICKET_NNNN`, `AdrNNNN`), a ticket's possessive
(`NNNN's`), the worklog's commit conventions (`Land NNNN: …`, `Close NNNN-NNNN`), or a worklog path named in text;
NNNN is four digits, the first a zero; after the word `ticket` two to four digits (the first tickets had three).
A range (`NNNN-NNNN`) is not one inside a UUID or another hex run, nor after a sign (`-0500 -0500`, a UTC offset). The patterns are case-insensitive and run over a whole text, so a reference
split across lines, comment leaders between its words included, is found. A commit message is held stricter still
(MESSAGE_REFERENCES): any number shaped like a ticket's, `NNNN` or `NNNNb`, standing alone fails.
`PUBLIC_WORKLOG=on|off` sets the rule; unset, WORKLOG_DEFAULT does: `on`, since the worklog moved to a repository of
its own (`off` checks a tree from before the move).

A ref that contains the archive (the tag `pre-publication`, the private history before the public repository's one
fresh root commit) is refused. The tag is looked up locally,
then on the remote `$PUBLIC_ARCHIVE_REMOTE` (default `private`, the private mirror's remote) when that
remote exists. A remote that exists and cannot be asked fails the check: whether the ref contains the archive is then
unknown, and a gate refuses what it cannot check.

With `--pre-push`, each stdin line is `<local ref> <sha> <remote ref> <sha>`; one of another shape fails the check, for
the same reason. No ref line at all is git's up-to-date push: nothing is pushed, so nothing can leak, and the check
passes quietly.

Every pattern is matched against each file's path as well as its contents: a name in a directory or a file name is
published too.

Exit 0: clean. 1: findings, one line each (`path:line: pattern`, `path: path: pattern`, `commit <sha>: line N:
pattern`, `path: worklog path`, `path:line: worklog reference: <text> (<kind>)`, `refused: …`).
2: no usable denylist, a bad pattern, a bad PUBLIC_WORKLOG, a missing tool (git, rg), a malformed pre-push
ref line, or an archive remote that cannot be asked: something could not be checked. Python stdlib, git and
ripgrep only.
"""

import base64
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

DEFAULT_DENYLIST = "~/.config/oxidestack/public-denylist"
ARCHIVE_TAG = "pre-publication"
ZERO = "0" * 40
# A path component: a real path names a user or a project here; `…`, `<user>`, `*` or a backtick do not.
COMPONENT = r"[a-z0-9._-]"
BUILT_IN = [
    # A home directory: the directory is not the tail of an identifier, a `}` or a `.`/`-` (`/scim/v2/Users/1`,
    # `{basePath}/Users/{id}` are product routes, not paths).
    ("local path", rf"(?:^|[^\w}}.-])(?-i:/Users/){COMPONENT}"),
    ("local path", rf"(?:^|[^\w}}.-])/home/{COMPONENT}"),
    ("local path", rf"~/Projects/{COMPONENT}"),
    ("internal host", r"\b[a-z0-9][a-z0-9-]*(\.[a-z0-9-]+)*\.(localhost|local|internal)\b"),
]
# A private-range address is not built in: the tree's are test and example data (a Kubernetes service address, the
# egress test's private network), which no generic pattern tells from the stand's. Those go in the denylist.
# Names every Docker or Kubernetes install answers to; they name no infrastructure of ours.
GENERIC_HOST_SUFFIXES = (".docker.internal", ".cluster.local")
# Java names that end in `.internal` or `.local` (the JDK's and JUnit's own packages, Maven's local repository
# property): code that names them names no host.
JAVA_NAMES = ("jdk.internal", "org.junit.internal", "maven.repo.local")
# Home directories of the images' own users, which name no person: distroless's `nonroot` (the shipped image's user,
# whose home is its working directory and what its JVM properties report).
IMAGE_HOMES = ("nonroot",)
# Source files, where an internal-host match outside a string literal and a comment is code (a field access), since a
# host name in source is always quoted or in a comment.
# Not Python: its docstrings span lines, so a line of one has no quote to tell it from code.
SOURCE_SUFFIXES = (".rs", ".java", ".kt", ".js", ".ts", ".go", ".c", ".h")

# The worklog rule's state when PUBLIC_WORKLOG is unset: `on`, since the worklog lives in a repository of its own.
# `PUBLIC_WORKLOG=off` checks a tree from before the move.
WORKLOG_DEFAULT = "on"
# The worklog's paths: its directories, then its files. An agents' instruction file is the worklog's wherever it sits.
# A file's name is spelt in two pieces so that this file does not name a worklog path itself.
WORKLOG_DIRS = ("tickets", "docs/adr", "docs/research", "docs/agents", ".claude", "prototypes")
WORKLOG_FILES = tuple(
    name + ext
    for name, ext in (
        ("docs/tree", ".md"),
        ("docs/milestones", ".md"),
        ("docs/positioning", ".md"),
        ("CONTEXT", ".md"),
        ("tools/ticket_tree", ".py"),
        ("tools/tests/test_ticket_tree", ".py"),
    )
)
AGENT_FILE = "CLAUDE" + ".md"
# Between the words of a reference: blanks and line breaks with any comment leader (`//`, `#`, `*`, `--`, `;`, `>`,
# `!`) on the next line, or a hyphen.
SEP = rb"[\s#/*;>!-]+"
# A worklog number: four digits, the first a zero (the tree's numbering), and a letter for a split ticket.
NUMBER = rb"0\d{3}[a-z]?\b"
REFERENCES = [
    (
        "a ticket, decision record or research number",
        rb"\b(?:tickets?|adrs?|research(?:" + SEP + rb"notes?)?|items?|branch(?:es)?)" + SEP + NUMBER,
    ),
    ("a decision record or research number", rb"\b(?:adrs?|research)" + SEP + rb"\d{4}\b"),
    # The first tickets had three digits (`ticket` and three digits); a ticket's number of two to four digits, whatever its first.
    ("a ticket number of two to four digits", rb"\btickets?" + SEP + rb"\d{2,4}[a-z]?\b"),
    # The same words joined to the number in an identifier, where `\b` does not fall: `adr_NNNNs_status`,
    # `RESEARCH_NNNN`, `AdrNNNN`. The word starts the identifier or follows an underscore or, camelCase, a lower-case
    # letter; the number is not followed by another digit.
    (
        "a ticket, decision record or research number in an identifier",
        rb"(?:(?<![a-z0-9])|(?-i:(?<=[a-z])(?=[A-Z])))"
        rb"(?:tickets?|adrs?|research(?:_*notes?)?|items?)_*0\d{3}[a-z]?(?!\d)",
    ),
    ("a ticket's possessive", rb"\b0\d{3}[a-z]?(?:'|\xe2\x80\x99)s\b"),
    ("a worklog commit convention", rb"\b(?:land(?:s|ed|ing)?|close[sd]?|closing|file[sd]?|filing)" + SEP + NUMBER),
    # Not a range: a number with a sign before it (`-0500 -0500`, `+0100 -0500`: UTC offsets; `-0000-0000-`: the groups
    # of a UUID) or one followed by another hex group (`0447-0602-ab12`: an id).
    (
        "a range of tickets",
        rb"(?<![-+])\b0\d{3}[a-z]?[ \t]*(?:-|\xe2\x80\x93)[ \t]*" + NUMBER + rb"(?!-[0-9a-f]{4,}\b)",
    ),
    (
        "a worklog path",
        rb"(?<![\w.-])(?:"
        + b"|".join(
            re.escape(name.encode())
            for name in [d + "/" for d in WORKLOG_DIRS] + list(WORKLOG_FILES) + [AGENT_FILE, "ticket_tree" + ".py"]
        )
        + rb")",
    ),
]
# A code commit's message says what changed and why; no number in it is a ticket's, so one standing alone fails.
MESSAGE_REFERENCES = REFERENCES + [("a ticket number", rb"(?<![\w.:/+-])" + NUMBER + rb"(?![.:/-]?\w)")]


class Unusable(Exception):
    """The check cannot run: exit 2."""


def git(*args, check=True):
    out = subprocess.run(["git", *args], capture_output=True, text=True, env=dict(os.environ, GIT_TERMINAL_PROMPT="0"))
    if check and out.returncode != 0:
        raise Unusable(f"git {' '.join(args)}: {out.stderr.strip()}")
    return out


def denylist_patterns():
    value = os.environ.get("PUBLIC_DENYLIST", "")
    if value == "none":
        return []
    path = Path(os.path.expanduser(value or DEFAULT_DENYLIST))
    if not path.is_file():
        raise Unusable(
            f"public-check: no denylist at {path}; it names the private things, so it is never in the repository. "
            "Create it, point PUBLIC_DENYLIST at one, or run only the built-in patterns with PUBLIC_DENYLIST=none."
        )
    patterns = []
    for number, line in enumerate(path.read_text().splitlines(), 1):
        line = line.strip()
        if line and not line.startswith("#"):
            patterns.append((f"{path}:{number}", line))
    if not patterns:
        raise Unusable(
            f"public-check: the denylist {path} has no pattern (PUBLIC_DENYLIST=none runs the built-in ones)"
        )
    return patterns


def worklog_rule():
    value = os.environ.get("PUBLIC_WORKLOG", "") or WORKLOG_DEFAULT
    if value not in ("on", "off"):
        raise Unusable(f"public-check: PUBLIC_WORKLOG is `{value}`; it is `on` or `off`")
    return value == "on"


def is_worklog_path(path):
    return (
        path in WORKLOG_FILES
        or path.split("/")[-1] == AGENT_FILE
        or any(path.startswith(d + "/") for d in WORKLOG_DIRS)
    )


def find_references(data, references):
    """Each worklog reference in `data` (bytes): yields (line number, kind, the reference with its blanks folded), a
    reference two patterns find reported once."""
    seen = set()
    for kind, pattern in references:
        for match in re.finditer(pattern, data, re.IGNORECASE):
            if match.start() in seen:
                continue
            seen.add(match.start())
            text = b" ".join(match.group(0).split()).decode(errors="replace")
            yield data.count(b"\n", 0, match.start()) + 1, kind, text


BUILT_IN_ORIGINS = frozenset(name for name, _ in BUILT_IN)
BUILT_IN_PATTERNS = frozenset(p for _, p in BUILT_IN)
EXEMPT_LIST = "tools/public/exempt-paths.txt"
ACCEPTED_LIST = "tools/public/accepted-hits.txt"
HOST_PATTERN = next(p for name, p in BUILT_IN if name == "internal host")
# Recorded and harvested test data: the oracle's goldens and the scenarios, vectors and files taken from upstream's tests
# name hosts of their own (`*.local`, `*.internal` servers and DNS names the tests resolve). They are never the author's
# environment, so the internal-host pattern skips them; the local-path patterns, the worklog rule and the denylist (which
# names the real private hosts) do not.
HOST_TEST_DATA_GLOBS = ("tests/scenarios/**", "tests/files/**")


def glob_regex(glob):
    """`**` any path, `*` anything but a slash, `?` one character but a slash; anchored."""
    out, i = "", 0
    while i < len(glob):
        if glob.startswith("**", i):
            out, i = out + ".*", i + 2
        elif glob[i] == "*":
            out, i = out + "[^/]*", i + 1
        elif glob[i] == "?":
            out, i = out + "[^/]", i + 1
        else:
            out, i = out + re.escape(glob[i]), i + 1
    return re.compile(out + r"\Z")


def host_test_data(path):
    return any(glob_regex(g).match(path) for g in HOST_TEST_DATA_GLOBS)


def line_hash(line):
    """What an accepted-hits entry names a line by: the SHA-256 of its text without the line break."""
    return hashlib.sha256(line.rstrip("\r\n").encode()).hexdigest()


def line_text(lines):
    """A matched line as ripgrep's JSON gives it: `text`, or `bytes` (base64) for a line that is not UTF-8, decoded the
    way `--accept` reads it."""
    if "text" in lines:
        return lines["text"]
    return base64.b64decode(lines.get("bytes", "")).decode(errors="replace")


def load_accepted(root):
    """The tracked list of accepted denylist hits (ACCEPTED_LIST in `root`): `<path>  <sha256 of the line>  # <reason>`
    per line. A hit of the denylist on exactly that line of that file is skipped: the line is verbatim or recorded
    upstream data (a public city, company or bot name in a data set) and cannot be scrubbed. The entry names no
    pattern, so the list tells nothing of the denylist; a changed line no longer matches its hash and fails again.
    Returns a function (path, line text) -> bool."""
    path = Path(root) / ACCEPTED_LIST
    entries = set()
    if path.is_file():
        for number, line in enumerate(path.read_text().splitlines(), 1):
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            fields, _, reason = line.partition("#")
            parts = fields.split()
            if len(parts) != 2 or not re.fullmatch(r"[0-9a-f]{64}", parts[1]) or not reason.strip():
                raise Unusable(f"public-check: {path}:{number}: a line is `<path>  <sha256 of the line>  # <reason>`")
            entries.add((parts[0], parts[1]))
    return lambda name, line: (name, line_hash(line)) in entries


def load_exempt(root):
    """The tracked list of verbatim third-party paths (EXEMPT_LIST in `root`): `<glob>  # <reason>` per line, a leading
    `!` takes a path back out (the last matching line wins), `#` alone starts a comment. Returns a function
    path -> bool. Such a path is exempt from the built-in patterns and the worklog rule only; the denylist still
    applies to it."""
    path = Path(root) / EXEMPT_LIST
    rules = []
    if path.is_file():
        for number, line in enumerate(path.read_text().splitlines(), 1):
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            glob, _, reason = line.partition("#")
            glob = glob.strip()
            if not glob or not reason.strip():
                raise Unusable(f"public-check: {path}:{number}: a line is `<glob>  # <reason>`")
            rules.append((glob_regex(glob.removeprefix("!")), not glob.startswith("!")))

    def exempt(f):
        verdict = False
        for regex, value in rules:
            if regex.match(f):
                verdict = value
        return verdict

    return exempt


def check_worklog(files, cwd, exempt=lambda f: False):
    """The worklog rule over `files` (relative to cwd): their paths, then their contents."""
    files = [f for f in files if not exempt(f)]
    findings = [f"{f}: worklog path" for f in files if is_worklog_path(f)]
    for f in files:
        data = (Path(cwd) / f).read_bytes()
        findings += [f"{f}:{n}: worklog reference: {t} ({k})" for n, k, t in find_references(data, REFERENCES)]
    return findings


def member_access(data, submatches):
    """True when every internal-host match in the line is followed by `.word` or `(`: a Java property key
    (`velocimacro.permissions.allow.inline.local.scope`), a package (`com.sun.org.apache.xerces.internal.dom.X`) or a
    field (`self.local.lock()`) is code, never a host name a URL or a config value would carry."""
    line = data["lines"].get("text", "").encode()
    return all(re.match(rb"\.\w|\(", line[m["end"] :]) for m in submatches)


def outside_literals(line, start):
    """True when byte `start` of a source line is code: not inside a `"…"` string literal and not after `//` outside
    one."""
    quoted, i = False, 0
    while i < start:
        c = line[i : i + 1]
        if quoted:
            if c == b"\\":
                i += 1
            elif c == b'"':
                quoted = False
        elif c == b'"':
            quoted = True
        elif line[i : i + 2] == b"//":
            return False
        i += 1
    return not quoted


def source_code(name, data, submatches):
    """True when every internal-host match in a source file's line is code, outside a string literal and a comment:
    a field (`conn` + `.local`) or a path expression, never a host, which source quotes."""
    if not name.endswith(SOURCE_SUFFIXES):
        return False
    line = data["lines"].get("text", "").encode()
    return all(outside_literals(line, m["start"]) for m in submatches)


def image_home(data, submatches):
    """True when every local-path match in the line is the home of an image's own user (IMAGE_HOMES)."""
    line = data["lines"].get("text", "").encode()
    homes = [h.encode() for h in IMAGE_HOMES]
    return all(
        any(re.match(re.escape(h) + rb"(?![\w.-])", line[m["end"] - 1 :]) for h in homes) for m in submatches
    )


def search(patterns, files, cwd, exempt=lambda f: False, code_aware=True, accepted=lambda f, line: False):
    """Each (origin, pattern) over `files` (relative to cwd): yields (file, line number, pattern). A built-in pattern
    skips the files `exempt` names and, for an internal host, the recorded test data (HOST_TEST_DATA); the denylist
    skips only the lines `accepted` names. `code_aware` False (the path search) reads a name's `.word` tail as part of
    the name."""
    files = list(files)
    if not files:
        return
    for origin, pattern in patterns:
        for start in range(0, len(files), 500):
            chunk = files[start : start + 500]
            out = subprocess.run(
                ["rg", "--no-config", "--json", "-i", "-a", "-e", pattern, "--", *chunk],
                cwd=cwd,
                capture_output=True,
                text=True,
                errors="replace",
            )
            if out.returncode not in (0, 1):
                raise Unusable(f"public-check: pattern {origin} `{pattern}`: {out.stderr.strip()}")
            for line in out.stdout.splitlines():
                event = json.loads(line)
                if event["type"] != "match":
                    continue
                data = event["data"]
                texts = [m["match"].get("text", "") for m in data["submatches"]]
                if origin == "internal host" and all(
                    ("." + t.lower()).endswith(GENERIC_HOST_SUFFIXES) or any(n in t.lower() for n in JAVA_NAMES)
                    for t in texts
                ):
                    continue
                if origin == "internal host" and code_aware and member_access(data, data["submatches"]):
                    continue
                name = data["path"].get("text", "?")
                if origin == "internal host" and code_aware and source_code(name, data, data["submatches"]):
                    continue
                if origin == "local path" and image_home(data, data["submatches"]):
                    continue
                if origin in BUILT_IN_ORIGINS and (exempt(name) or (origin == "internal host" and host_test_data(name))):
                    continue
                if origin not in BUILT_IN_ORIGINS and accepted(name, line_text(data["lines"])):
                    continue
                yield name, data["line_number"], pattern


def search_paths(patterns, paths, exempt=lambda f: False):
    """Each (origin, pattern) over the file paths themselves: a name in a directory or a file name is published as
    surely as one in a file. Yields (path, pattern)."""
    paths = list(paths)
    if not paths:
        return
    with tempfile.TemporaryDirectory() as tmp:
        listing = Path(tmp) / "paths"
        listing.write_text("".join(p.replace("\n", " ") + "\n" for p in paths))
        for _file, number, pattern in search(patterns, [listing.name], tmp, code_aware=False):
            if pattern in BUILT_IN_PATTERNS and exempt(paths[number - 1]):
                continue
            if pattern == HOST_PATTERN and host_test_data(paths[number - 1]):
                continue
            yield paths[number - 1], pattern


def tracked_files(root):
    """Tracked files that are on disk, submodules (mode 160000) left out: they are other repositories."""
    out = git("-C", str(root), "ls-files", "-s", "-z").stdout
    files = []
    for entry in filter(None, out.split("\0")):
        meta, path = entry.split("\t", 1)
        if not meta.startswith("160000") and (root / path).is_file():
            files.append(path)
    return files


def check_working_copy(patterns, root, worklog):
    files = tracked_files(root)
    exempt = load_exempt(root)
    accepted = load_accepted(root)
    return (
        [f"{f}: path: {p}" for f, p in search_paths(patterns, files, exempt)]
        + [f"{f}:{n}: {p}" for f, n, p in search(patterns, files, root, exempt, accepted=accepted)]
        + (check_worklog(files, root, exempt) if worklog else [])
    )


def check_tree(patterns, sha, worklog):
    """The tree of a commit, as `git archive` gives it (submodules are not in it)."""
    with tempfile.TemporaryDirectory() as tmp:
        archive = subprocess.run(["git", "archive", "--format=tar", sha], capture_output=True)
        if archive.returncode != 0:
            raise Unusable(f"git archive {sha}: {archive.stderr.decode(errors='replace').strip()}")
        tar_path = Path(tmp) / "tree.tar"
        tar_path.write_bytes(archive.stdout)
        out_dir = Path(tmp) / "tree"
        out_dir.mkdir()
        with tarfile.open(tar_path) as tar:
            members = [m for m in tar.getmembers() if m.isfile()]
            tar.extractall(out_dir, members=members, filter="data")
        names = [m.name for m in members]
        exempt = load_exempt(out_dir)
        accepted = load_accepted(out_dir)
        return (
            [f"{f}: path: {p}" for f, p in search_paths(patterns, names, exempt)]
            + [f"{f}:{n}: {p}" for f, n, p in search(patterns, names, out_dir, exempt, accepted=accepted)]
            + (check_worklog(names, out_dir, exempt) if worklog else [])
        )


def check_messages(patterns, rev_args, worklog):
    shas = git("rev-list", *rev_args).stdout.split()
    if not shas:
        return []
    with tempfile.TemporaryDirectory() as tmp:
        references = []
        for sha in shas:
            message = git("log", "-1", "--format=%B", sha).stdout
            (Path(tmp) / sha).write_text(message)
            if worklog:
                references += [
                    f"commit {sha[:12]}: line {n}: worklog reference: {t} ({k})"
                    for n, k, t in find_references(message.encode(), MESSAGE_REFERENCES)
                ]
        return [f"commit {f[:12]}: line {n}: {p}" for f, n, p in search(patterns, shas, tmp)] + references


def archive_commit():
    local = git("rev-parse", "-q", "--verify", f"refs/tags/{ARCHIVE_TAG}^{{commit}}", check=False)
    if local.returncode == 0:
        return local.stdout.strip()
    remote = os.environ.get("PUBLIC_ARCHIVE_REMOTE", "private")
    if remote not in git("remote").stdout.split():
        return None
    out = git("ls-remote", "--tags", remote, f"refs/tags/{ARCHIVE_TAG}", f"refs/tags/{ARCHIVE_TAG}^{{}}", check=False)
    if out.returncode != 0:
        raise Unusable(
            f"public-check: cannot ask the remote `{remote}` for the archive {ARCHIVE_TAG}, so whether the pushed "
            f"history contains it is unchecked: {out.stderr.strip()}"
        )
    refs = dict(reversed(line.split("\t")) for line in out.stdout.splitlines() if "\t" in line)
    return refs.get(f"refs/tags/{ARCHIVE_TAG}^{{}}") or refs.get(f"refs/tags/{ARCHIVE_TAG}")


def check_archive(archive, sha, name):
    """A ref that contains the archive brings the old history with it. An archive commit we do not have locally
    cannot be in a local commit's history."""
    if archive is None or git("cat-file", "-e", f"{archive}^{{commit}}", check=False).returncode != 0:
        return []
    if git("merge-base", "--is-ancestor", archive, sha, check=False).returncode == 0:
        return [f"refused: {name} contains the archive {ARCHIVE_TAG} ({archive[:12]}); push only the fresh history"]
    return []


SHA = re.compile(r"\A(?:[0-9a-f]{40}|[0-9a-f]{64})\Z")


def pre_push_refs(text):
    """git's pre-push lines, each `<local ref> <local sha> <remote ref> <remote sha>`. One of another shape means the
    pushed refs are unknown, and an unchecked push must not pass as a clean one. None (an up-to-date push) is []."""
    lines = [line for line in text.splitlines() if line.strip()]
    refs = []
    for line in lines:
        parts = line.split()
        if len(parts) != 4 or not SHA.match(parts[1]) or not SHA.match(parts[3]):
            raise Unusable(
                f"public-check: --pre-push cannot read the ref line `{line.strip()}` "
                "(`<local ref> <sha> <remote ref> <sha>`), so that push was not checked"
            )
        refs.append(parts)
    return refs


def accept_entry(path, number):
    """`--accept <path> <line number>`: the ACCEPTED_LIST entry for that line, its reason left to fill in."""
    # lines as ripgrep numbers them: split at "\n" only (str.splitlines also splits at \x0b, \x1c and others)
    lines = Path(path).read_bytes().decode(errors="replace").split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    if not number.isdigit() or not 1 <= int(number) <= len(lines):
        raise Unusable(f"public-check: {path} has no line {number}")
    print(f"{path}  {line_hash(lines[int(number) - 1])}  # <why this line is verbatim or recorded data>")
    return 0


def main(argv):
    if argv[:1] == ["--accept"] and len(argv) == 3:
        return accept_entry(argv[1], argv[2])
    if len(argv) not in (0, 2) or (argv and argv[0] != "--pre-push"):
        print(__doc__, file=sys.stderr)
        return 2
    for tool in ("git", "rg"):
        if shutil.which(tool) is None:
            raise Unusable(f"public-check: needs `{tool}` on PATH")
    patterns = [(name, p) for name, p in BUILT_IN] + denylist_patterns()
    worklog = worklog_rule()
    root = Path(git("rev-parse", "--show-toplevel").stdout.strip())
    os.chdir(root)
    refs = pre_push_refs(sys.stdin.read()) if argv else None
    if refs == []:
        # git's up-to-date push: nothing is pushed, so nothing can leak.
        return 0
    archive = archive_commit()
    findings = []
    if not argv:
        findings += check_working_copy(patterns, root, worklog)
        if git("rev-parse", "-q", "--verify", "HEAD", check=False).returncode == 0:
            findings += check_messages(patterns, ["HEAD", "--not", "--remotes"], worklog)
            findings += check_archive(archive, "HEAD", "HEAD")
    else:
        remote = argv[1]
        for local_ref, local_sha, _remote_ref, remote_sha in refs:
            if local_sha == ZERO:
                continue
            findings += check_tree(patterns, local_sha, worklog)
            if remote_sha != ZERO and git("cat-file", "-e", f"{remote_sha}^{{commit}}", check=False).returncode == 0:
                findings += check_messages(patterns, [local_sha, f"^{remote_sha}"], worklog)
            else:
                findings += check_messages(patterns, [local_sha, "--not", f"--remotes={remote}"], worklog)
            findings += check_archive(archive, local_sha, local_ref)
    for finding in findings:
        print(finding)
    if findings:
        print(
            f"public-check: {len(findings)} findings: a private name, a local path, an internal host or the worklog",
            file=sys.stderr,
        )
        return 1
    rule = "on" if worklog else "off; PUBLIC_WORKLOG=on runs it"
    print(f"public-check: clean ({len(patterns)} patterns; the worklog rule {rule})", file=sys.stderr)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except Unusable as e:
        print(e, file=sys.stderr)
        sys.exit(2)
