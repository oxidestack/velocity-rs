PY ?= python3
HOOKS_DIR ?= $(shell git rev-parse --git-path hooks)

.PHONY: test lint doc msrv deny public-check commits-check release-notes release-check publish-dry-run tools-test hooks site site-versions

test: ## unit and integration tests
	cargo test

lint: ## clippy, warnings are errors
	cargo clippy --all-targets -- -D warnings

doc: ## the API documentation, rustdoc's warnings are errors
	RUSTDOCFLAGS='-D warnings' cargo doc --no-deps

# The Rust Cargo.toml declares as the oldest it builds on; `rustup toolchain install <it> --profile minimal` first.
# RUSTC names that toolchain's compiler, so a rustc found first on PATH (a system one) cannot stand in for it.
MSRV ?= $(shell sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml)

msrv: ## the library builds on the MSRV (MSRV=<version>, default Cargo.toml's rust-version)
	RUSTC="$$(rustup which --toolchain $(MSRV) rustc)" rustup run $(MSRV) cargo check --lib

deny: ## licences, advisories and sources against deny.toml (cargo install cargo-deny --locked)
	cargo deny check

# The publication gate: no private name, local path, internal host or worklog reference in a tracked file or an
# unpushed commit message. PUBLIC_DENYLIST=<file>|none (none runs the built-in half alone).
public-check: ## fail on a private name, a local path, an internal host or a worklog reference
	$(if $(PUBLIC_DENYLIST),PUBLIC_DENYLIST='$(PUBLIC_DENYLIST)') $(PY) tools/public/public_check.py $(if $(PUSH),--pre-push $(PUSH))

# The commit-message gate (tools/lint/commits.py, CONTRIBUTING.md "Commit messages"): its fixtures, then every commit
# after the base. The pre-push hook calls it with PUSH=<remote> and git's ref lines on stdin.
commits-check: ## the commit messages after the base; PUSH=<remote> with git's pre-push ref lines on stdin: the pushed ones
	$(PY) tools/lint/commits.py $(if $(PUSH),--pre-push $(PUSH))

release-notes: ## draft the next release's section at the top of CHANGELOG.md (git-cliff in Docker); REPO=<dir> drafts another repository's
	sh tools/release/release-notes.sh $(REPO)

# The release (.github/workflows/release.yml, tools/release/release.py). The gate a tag passes before anything is
# published: the tag's version in Cargo.toml, CHANGELOG.md's section and the upgrade guide's for a release, and every
# sibling repository's crate this one depends on already on crates.io. Without TAG, the gate of the manual dry run.
release-check: ## the release gate for TAG=v<version> (a pre-release: v<version>-rc.N); without TAG, the dry run's
	$(PY) tools/release/release.py check $(if $(TAG),$(TAG),--dry-run)

publish-dry-run: ## cargo publish --dry-run of what a tag publishes; nothing is uploaded
	$(PY) tools/release/release.py publish --dry-run

tools-test: ## the tests of tools/
	$(PY) -m unittest discover -s tools/public
	$(PY) -m unittest discover -s tools/lint
	$(PY) -m unittest discover -s tools/release

hooks: ## install the pre-push hook: make public-check and make commits-check before every push
	@mkdir -p '$(HOOKS_DIR)'
	@if [ -e '$(HOOKS_DIR)/pre-push' ] && ! cmp -s tools/public/pre-push '$(HOOKS_DIR)/pre-push'; then \
		echo "hooks: $(HOOKS_DIR)/pre-push exists and differs from tools/public/pre-push; move it away first" >&2; exit 1; fi
	install -m 755 tools/public/pre-push '$(HOOKS_DIR)/pre-push'

# The documentation site (site/README.md): Astro Starlight over README.md, docs/guide/ and their translations, built
# in the node image pinned here by digest (node 22), so no host needs node; npm installs into site/node_modules/ alone.
SITE_NODE_IMAGE ?= node:22-alpine@sha256:1314fb30ec37592d5845e5a160d522c0e680972aa534549940b8d73bde88481a

site: ## the documentation site into site/dist/; a broken internal link or anchor fails it
	docker run --rm --name velocity-rs-site-$$$$ --user "$$(id -u):$$(id -g)" -e HOME=/tmp -v "$(CURDIR):/src" -w /src \
		$(SITE_NODE_IMAGE) sh site/build.sh

# extract.sh needs git, so it runs on the host; build.sh in the node image. With no release tag it builds the working
# tree at the root alone (site/README.md, "Versions").
site-versions: ## every release's documentation into target/site/pages/ as GitHub Pages serves it
	sh site/versions/extract.sh
	docker run --rm --name velocity-rs-site-versions-$$$$ --user "$$(id -u):$$(id -g)" -e HOME=/tmp -v "$(CURDIR):/src" -w /src \
		$(SITE_NODE_IMAGE) sh site/versions/build.sh
