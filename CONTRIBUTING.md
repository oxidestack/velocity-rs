# Contributing to velocity-rs

The measure of a change is the real Apache Velocity 2.4.1 (and Velocity Tools 3.1) configured as MockServer 8.0.0
configures them: where this crate and the Java library disagree on a template's output, the Java library is right.

## Build and test

See the README. `cargo test`, `make lint` and `make public-check` must pass; `make deny` checks the licences against the
CNCF's allowed list, so a new dependency needs a licence on it.

New behaviour comes with a case in `tests/cases/*.json` and its expected output recorded from the real Java library
(`reference/reference.sh`, `generators/`), never worked out by hand.

## Commit messages

Every commit follows [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/) and is signed off
([DCO](https://developercertificate.org/)). The release notes are drafted from the commits' release-note blocks (`make release-notes`), and
`make commits-check` checks the messages; the pre-push hook (`make hooks`) runs it.

````text
<type>(<scope>)!: <description>

<body>

```release-note
<what changes for users and operators, and what they have to do>
```

BREAKING-CHANGE: <what changes and what to do>
Closes: #<issue>
Signed-off-by: Your Name <you@example.org>
````

- **Type**: `feat` (something new a caller can use), `fix` (what ships behaved wrongly), `perf`,
  `refactor`, `test`, `docs`, `build`, `ci`, `chore`, `revert`.
- **Scope**, optional, the part changed: `parser`, `render`, `introspect`, `resource`, `config`, `events`, `math`,
  `upstream`, `deps`, `release`. Leave it out when the change spans several.
- **Description**: imperative, lower-case, no full stop; the whole first line at most 72 characters.
- **Body**: what changed and why, wrapped at 72 columns. A line that cannot be wrapped is exempt: one without a space
  (a URL, a path), one indented by four spaces or more, and a `Signed-off-by:` or `Co-authored-by:` trailer.
- **Release note**: a `release-note` block, the body's last paragraph, written for the crate's users, not for its
  developers. Required on `feat`, `fix`, `perf` and breaking commits, optional on the others. Write
  `NONE` in it when no released version is affected. No line in it starts with `#`: git drops such lines when the
  message goes through the editor.
- **Breaking**: a caller must change to keep compiling or working (a public item removed or renamed, a
  changed signature, a different result for the same input; matching the Java library where we differed is a
  `fix`): `!` before the colon, a `BREAKING-CHANGE:` footer, and a release note saying what to do.
- **Footers**: the message's last paragraph, one block, in this order: `BREAKING-CHANGE:`, `Refs:`, `Closes:`,
  `Co-authored-by:`, `Signed-off-by:`. Never put a paragraph opening with `Word:` or `Word #` before the release
  note: the release notes are drafted with a parser that reads the footers from such a paragraph on.
- **Reverts**: `revert: <the reverted commit's first line>`, the reason, and `Refs: <hash>`.

Pull requests are merged with "Rebase and merge", so each commit lands as written: squash your fixups
(`git commit --fixup`, `git rebase -i --autosquash`) before asking for a merge.

### Signing off

The sign-off is your statement, under the [Developer Certificate of Origin](https://developercertificate.org/), that
you wrote the change or otherwise have the right to submit it under the project's licences. There is no contributor
licence agreement. Sign off with the name and address your commits are authored with:

```sh
git config user.name "Your Name"
git config user.email you@example.org
git commit -s
```

A check on every pull request fails a commit whose last footer is not its author's `Signed-off-by:`. A commit you
forgot to sign off is signed off again with `git commit --amend -s --no-edit`, or, for every commit of a branch,
`git rebase --signoff main`; then push the branch with `--force-with-lease`.

