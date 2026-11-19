# Upgrading

<!-- languages:begin -->
**English** · [Русский](../i18n/ru/guide/upgrading.md) · [中文](../i18n/zh/guide/upgrading.md) ·
[Español](../i18n/es/guide/upgrading.md)
<!-- languages:end -->

How to move code that uses velocity-rs from the release it depends on to a newer one. Find the section of every release
newer than yours, and apply them oldest first: going from 0.1.0 to 0.3.0 means the section of 0.2.0, then the one of
0.3.0. A release with nothing to change says so. Then [upgrade](#how-to-upgrade) to the newest release in one step: a
release never needs an intermediate one first.

Each section lists what the release asks of code that uses the crate: under **Action required**, a change to make
with the upgrade; under **Deprecated**, something that still works and will stop in a later release; under
**Removed**, something that has stopped. The [changelog](https://github.com/oxidestack/velocity-rs/blob/main/CHANGELOG.md) says everything else each release changed. Only
releases have a section: a release candidate (`-rc.N`) is described on its GitHub release, and what it changes is
written here under the release that ships it.

The documentation of the release you use is on this site too: the version picker at the top of each page switches
between releases, and a page of an older release links the same page in the latest.

## How to upgrade

velocity-rs is a library: an upgrade changes one line of `Cargo.toml` and nothing else of yours, unless a section below
asks for more. The versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html): before 1.0.0 a new
minor version (0.1 to 0.2) may change the API, and a patch (0.1.0 to 0.1.1) does not.

1. Read the sections below for every release newer than yours, and make their changes to your code.
2. Change the version in `Cargo.toml` (`velocity-rs = "<version>"`) and run `cargo update -p velocity-rs`.
3. Build and run your tests. A release that follows a newer upstream release says so in its section, with what
   changes in the output. To roll back, put the version you had back in `Cargo.toml`.

## Upgrading to 0.1.0 (first release, date not yet set)

<!-- Placeholder: the date is set when the first release is cut. Every later release gets its own section above this
     one. -->

The first release of velocity-rs: a port of Apache Velocity Engine 2.4.1 and the generic tools of Velocity Tools 3.1. There is no earlier release to upgrade from.

### Action required

Nothing.

### Deprecated

Nothing.

### Removed

Nothing.
