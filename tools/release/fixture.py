"""A throwaway git history for the tests of `make release-notes`: one commit of every kind the draft treats
differently, after a tagged release whose commits it must leave out.

    python3 tools/release/fixture.py <empty directory> [--untagged]

Every author, committer and date is fixed, so the commit hashes, and the draft's entries that name them, are the same
on every machine. `--untagged` leaves out the tag: the history of a first release.
"""

import os
import re
import subprocess
import sys
import tomllib

# The repository's root: cliff.toml's header is the fixture's CHANGELOG.md, the file the draft is prepended to.
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

SIGN_OFF = "Signed-off-by: Jane Doe <jane@example.org>"

# (message, the tag put on that commit or None), oldest first. The bodies follow the commit format: the release-note
# block is the body's last paragraph, the footers one trailer block after it.
COMMITS = [
    ("chore: start the public history", None),
    (
        "feat: accept input from any reader\n\n"
        "```release-note\nInput may come from any `std::io::Read`.\n```",
        "v0.1.0",
    ),
    (
        "feat(parser): add an option to trim trailing whitespace\n\n"
        "```release-note\nThe `trim` option drops trailing whitespace from the output.\n```",
        None,
    ),
    (
        "fix: keep an error's position after a nested call\n\n"
        "An error raised in a nested call named the inner position, so a\n"
        "caller looked in the wrong place.\n\n"
        "```release-note\n"
        "An error raised in a nested call names the position of the call, so\n"
        "a caller finds it.\n"
        "```",
        None,
    ),
    (
        "feat!: drop the strict_mode alias\n\n"
        "```release-note\n"
        "`strict_mode` is no longer read. A configuration that sets it sets\n"
        "`strict` to the same value instead.\n"
        "```\n\n"
        "BREAKING-CHANGE: a configuration that sets strict_mode must set\n"
        " strict instead; the value is unchanged.",
        None,
    ),
    (
        "fix: close the input file before the parser reads it\n\n"
        "No release has carried the bug.\n\n"
        "```release-note\nNONE\n```",
        None,
    ),
    ("build(deps): bump serde from 1.0.219 to 1.0.228", None),
    (
        "perf: parse each input once\n\n"
        "```release-note\nAn input used twice is parsed once, so using it again is faster.\n```",
        None,
    ),
    (
        "docs: describe the options in the README\n\n"
        "```release-note\nThe README lists every option with its default.\n```",
        None,
    ),
    (
        "fix: return an error for input that is not UTF-8\n\n"
        "```release-note\nInput that is not UTF-8 returns an error instead of a panic.\n```",
        None,
    ),
    ("test: cover the empty input", None),
    # An issue reference is a footer after the block (`Closes: #<n>`), and the entry is drafted.
    (
        "fix: name the input's line in an error\n\n"
        "```release-note\nAn error names the line of the input, as the Java library does.\n```\n\n"
        "Closes: #3",
        None,
    ),
    # The same reference as a paragraph before the block: git-cliff's parser starts the footers there (`Token #value`
    # is a footer too), so the block is not in the body and the draft has no entry. The commit gate refuses this one
    # (release-note-place); the fixture holds git-cliff to the behaviour the rule guards against.
    (
        "fix: return an error for an unclosed block\n\n"
        "Closes #4\n\n"
        "```release-note\nAn unclosed block returns an error instead of an empty output.\n```",
        None,
    ),
]


def git(repo, *args, when=None):
    env = {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "HOME": repo,
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_AUTHOR_NAME": "Jane Doe",
        "GIT_AUTHOR_EMAIL": "jane@example.org",
        "GIT_COMMITTER_NAME": "Jane Doe",
        "GIT_COMMITTER_EMAIL": "jane@example.org",
    }
    if when:
        env["GIT_AUTHOR_DATE"] = env["GIT_COMMITTER_DATE"] = when
    subprocess.run(["git", "-C", repo, *args], env=env, check=True, capture_output=True)


def build(repo, tagged=True):
    """Builds the history in `repo`, an empty directory; its CHANGELOG.md is the repository's head alone (cliff.toml's
    header), without the releases the repository's file has drafted since."""
    git(repo, "init", "-q", "-b", "main")
    with open(os.path.join(ROOT, "cliff.toml"), "rb") as src:
        changelog = tomllib.load(src)["changelog"]["header"]
    for n, (message, tag) in enumerate(COMMITS):
        when = f"2026-01-{n + 1:02d}T12:00:00+00:00"
        name = "CHANGELOG.md" if n == 0 else f"change-{n:02d}.txt"
        with open(os.path.join(repo, name), "w", encoding="utf-8") as out:
            out.write(changelog if n == 0 else f"{n}\n")
        git(repo, "add", name)
        # The sign-off joins a footer block the message's body ends with, as `git commit -s` does.
        footers = "\n\n" in message and re.match(r"[A-Za-z-]+: ", message.rsplit("\n\n", 1)[-1])
        sep = "\n" if footers else "\n\n"
        git(repo, "commit", "-q", "--cleanup=verbatim", "-m", f"{message}{sep}{SIGN_OFF}", when=when)
        if tag and tagged:
            git(repo, "tag", tag, when=when)


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3) or (len(sys.argv) == 3 and sys.argv[2] != "--untagged"):
        sys.exit(__doc__)
    build(sys.argv[1], tagged=len(sys.argv) == 2)
