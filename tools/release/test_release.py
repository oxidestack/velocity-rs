"""release.py, the release's gate, notes and publish command, on fixture repositories built in a temporary directory:
a crate or a workspace, its CHANGELOG.md and upgrade guide, and a local copy of crates.io's sparse index for the
dependencies on sibling repositories. Needs only Python and git; nothing reaches the network."""

import contextlib
import io
import os
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import release  # noqa: E402

HEAD = "# Changelog\n\n## Unreleased\n\nThe changes since the last release.\n\n"
GUIDE = "# Upgrading\n\n## How to upgrade\n\nChange the version.\n\n"


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(textwrap.dedent(text))


def manifest(name, version, deps=""):
    return f'[package]\nname = "{name}"\nversion = "{version}"\nedition = "2024"\n' \
           f'repository = "https://example.org/{name}"\n\n[dependencies]\n{deps}'


class Fixture(unittest.TestCase):
    def setUp(self):
        self.base = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.base)
        self.root = os.path.join(self.base, "repo")
        self.index = os.path.join(self.base, "index")
        os.makedirs(self.index)
        saved = dict(os.environ)
        self.addCleanup(lambda: (os.environ.clear(), os.environ.update(saved)))
        for var in ("CHANGELOG", "UPGRADING", "GITHUB_ACTIONS", "GITHUB_REF_TYPE", "CARGO_REGISTRY_TOKEN"):
            os.environ.pop(var, None)
        os.environ["RELEASE_INDEX"] = self.index
        self.crate("0.2.0")
        self.changelog("")
        self.guide("")

    def crate(self, version, deps=""):
        write(os.path.join(self.root, "Cargo.toml"), manifest("demo-rs", version, deps))

    def changelog(self, sections):
        write(os.path.join(self.root, "CHANGELOG.md"), HEAD + textwrap.dedent(sections))

    def guide(self, sections):
        write(os.path.join(self.root, "docs", "guide", "upgrading.md"), GUIDE + textwrap.dedent(sections))

    def on_crates_io(self, name, *versions, yanked=()):
        lines = [f'{{"name": "{name}", "vers": "{v}", "yanked": {"true" if v in yanked else "false"}}}'
                 for v in versions]
        write(os.path.join(self.index, release.index_path(name)), "\n".join(lines) + "\n")

    def run_main(self, *argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = release.main(list(argv), root=self.root)
        return code, out.getvalue(), err.getvalue()


class Gate(Fixture):
    def test_a_release_with_its_section_passes(self):
        self.changelog("## [0.2.0] - 2026-11-19\n\n### Features\n\n- Something.\n")
        code, out, err = self.run_main("check", "v0.2.0")
        self.assertEqual(code, 0, err)
        self.assertEqual(out, "version=0.2.0\nprerelease=false\ncrates=demo-rs\n")

    def test_action_required_with_its_upgrade_section_passes(self):
        self.changelog("## [0.2.0] - 2026-11-19\n\n### Action required\n\n- Rename x.\n")
        self.guide("## Upgrading to 0.2.0\n\n### Action required\n\nRename x.\n")
        code, _, err = self.run_main("check", "v0.2.0")
        self.assertEqual(code, 0, err)

    def test_the_upgrade_section_may_carry_a_note_after_the_version(self):
        self.changelog("## [0.2.0] - 2026-11-19\n\n### Action required\n\n- Rename x.\n")
        self.guide("## Upgrading to 0.2.0 (2026-11-19)\n\nRename x.\n")
        self.assertEqual(self.run_main("check", "v0.2.0")[0], 0)

    def test_action_required_without_its_upgrade_section_is_refused(self):
        self.changelog("## [0.2.0] - 2026-11-19\n\n### Action required\n\n- Rename x.\n")
        self.guide("## Upgrading to 0.1.0\n\nNothing.\n\n## Upgrading to 0.2.01\n\nA lookalike.\n")
        code, _, err = self.run_main("check", "v0.2.0")
        self.assertEqual(code, 1)
        self.assertIn("no ## Upgrading to 0.2.0 section", err)

    def test_a_release_without_its_changelog_section_is_refused(self):
        self.changelog("## [0.2.0-rc.1] - 2026-11-19\n\n- Not the release.\n")
        code, _, err = self.run_main("check", "v0.2.0")
        self.assertEqual(code, 1)
        self.assertIn("no section ## [0.2.0]", err)

    def test_an_rc_tag_is_a_pre_release_and_needs_no_section(self):
        self.crate("0.2.0-rc.1")
        self.changelog("## [0.2.0] - draft\n\n### Action required\n\n- Rename x.\n")
        code, out, err = self.run_main("check", "v0.2.0-rc.1")
        self.assertEqual(code, 0, err)
        self.assertEqual(out, "version=0.2.0-rc.1\nprerelease=true\ncrates=demo-rs\n")

    def test_a_tag_that_disagrees_with_cargo_toml_is_refused(self):
        self.changelog("## [0.3.0] - 2026-11-19\n\n- Something.\n")
        code, _, err = self.run_main("check", "v0.3.0")
        self.assertEqual(code, 1)
        self.assertIn("demo-rs is version 0.2.0, the tag says 0.3.0", err)

    def test_an_rc_tag_on_the_release_version_is_refused(self):
        # A pre-release is a commit that sets X.Y.Z-rc.N in Cargo.toml, then its tag.
        code, _, err = self.run_main("check", "v0.2.0-rc.1")
        self.assertEqual(code, 1)
        self.assertIn("the tag says 0.2.0-rc.1", err)

    def test_every_reason_is_printed(self):
        self.changelog("")
        code, _, err = self.run_main("check", "v0.3.0")
        self.assertEqual(code, 1)
        self.assertIn("the tag says 0.3.0", err)
        self.assertIn("no section ## [0.3.0]", err)

    def test_what_is_not_a_version_tag_is_refused(self):
        for tag in ("0.2.0", "v0.2", "v0.2.0+build", "release-0.2.0", "v0.2.0-"):
            with self.subTest(tag=tag):
                code, _, err = self.run_main("check", tag)
                self.assertEqual(code, 1)
                self.assertIn("is not a version tag", err)

    def test_every_workspace_member_must_carry_the_version(self):
        write(os.path.join(self.root, "Cargo.toml"), manifest("demo-rs", "0.2.0", 'part = { package = "demo-part", '
              'path = "crates/part", version = "0.2.0" }\n') + '\n[workspace]\nmembers = ["crates/part"]\n')
        write(os.path.join(self.root, "crates", "part", "Cargo.toml"), manifest("demo-part", "0.1.0"))
        self.changelog("## [0.2.0] - 2026-11-19\n\n- Something.\n")
        code, _, err = self.run_main("check", "v0.2.0")
        self.assertEqual(code, 1)
        self.assertIn("crates/part/Cargo.toml: demo-part is version 0.1.0", err)


class DryRun(Fixture):
    def test_the_dry_run_is_a_pre_release_of_cargo_toml_s_version(self):
        code, out, err = self.run_main("check", "--dry-run")
        self.assertEqual(code, 0, err)
        self.assertIn("version=0.2.0-rc.0\nprerelease=true\n", out)

    def test_the_dry_run_takes_a_version(self):
        self.assertIn("version=0.3.0-rc.2\n", self.run_main("check", "--dry-run", "0.3.0-rc.2")[1])
        self.assertEqual(self.run_main("check", "--dry-run", "nope")[0], 1)

    def test_the_dry_run_gates_the_unreleased_section(self):
        self.changelog("")
        write(os.path.join(self.root, "CHANGELOG.md"),
              "# Changelog\n\n## Unreleased\n\n### Action required\n\n- Rename x.\n")
        code, _, err = self.run_main("check", "--dry-run")
        self.assertEqual(code, 1)
        self.assertIn("Unreleased section has Action required", err)
        self.guide("## Upgrading to X.Y.Z\n\nRename x.\n")
        self.assertEqual(self.run_main("check", "--dry-run")[0], 0)

    def test_stamp_writes_the_version_into_the_workspace(self):
        write(os.path.join(self.root, "Cargo.toml"), manifest(
            "demo-rs", "0.2.0",
            'part = { package = "demo-part", path = "crates/part", version = "0.2.0" }\n'
            'sib = { package = "sib-rs", path = "../sib-rs", version = "0.1.0" }\n'
        ) + '\n[workspace]\nmembers = ["crates/part"]\n')
        write(os.path.join(self.root, "crates", "part", "Cargo.toml"), manifest("demo-part", "0.2.0"))
        self.assertEqual(self.run_main("stamp", "0.2.0-rc.0")[0], 0)
        with open(os.path.join(self.root, "Cargo.toml"), encoding="utf-8") as f:
            top = f.read()
        self.assertIn('version = "0.2.0-rc.0"\nedition', top)
        self.assertIn('path = "crates/part", version = "0.2.0-rc.0"', top)
        # A sibling repository's crate keeps its own version.
        self.assertIn('path = "../sib-rs", version = "0.1.0"', top)
        self.assertEqual(release.package(os.path.join(self.root, "crates", "part", "Cargo.toml"))["version"],
                         "0.2.0-rc.0")

    def test_stamp_writes_the_version_into_cargo_lock(self):
        write(os.path.join(self.root, "Cargo.lock"), 'version = 4\n\n[[package]]\nname = "demo-rs"\nversion = "0.2.0"\n'
              '\n[[package]]\nname = "demo-rs-other"\nversion = "0.2.0"\n')
        self.assertEqual(self.run_main("stamp", "0.2.0-rc.0")[0], 0)
        with open(os.path.join(self.root, "Cargo.lock"), encoding="utf-8") as f:
            lock = f.read()
        self.assertIn('name = "demo-rs"\nversion = "0.2.0-rc.0"', lock)
        self.assertIn('name = "demo-rs-other"\nversion = "0.2.0"', lock)

    def test_stamp_takes_only_a_version(self):
        self.assertEqual(self.run_main("stamp", "latest")[0], 1)


class Siblings(Fixture):
    def setUp(self):
        super().setUp()
        self.crate("0.2.0", 'sib = { package = "sib-rs", path = "../sib-rs", version = "0.1.0" }\n'
                            'inner = { path = "inner", version = "0.2.0" }\nserde = "1"\n')
        self.changelog("## [0.2.0] - 2026-11-19\n\n- Something.\n")

    def test_a_sibling_not_on_crates_io_fails_early(self):
        code, _, err = self.run_main("check", "v0.2.0")
        self.assertEqual(code, 1)
        self.assertIn("depends on sib-rs 0.1.0, and crates.io has no such version (it has: none): release sib-rs "
                      "first", err)
        # The dry run fails the same way: nothing it could package resolves either.
        self.assertEqual(self.run_main("check", "--dry-run")[0], 1)

    def test_a_sibling_on_crates_io_passes(self):
        self.on_crates_io("sib-rs", "0.1.0", "0.1.3")
        self.assertEqual(self.run_main("check", "v0.2.0")[0], 0)

    def test_only_a_version_the_requirement_takes_counts(self):
        self.on_crates_io("sib-rs", "0.0.9", "0.1.0-rc.1", "0.2.0", "0.1.1", yanked=("0.1.1",))
        code, _, err = self.run_main("check", "v0.2.0")
        self.assertEqual(code, 1)
        self.assertIn("it has: 0.0.9, 0.1.0-rc.1, 0.2.0", err)

    def test_an_index_out_of_reach_refuses_with_its_reason(self):
        os.environ["RELEASE_INDEX"] = "http://127.0.0.1:9"
        code, _, err = self.run_main("check", "v0.2.0")
        self.assertEqual(code, 1)
        self.assertIn("crates.io's index did not answer for http://127.0.0.1:9/si/b-/sib-rs", err)

    def test_a_path_without_a_version_is_refused(self):
        self.crate("0.2.0", 'sib = { package = "sib-rs", path = "../sib-rs" }\n')
        code, _, err = self.run_main("check", "v0.2.0")
        self.assertEqual(code, 1)
        self.assertIn("has a path and no version", err)


class Requirements(unittest.TestCase):
    def test_cargo_s_rules(self):
        cases = [
            ("0.1.0", "0.1.0", True), ("0.1.5", "0.1.0", True), ("0.2.0", "0.1.0", False),
            ("1.4.0", "1.2", True), ("2.0.0", "1", False), ("0.0.4", "0.0.3", False),
            ("0.1.0-rc.1", "0.1.0", False), ("0.1.0-rc.1", "0.1.0-rc.1", True), ("0.1.0-rc.2", "0.1.0-rc.1", True),
            ("0.1.0", "0.1.0-rc.1", True), ("0.1.0-rc.1", "=0.1.0-rc.1", True), ("0.1.1", "=0.1.0", False),
            ("0.1.9", "~0.1.2", True), ("0.2.0", "~0.1.2", False), ("1.5.0", ">=1.2, <2", True),
            ("2.0.0", ">=1.2, <2", False), ("3.1.4", "*", True),
        ]
        for version, requirement, want in cases:
            with self.subTest(version=version, requirement=requirement):
                self.assertEqual(release.satisfies(version, requirement), want)

    def test_the_index_paths(self):
        self.assertEqual(release.index_path("a"), "1/a")
        self.assertEqual(release.index_path("ab"), "2/ab")
        self.assertEqual(release.index_path("abc"), "3/a/abc")
        self.assertEqual(release.index_path("Java-Compat-rs"), "ja/va/java-compat-rs")


class Notes(Fixture):
    def test_a_release_s_notes_are_its_section(self):
        self.changelog("## [0.2.0] - 2026-11-19\n\n### Features\n\n- Something.\n\n"
                       "[0.2.0]: https://example.org/compare/v0.1.0...v0.2.0\n\n## [0.1.0] - 2026-01-01\n\n- Old.\n")
        code, out, err = self.run_main("notes", "0.2.0")
        self.assertEqual(code, 0, err)
        self.assertTrue(out.startswith("### Features\n\n- Something.\n\n## Use it\n"), out)
        self.assertNotIn("compare", out)
        self.assertNotIn("Old.", out)
        self.assertIn("`cargo add demo-rs@0.2.0`", out)
        self.assertIn("https://example.org/demo-rs/blob/v0.2.0/docs/guide/upgrading.md", out)

    def test_a_release_without_a_section_has_no_notes(self):
        self.assertEqual(self.run_main("notes", "0.2.0")[0], 1)

    def test_a_pre_release_says_what_it_is(self):
        self.changelog("## [0.2.0] - draft\n\n### Features\n\n- Something.\n")
        code, out, _ = self.run_main("notes", "0.2.0-rc.1")
        self.assertEqual(code, 0)
        self.assertTrue(out.startswith("A pre-release of 0.2.0, for testing"), out)
        self.assertIn("What 0.2.0 brings so far:\n\n### Features\n\n- Something.", out)
        self.assertIn("`cargo add demo-rs@=0.2.0-rc.1`", out)
        self.assertNotIn("upgrade guide", out)

    def test_a_pre_release_with_no_draft_yet(self):
        code, out, _ = self.run_main("notes", "0.2.0-rc.1")
        self.assertEqual(code, 0)
        self.assertNotIn("brings so far", out)


class Publish(Fixture):
    def git(self, *args):
        subprocess.run(["git", "-C", self.root, *args], check=True, capture_output=True)

    def test_the_command(self):
        self.git("init", "-q")
        self.assertEqual(release.publish_command(self.root, True), ["cargo", "publish", "--dry-run", "--allow-dirty"])
        write(os.path.join(self.root, "Cargo.lock"), "version = 4\n")
        self.git("add", "Cargo.lock")
        self.assertEqual(release.publish_command(self.root, False), ["cargo", "publish", "--locked"])
        with open(os.path.join(self.root, "Cargo.toml"), "a", encoding="utf-8") as f:
            f.write('\n[workspace]\nmembers = ["crates/part"]\n')
        write(os.path.join(self.root, "crates", "part", "Cargo.toml"), manifest("demo-part", "0.2.0"))
        self.assertEqual(release.publish_command(self.root, False), ["cargo", "publish", "--workspace", "--locked"])

    def test_a_real_publish_refuses_outside_the_release_workflow(self):
        code, _, err = self.run_main("publish")
        self.assertEqual(code, 1)
        self.assertIn("runs only in the release workflow", err)
        os.environ.update(GITHUB_ACTIONS="true", GITHUB_REF_TYPE="branch", CARGO_REGISTRY_TOKEN="x")
        self.assertEqual(self.run_main("publish")[0], 1)

    def test_usage(self):
        self.assertEqual(self.run_main("publish", "--yes")[0], 2)
        self.assertEqual(self.run_main()[0], 2)


class Workflow(unittest.TestCase):
    """The release workflow's promises that a reader of the YAML alone would have to trust."""

    def setUp(self):
        with open(os.path.join(release.ROOT, ".github", "workflows", "release.yml"), encoding="utf-8") as f:
            self.text = f.read()

    def test_every_action_is_pinned_by_commit(self):
        import re
        for use in re.findall(r"uses:\s*(\S+)", self.text):
            if use.startswith("./"):
                continue
            with self.subTest(use=use):
                self.assertRegex(use, r"^[\w.-]+/[\w./-]+@[0-9a-f]{40}$")

    def test_nothing_is_granted_at_the_top(self):
        self.assertIn("\npermissions: {}\n", self.text)

    def test_only_the_publish_job_gets_an_oidc_token(self):
        self.assertEqual(self.text.count("id-token: write"), 1)
        job = self.text.split("\n  publish:\n", 1)[1].split("\n  github-release:\n", 1)[0]
        self.assertIn("id-token: write", job)
        self.assertIn("environment: release", job)
        self.assertIn("rust-lang/crates-io-auth-action@", job)

    def test_publish_runs_without_dry_run_only_in_the_publish_job(self):
        self.assertEqual(self.text.count("tools/release/release.py publish\n"), 1)


if __name__ == "__main__":
    unittest.main()
