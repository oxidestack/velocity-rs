# The documentation site

[Astro Starlight](https://starlight.astro.build) over the documentation this repository already has: the
[README](../README.md) and the guides in [`docs/guide/`](../docs/guide/). It is served as a project page at
<https://oxidestack.github.io/velocity-rs/> once the repository is public and Pages is on.

```sh
make site    # site/dist/, built in the pinned node image; a broken internal link or anchor fails it
```

Node runs only in a container: `make site` runs [`build.sh`](build.sh) in the node 22 image that the Makefile pins by
digest (`SITE_NODE_IMAGE`), and `npm ci` installs exactly [`package-lock.json`](package-lock.json)'s tree, every
package pinned by version and integrity hash. npm's cache is `target/site/npm-cache`. To look at the pages while
editing them, run the dev server in that image, from the repository root:

```sh
docker run --rm -it -p 4321:4321 --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/src" -w /src/site \
  "$(sed -n 's/^SITE_NODE_IMAGE ?= //p' Makefile)" sh -c 'sh build.sh && npm run dev'
```

and open <http://localhost:4321/velocity-rs/>.

## A page has one home, and it is not here

Nothing is copied into this directory by hand. Every build runs [`scripts/stage.mjs`](scripts/stage.mjs), which reads
`README.md`, `docs/guide/*.md` and their translations ([Languages](#languages)) and writes them into
`src/content/docs/` with these changes and no others:

| Change | Why |
| --- | --- |
| The first `#` heading becomes the page's title | Starlight takes the title from frontmatter |
| `[Configuration](configuration.md#server)` becomes `/velocity-rs/configuration/#server`; a link to any other file of the repository becomes a link to it on GitHub | right on GitHub, a 404 on the site, which carries only the documentation |
| `<name>` outside code becomes `&lt;name>` | a renderer reads it as an HTML tag and drops it |
| An image, `![…](site/src/assets/…)`, is linked relative to the staged page; an image of another file of the repository fails the staging | a GitHub page is no image, and a file under `src/` is bundled by the build |

`src/content/docs/`, `dist/`, `.astro/` and `node_modules/` are build output, ignored by git: edit `README.md` or
`docs/guide/…`, never a staged page. A new guide page is staged by being there; it appears in the navigation once
[`astro.config.mjs`](astro.config.mjs)'s `sidebar` names it.

## Versions

The site serves releases, never `main`: the latest release at the root, `/velocity-rs/`, and every minor under
`/velocity-rs/<x.y>/`, each built by its own tag's `site/` (its own lockfile and pages), with a version picker in
the header and, on an older release, a banner that points at the same page in the latest and at its upgrade guide.
Only stable tags `vX.Y.Z` count; a release candidate's notes live on its GitHub release. A minor is built from its
latest patch tag (`1.1` is `v1.1.3` once that is out).

```sh
make site-versions    # into target/site/pages/, as GitHub Pages serves it
```

[`versions/extract.sh`](versions/extract.sh) (on the host, it needs git) unpacks each release tag with `git archive`;
[`versions/build.sh`](versions/build.sh) (in the node image) builds each with `SITE_BASE` (its base path) and
`SITE_REF` (the tag, which the links to repository files name), and the latest once more at the root;
[`versions/inject.mjs`](versions/inject.mjs) then gives every page a `docs-version` meta tag and
[`versions/versions.js`](versions/versions.js), which draws the picker and the banner from `versions.json` at run time
(so a release built long ago lists the ones after it), and marks every `/<x.y>/` copy `noindex`. Its tests:
`node --test site/versions/inject.test.mjs`. A tag older than the site, or whose `astro.config.mjs` does not read
`SITE_BASE`, fails the build with a message naming it.

**No release tag yet.** `make site-versions` still succeeds: it builds the working tree once, at the root, with no
picker and no `versions.json`, and says so. The first stable tag changes that with no other edit. The deployment that
publishes `target/site/pages/` is [Where it is served from](#where-it-is-served-from)'s.

## Languages

English is the original, at the site's root. The README is translated into Russian, Chinese and Spanish beside it:
`README.ru.md`, `README.zh-CN.md`, `README.es.md`. A guide page is translated as a file of `docs/i18n/` at the page's
own path under its language: `docs/i18n/zh/guide/upgrading.md` translates `docs/guide/upgrading.md`. The languages and
their READMEs are [`locales.mjs`](locales.mjs)'s, each labelled with its code and its own name, `RU (Русский)`;
adding one is a line there, a README and a directory. The translations themselves are written separately: until they
exist, the other languages show the English page, with Starlight's note that it is not translated yet.

On GitHub, an original and its translations link each other in a languages line at their head, between
`<!-- languages:begin -->` and `<!-- languages:end -->`. On the site the language picker does that, so the staging
takes the line out. The staging fails when an original's line misses a translation that exists or links one that does not
(`scripts/stage.mjs`, `languages()`).
A translated page is served under its language, `/velocity-rs/ru/upgrading/`.

A translation records which version of its original it translates: `<!-- translation of README.md: 1a2b3c4d5e6f -->`,
the first 12 hex digits of the original's SHA-256 (`sha256sum README.md | cut -c1-12`). The staging fails on a
translation without one; when the original's digest is another, the staging logs the translation as possibly out of
date and the page carries Starlight's banner saying so, in its language, with a link to the original.

## The checks

[starlight-links-validator](https://github.com/HiDeoo/starlight-links-validator) fails the build on an internal link
to a page that does not exist or to an anchor that is not a heading of its page, and the staging fails on a link to a
file that is not in the repository. Before `npm ci` fetches anything, [`scripts/licences.mjs`](scripts/licences.mjs)
holds every package of the lockfile to `deny.toml`'s licence allowlist (the CNCF's allowed third-party licences); the
few build tools outside it (libvips, lightningcss, three BlueOak-licensed helpers, CSS syntax data) are listed there
by name with the licence they were reviewed under: each runs only during the build, and nothing of theirs is in
`dist/`, which the CNCF policy treats as internal tooling needing no exception.

## Where it is served from

`oxidestack.github.io` is shared by every project of the organisation, so this site is a project page under
`/velocity-rs/`: that is `base` in `astro.config.mjs`, and every internal link carries it.

[`.github/workflows/site.yml`](../.github/workflows/site.yml), which the CI workflow calls as its `site` job,
builds `make site` and `make site-versions` on every push and pull request and deploys `target/site/pages/` to
GitHub Pages on a push to `main`, only while the repository is public: GitHub Pages refuses a private repository of
a Free organisation, and while the repository is private no CI job runs on a push at all. It needs the repository's
Settings → Pages → Source set to GitHub Actions.

## The logo

None yet: `astro.config.mjs` marks where `logo` and `favicon` go.
