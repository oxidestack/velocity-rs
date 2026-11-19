#!/bin/sh
# Drafts the next release's section of CHANGELOG.md from the commits since the last tag (`make release-notes`): the
# git-cliff pinned below, in its container, with cliff.toml. The version printed is the one the commits call for (fix
# a patch, feat a minor, breaking a major; 0.1.0 before the first tag), and the release's. The section goes under CHANGELOG.md's head and is edited by hand before the tag.
#
#   sh tools/release/release-notes.sh [<repository>]     default: this one
#
# The repository is mounted into the container, so on macOS it lies where the Docker VM sees it (the home directory,
# by default); from inside a container, see RELEASE_NOTES_VOLUMES_FROM below. Exit 0: drafted, the version printed; 1: git-cliff failed; 2: no Docker, no such repository, or no
# CHANGELOG.md in it.
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
REPO=${1:-$ROOT}
[ -d "$REPO" ] || {
	echo "release-notes: no directory $REPO" >&2
	exit 2
}
REPO=$(cd "$REPO" && pwd)
[ -f "$REPO/CHANGELOG.md" ] || {
	echo "release-notes: no CHANGELOG.md in $REPO" >&2
	exit 2
}
command -v docker > /dev/null || {
	echo "release-notes: needs Docker" >&2
	exit 2
}
# The image, pinned by digest (a multi-architecture index: the host's own architecture runs).
IMAGE=orhunp/git-cliff:2.10.1@sha256:6ba0d1fcb051bd7b154cfb19c4b2b3bfa2c22c475f5285fc30606777b6573119

# The image's entrypoint runs git-cliff as the owner of the working directory, so the files it writes are the caller's.
#
# RELEASE_NOTES_VOLUMES_FROM=<container>: the caller is itself a container talking to the host's Docker socket (a CI
# job). A bind mount's path is resolved on the Docker host, where the job's paths do not exist, so git-cliff gets that
# container's volumes instead, at the same paths, and reads the repository and cliff.toml where the caller sees them.
run() {
	if [ -n "${RELEASE_NOTES_VOLUMES_FROM:-}" ]; then
		docker run --rm --name "velocity-rs-release-notes-$$" \
			--volumes-from "$RELEASE_NOTES_VOLUMES_FROM" -w "$REPO" \
			"$IMAGE" --config "$ROOT/cliff.toml" "$@"
	else
		docker run --rm --name "velocity-rs-release-notes-$$" \
			-v "$ROOT/cliff.toml:/cliff.toml:ro" -v "$REPO:/repo" -w /repo \
			"$IMAGE" --config /cliff.toml "$@"
	fi
}

version=$(run --bumped-version) || exit 1
run --unreleased --bump --prepend CHANGELOG.md || exit 1
echo "release-notes: drafted $version at the top of $REPO/CHANGELOG.md"
