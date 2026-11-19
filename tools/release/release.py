#!/usr/bin/env python3
"""The release's steps that need more than a line of shell (.github/workflows/release.yml): the gate a version tag
passes before anything is published, the GitHub release's notes, and `cargo publish` itself.

    python3 tools/release/release.py check <tag>
        The gate of a tag push. <tag> is vX.Y.Z, or vX.Y.Z-<pre-release> (vX.Y.Z-rc.N) for a pre-release, which
        crates.io records as a pre-release version: `cargo add` and a `"X.Y"` requirement never pick it. Refused
        (exit 1, every reason printed) when:
        - the tag is not such a version;
        - a package the release publishes (the root's, and every workspace member's) has another version in its
          Cargo.toml: a pre-release is a commit that sets X.Y.Z-rc.N there, then its tag;
        - for a release (no `-`): CHANGELOG.md has no `## [X.Y.Z]` section (`make release-notes` drafts it), or that
          section has `### Action required` and docs/guide/upgrading.md has no `## Upgrading to X.Y.Z` section.
          A pre-release needs neither: its notes live on its GitHub release;
        - a dependency on a sibling repository's crate (a `path` outside this repository, with a `version`) has no
          version on crates.io that the requirement accepts: `cargo publish` keeps the version and drops the path, so
          the sibling has to be released first (the order: the sibling's release, then this one).
        On success prints `version=<version>`, `prerelease=<true|false>` and `crates=<the packages, space-separated>`,
        the lines GITHUB_OUTPUT takes.
    python3 tools/release/release.py check --dry-run [<version>]
        The gate of a manual run, which publishes nothing: the version is <version>, by default Cargo.toml's with
        `-rc.0` added (a pre-release, which `stamp` writes into the tree), the CHANGELOG gate runs on the Unreleased
        section (when it has Action required, the upgrade guide needs some `## Upgrading to` section), and the
        sibling check as above. Prints what `check <tag>` prints.
    python3 tools/release/release.py stamp <version>
        Writes <version> into every published package's Cargo.toml, into the requirements on each other inside this
        repository and into Cargo.lock, so the dry run packages the pre-release a tag would. The tree is left
        changed.
    python3 tools/release/release.py notes <version>
        The GitHub release's body on stdout: CHANGELOG.md's `## [<version>]` section without its heading. A
        pre-release without a section of its own says what it is, and carries the section of the release it leads
        to when CHANGELOG.md has one already. Then how to depend on it.
    python3 tools/release/release.py publish [--dry-run]
        `cargo publish` of what the tag releases (`--workspace` for a workspace, `--locked` when Cargo.lock is
        tracked). Without --dry-run it runs only in a GitHub Actions job of a tag push holding the token of
        crates.io's trusted publishing (CARGO_REGISTRY_TOKEN), and refuses anywhere else.

Environment: CHANGELOG and UPGRADING name other files (the tests); RELEASE_INDEX replaces crates.io's sparse index
(https://index.crates.io/) by another URL or a local directory laid out the same way (the tests). Exit 0: done; 1: the
gate refused, or a step failed; 2: usage.
"""

import glob
import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
import urllib.error
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
INDEX = "https://index.crates.io"
TAG = re.compile(r"^v(?P<version>(?P<base>\d+\.\d+\.\d+)(?:-(?P<pre>[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?)$")
DEPENDENCY_TABLES = ("dependencies", "build-dependencies")


class Refused(Exception):
    """The gate's reasons, every one of them."""

    def __init__(self, reasons):
        super().__init__("\n".join(reasons))
        self.reasons = reasons


def say(message):
    if os.environ.get("GITHUB_ACTIONS") == "true":
        print(f"::error title=release::{message}", file=sys.stderr)
    else:
        print(f"release: {message}", file=sys.stderr)


# --- the manifests ---------------------------------------------------------------------------------------------------


def load(path):
    with open(path, "rb") as f:
        return tomllib.load(f)


def manifests(root):
    """The Cargo.toml of every package a release publishes: the root's [package], then each workspace member's, in
    the order the workspace names them; a package with `publish = false` is left out."""
    top = load(os.path.join(root, "Cargo.toml"))
    paths = []
    if "package" in top:
        paths.append(os.path.join(root, "Cargo.toml"))
    for member in top.get("workspace", {}).get("members", []):
        for d in sorted(glob.glob(os.path.join(root, member))):
            p = os.path.join(d, "Cargo.toml")
            if os.path.isfile(p) and os.path.abspath(p) != os.path.join(os.path.abspath(root), "Cargo.toml"):
                paths.append(p)
    out = []
    for p in paths:
        package = load(p).get("package", {})
        if package.get("publish", True) is False or package.get("publish") == []:
            continue
        out.append(p)
    return out


def package(manifest):
    return load(manifest)["package"]


def is_workspace(root):
    return bool(load(os.path.join(root, "Cargo.toml")).get("workspace", {}).get("members"))


def inside(root, path):
    root = os.path.realpath(root)
    path = os.path.realpath(path)
    return path == root or path.startswith(root + os.sep)


def dependencies(manifest):
    """(table, key, spec) of every normal and build dependency, the target-specific ones too."""
    doc = load(manifest)
    tables = [doc]
    tables += list(doc.get("target", {}).values())
    for t in tables:
        for name in DEPENDENCY_TABLES:
            for key, spec in t.get(name, {}).items():
                yield name, key, spec


def siblings(root):
    """(crate, requirement, manifest) of each dependency on a crate of another repository: a `path` outside root."""
    out = []
    for m in manifests(root):
        for _, key, spec in dependencies(m):
            if not isinstance(spec, dict) or "path" not in spec:
                continue
            if inside(root, os.path.join(os.path.dirname(m), spec["path"])):
                continue
            out.append((spec.get("package", key), spec.get("version"), m))
    return out


# --- versions and requirements ---------------------------------------------------------------------------------------


def parse_version(text):
    m = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?", text.strip())
    if not m:
        raise ValueError(f"not a version: {text}")
    pre = tuple(int(p) if p.isdigit() else p for p in m.group(4).split(".")) if m.group(4) else ()
    return int(m.group(1)), int(m.group(2)), int(m.group(3)), pre


def version_key(v):
    major, minor, patch, pre = v
    # SemVer precedence: a pre-release sorts before its release; numeric identifiers before alphanumeric ones.
    return (major, minor, patch, 0 if pre else 1, tuple((0, p, "") if isinstance(p, int) else (1, 0, p) for p in pre))


def comparator(text):
    """One comparator of a Cargo requirement as (op, major, minor|None, patch|None, pre)."""
    m = re.fullmatch(r"\s*(\^|~|=|>=|<=|>|<)?\s*(\d+)(?:\.(\d+|\*|x|X))?(?:\.(\d+|\*|x|X))?(?:-([0-9A-Za-z.-]+))?\s*", text)
    if not m:
        raise ValueError(f"not a requirement: {text}")
    op = m.group(1) or "^"

    def num(g):
        return int(g) if g is not None and g.isdigit() else None

    minor, patch = num(m.group(3)), num(m.group(4))
    pre = tuple(int(p) if p.isdigit() else p for p in m.group(5).split(".")) if m.group(5) else ()
    return op, int(m.group(2)), minor, patch, pre


def satisfies(version, requirement):
    """Cargo's rule: every comparator holds, and a pre-release version matches only a comparator naming a pre-release
    of the same X.Y.Z."""
    v = parse_version(version)
    comps = [comparator(c) for c in requirement.split(",")] if requirement.strip() not in ("", "*") else []
    if v[3] and not any(c[4] and (c[1], c[2], c[3]) == v[:3] for c in comps):
        return False
    key = version_key(v)
    for op, major, minor, patch, pre in comps:
        low = version_key((major, minor or 0, patch or 0, pre))
        if op == "=":
            if minor is None:
                ok = v[0] == major
            elif patch is None:
                ok = v[:2] == (major, minor)
            else:
                ok = key == low
        elif op == ">":
            ok = key > low
        elif op == ">=":
            ok = key >= low
        elif op == "<":
            ok = key < low
        elif op == "<=":
            ok = key <= low
        elif op == "~":
            high = (major + 1, 0, 0) if minor is None else (major, minor + 1, 0)
            ok = key >= low and key < version_key(high + ((0,),))
        else:  # caret
            if major > 0 or minor is None:
                high = (major + 1, 0, 0)
            elif minor > 0 or patch is None:
                high = (0, minor + 1, 0)
            else:
                high = (0, 0, patch + 1)
            ok = key >= low and key < version_key(high + ((0,),))
        if not ok:
            return False
    return True


# --- crates.io's sparse index ----------------------------------------------------------------------------------------


def index_path(name):
    n = name.lower()
    if len(n) <= 2:
        return f"{len(n)}/{n}"
    if len(n) == 3:
        return f"3/{n[0]}/{n}"
    return f"{n[:2]}/{n[2:4]}/{n}"


def fetch(url):
    """The body at <url>; None when the index has no such file. curl when there is one, since it trusts the system's
    certificates where a Python build may carry none; urllib otherwise. A failure to reach the index refuses."""
    agent = "release-gate (cargo crate release workflow)"
    if shutil.which("curl"):
        done = subprocess.run(["curl", "-sS", "-L", "--retry", "3", "-A", agent, "-w", "\n%{http_code}", url],
                              capture_output=True, text=True)
        body, _, code = done.stdout.rpartition("\n")
        if done.returncode == 0 and code == "200":
            return body
        if done.returncode == 0 and code in ("403", "404"):
            return None
        raise Refused([f"crates.io's index did not answer for {url}: {done.stderr.strip() or 'HTTP ' + code}"])
    try:
        with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": agent}), timeout=30) as r:
            return r.read().decode("utf-8")
    except urllib.error.HTTPError as e:
        if e.code in (403, 404):
            return None
        raise Refused([f"crates.io's index did not answer for {url}: HTTP {e.code}"])
    except urllib.error.URLError as e:
        raise Refused([f"crates.io's index did not answer for {url}: {e.reason}"])


def published(name):
    """The versions of <name> on crates.io that are not yanked; [] when it has none (or is not there)."""
    base = os.environ.get("RELEASE_INDEX", INDEX)
    rel = index_path(name)
    if os.path.isdir(base):
        p = os.path.join(base, rel)
        if not os.path.isfile(p):
            return []
        with open(p, encoding="utf-8") as f:
            text = f.read()
    else:
        text = fetch(f"{base.rstrip('/')}/{rel}")
        if text is None:
            return []
    out = []
    for line in text.splitlines():
        if line.strip():
            entry = json.loads(line)
            if not entry.get("yanked"):
                out.append(entry["vers"])
    return out


def check_siblings(root):
    reasons = []
    for crate, requirement, manifest in siblings(root):
        where = os.path.relpath(manifest, root)
        if not requirement:
            reasons.append(f"{where}: the dependency on {crate} has a path and no version, which crates.io cannot take")
            continue
        versions = published(crate)
        if not any(satisfies(v, requirement) for v in versions):
            have = ", ".join(versions[-5:]) if versions else "none"
            reasons.append(
                f"{where} depends on {crate} {requirement}, and crates.io has no such version (it has: {have}): "
                f"release {crate} first, then this crate"
            )
    return reasons


# --- CHANGELOG.md and the upgrade guide ------------------------------------------------------------------------------


def changelog(root):
    return os.environ.get("CHANGELOG", os.path.join(root, "CHANGELOG.md"))


def upgrading(root):
    return os.environ.get("UPGRADING", os.path.join(root, "docs", "guide", "upgrading.md"))


def read(path):
    try:
        with open(path, encoding="utf-8") as f:
            return f.read()
    except FileNotFoundError:
        return ""


def section(text, heading):
    """The lines under the `## ` heading that starts with <heading> (`[0.2.0]`, `Unreleased`), up to the next `## `;
    None when there is no such heading. `[0.2.0]` never matches `[0.2.0-rc.1]`: the bracket closes it."""
    lines, on, found = [], False, False
    want = f"## {heading}"
    for line in text.splitlines():
        if line.startswith("## "):
            on = line == want or (line.startswith(want) and (heading.endswith("]") or line[len(want)] == " "))
            found = found or on
            continue
        if on:
            lines.append(line)
    return "\n".join(lines).strip("\n") if found else None


def asks_for_action(body):
    return body is not None and re.search(r"^### Action required\s*$", body, flags=re.M) is not None


def has_upgrade_section(root, version=None):
    text = read(upgrading(root))
    if version is None:
        return re.search(r"^## Upgrading to ", text, flags=re.M) is not None
    return re.search(rf"^## Upgrading to {re.escape(version)}(\s.*)?$", text, flags=re.M) is not None


def gate_changelog(root, version):
    """The upgrade-guide gate: a release needs its CHANGELOG.md section, and an upgrade-guide section when that asks
    for action. None for a pre-release; the Unreleased section for a dry run (version None)."""
    text = read(changelog(root))
    if version is None:
        if asks_for_action(section(text, "Unreleased")) and not has_upgrade_section(root):
            return ["CHANGELOG.md's Unreleased section has Action required, and docs/guide/upgrading.md has no "
                    "## Upgrading to section"]
        return []
    if "-" in version:
        return []
    body = section(text, f"[{version}]")
    if body is None:
        return [f"CHANGELOG.md has no section ## [{version}] (make release-notes drafts it)"]
    if asks_for_action(body) and not has_upgrade_section(root, version):
        return [f"CHANGELOG.md's {version} has Action required, and docs/guide/upgrading.md has no "
                f"## Upgrading to {version} section"]
    return []


# --- the commands ----------------------------------------------------------------------------------------------------


def check(root, tag):
    m = TAG.match(tag)
    if not m:
        raise Refused([f"{tag} is not a version tag (vX.Y.Z, or vX.Y.Z-rc.N for a pre-release)"])
    version = m.group("version")
    reasons = []
    for p in manifests(root):
        pkg = package(p)
        if pkg.get("version") != version:
            reasons.append(
                f"{os.path.relpath(p, root)}: {pkg['name']} is version {pkg.get('version')}, the tag says {version}"
            )
    reasons += gate_changelog(root, version)
    reasons += check_siblings(root)
    if reasons:
        raise Refused(reasons)
    return version


def dry_run_version(root, version=None):
    if version:
        if not TAG.match(f"v{version}"):
            raise Refused([f"{version} is not a version (X.Y.Z, or X.Y.Z-rc.N)"])
        return version
    own = package(manifests(root)[0])["version"]
    return own if "-" in own else f"{own}-rc.0"


def check_dry_run(root, version=None):
    version = dry_run_version(root, version)
    reasons = gate_changelog(root, None) + check_siblings(root)
    if reasons:
        raise Refused(reasons)
    return version


def outputs(root, version):
    names = " ".join(package(p)["name"] for p in manifests(root))
    return f"version={version}\nprerelease={'true' if '-' in version else 'false'}\ncrates={names}\n"


def stamp(root, version):
    """Rewrites the version of every published package, and of the requirements between them, to <version>."""
    for p in manifests(root):
        with open(p, encoding="utf-8") as f:
            lines = f.read().split("\n")
        table = None
        for i, line in enumerate(lines):
            head = re.match(r"^\s*\[([^\]]+)\]\s*$", line)
            if head:
                table = head.group(1).strip()
                continue
            if table == "package" and re.match(r'^version\s*=\s*"[^"]*"', line):
                lines[i] = re.sub(r'"[^"]*"', f'"{version}"', line, count=1)
                continue
            dep = re.search(r'\bpath\s*=\s*"([^"]+)"', line)
            if dep and inside(root, os.path.join(os.path.dirname(p), dep.group(1))):
                lines[i] = re.sub(r'(\bversion\s*=\s*)"[^"]*"', rf'\g<1>"{version}"', line)
        with open(p, "w", encoding="utf-8") as f:
            f.write("\n".join(lines))
    # Cargo.lock records each package's version too, and `cargo publish --locked` refuses a lockfile it must change.
    lock = os.path.join(root, "Cargo.lock")
    if os.path.isfile(lock):
        names = {package(p)["name"] for p in manifests(root)}
        with open(lock, encoding="utf-8") as f:
            text = f.read()
        for name in names:
            text = re.sub(rf'(\nname = "{re.escape(name)}"\nversion = )"[^"]*"', rf'\g<1>"{version}"', text)
        with open(lock, "w", encoding="utf-8") as f:
            f.write(text)


def repository(root):
    return package(manifests(root)[0]).get("repository", "").rstrip("/")


def notes(root, version):
    text = read(changelog(root))
    body = section(text, f"[{version}]")
    base = version.split("-", 1)[0]
    if body is None and "-" not in version:
        raise Refused([f"CHANGELOG.md has no section ## [{version}]"])
    if body is None:
        body = f"A pre-release of {base}, for testing: `cargo add` and a `\"{base}\"` requirement never pick it."
        ahead = section(text, f"[{base}]")
        if ahead:
            body += f"\n\nWhat {base} brings so far:\n\n{ahead}"
    # A released section may end with its compare link.
    body = re.sub(rf"\n*^\[{re.escape(version)}\]: \S+\s*\Z", "", body.strip("\n"), flags=re.M)
    crates = [package(p)["name"] for p in manifests(root)]
    req = f"={version}" if "-" in version else version
    lines = [body, "", "## Use it", ""]
    for name in crates:
        lines.append(f"- `cargo add {name}@{req}`: [crates.io](https://crates.io/crates/{name}/{version}), "
                     f"[docs.rs](https://docs.rs/{name}/{version})")
    if "-" not in version:
        lines += ["", f"Upgrading from an earlier release: [the upgrade guide]({repository(root)}/blob/v{version}/"
                      f"docs/guide/upgrading.md)."]
    return "\n".join(lines).strip("\n") + "\n"


def publish_command(root, dry_run):
    cmd = ["cargo", "publish"]
    if is_workspace(root):
        cmd.append("--workspace")
    tracked = subprocess.run(["git", "-C", root, "ls-files", "--error-unmatch", "Cargo.lock"], capture_output=True)
    if tracked.returncode == 0:
        cmd.append("--locked")
    if dry_run:
        # The dry run packages a stamped tree, which git sees as changed.
        cmd += ["--dry-run", "--allow-dirty"]
    return cmd


def publish(root, dry_run):
    if not dry_run:
        env = os.environ
        if not (env.get("GITHUB_ACTIONS") == "true" and env.get("GITHUB_REF_TYPE") == "tag"
                and env.get("CARGO_REGISTRY_TOKEN")):
            raise Refused(["publish without --dry-run runs only in the release workflow on a tag push, with the "
                           "trusted-publishing token in CARGO_REGISTRY_TOKEN"])
    cmd = publish_command(root, dry_run)
    print("release: " + " ".join(cmd), file=sys.stderr)
    return subprocess.run(cmd, cwd=root).returncode


def main(argv, root=ROOT):
    try:
        if len(argv) == 2 and argv[0] == "check" and argv[1] != "--dry-run":
            sys.stdout.write(outputs(root, check(root, argv[1])))
        elif argv[:2] == ["check", "--dry-run"] and len(argv) <= 3:
            sys.stdout.write(outputs(root, check_dry_run(root, argv[2] if len(argv) == 3 else None)))
        elif len(argv) == 2 and argv[0] == "stamp":
            dry_run_version(root, argv[1])
            stamp(root, argv[1])
        elif len(argv) == 2 and argv[0] == "notes":
            sys.stdout.write(notes(root, argv[1]))
        elif argv in (["publish"], ["publish", "--dry-run"]):
            return publish(root, len(argv) == 2)
        else:
            print(__doc__.split("\n\n", 1)[1].split("\nEnvironment:")[0], file=sys.stderr)
            return 2
    except Refused as e:
        for reason in e.reasons:
            say(reason)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
