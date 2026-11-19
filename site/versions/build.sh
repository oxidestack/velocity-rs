#!/bin/sh
# The second half of `make site-versions` (site/README.md, "Versions"), in the pinned node image: each release that
# extract.sh unpacked is built by its own site/build.sh, with its own lockfile and pages, under
# /velocity-rs/<x.y>/, and the latest once more at the root, /velocity-rs/. Then inject.mjs gives every page the
# version picker and, on an older release, the banner that points at the latest. The site lands in target/site/pages/.
#
# With no release tag yet (extract.sh left the list empty) the working tree is built once, at the root, and no picker
# or versions.json is written: there is one version and nothing to pick.
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
WORK=$ROOT/target/site/versions
OUT=$ROOT/target/site/pages
SITE_ROOT=/velocity-rs

# build <tree> <tag> <base> <out>: the release's site built at <base> into <out>.
build() {
	tree=$1 tag=$2 base=$3 out=$4
	[ -f "$tree/site/build.sh" ] || {
		echo "site-versions: $tag has no site/build.sh: a tree older than the site cannot be built" >&2
		exit 1
	}
	grep -q SITE_BASE "$tree/site/astro.config.mjs" || {
		echo "site-versions: $tag's site does not read SITE_BASE, so it cannot be served under $base" >&2
		exit 1
	}
	SITE_BASE=$base SITE_REF=$tag sh "$tree/site/build.sh"
	rm -rf "$out"
	mkdir -p "$(dirname "$out")"
	cp -R "$tree/site/dist" "$out"
}

rm -rf "$OUT"
mkdir -p "$OUT"
if [ ! -s "$WORK/tags" ]; then
	build "$ROOT" main "$SITE_ROOT" "$WORK/root"
	cp -R "$WORK/root"/. "$OUT"/
	rm -rf "$WORK/root"
	echo "site-versions: no release tag; the working tree is at $SITE_ROOT/, without a version picker" >&2
	exit 0
fi
latest=$(tail -n 1 "$WORK/tags")
versions=
while read -r tag; do
	minor=${tag#v}
	minor=${minor%.*}
	versions="$minor $versions"
	build "$WORK/$tag" "$tag" "$SITE_ROOT/$minor" "$OUT/$minor"
	if [ "$tag" = "$latest" ]; then
		root=$WORK/root
		build "$WORK/$tag" "$tag" "$SITE_ROOT" "$root"
		cp -R "$root"/. "$OUT"/
		rm -rf "$root"
	fi
done < "$WORK/tags"

# shellcheck disable=SC2086 # the versions are words, newest first
node "$ROOT/site/versions/inject.mjs" "$OUT" "$SITE_ROOT" "https://github.com/oxidestack/velocity-rs" $versions
