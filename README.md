# velocity-rs

<!-- languages:begin -->
**English** · [Русский](README.ru.md) · [中文](README.zh-CN.md) · [Español](README.es.md)
<!-- languages:end -->

An unofficial Rust port of [Apache Velocity Engine](https://github.com/apache/velocity-engine) 2.4.1 and of the
generic tools of [Apache Velocity Tools](https://github.com/apache/velocity-tools) 3.1, faithful to the rendering
output: the same template, the same variables, the same bytes, including the error text. Apache, Apache Velocity and
the Apache feather are trademarks of the Apache Software Foundation; this project is not affiliated with or endorsed
by the Foundation ([`NOTICE`](NOTICE)).

Status: pre-release. The crate is not published on crates.io yet; until it is, it is used from a checkout of this
repository.

## What it is

- **The Velocity Template Language as Velocity Engine renders it**: the parser, the directives (`#set`, `#if`,
  `#foreach`, `#macro`, `#parse`, `#include`, `#define`, `#evaluate`, `#break`, `#stop`), the introspection of
  values (`UberspectImpl`, `SecureUberspector`), the event handlers, the resource loaders and the configuration
  properties, with Velocity's own error messages.
- **Java values**: a template sees Java values from the `java-compat-rs` crate (`JValue`: strings, numbers, lists,
  maps, arrays, objects), introspected the way Velocity introspects them, so `$list.size()` or `$map.key` behave as
  on the JVM.
- **The generic tools** of Velocity Tools 3.1 (`$math`, `$number`, `$date`, `$esc`, `$json`, `$xml` and the
  others), under `velocity::tools`.
- **MockServer's configuration in one call**: `Engine::mockserver()` configures the engine as MockServer does (a
  string resource loader, its `#ifnull` directive, the tool manager with the generic toolbox, the render deadline).

It exists for [mock-server](https://github.com/oxidestack/mock-server), a Rust drop-in for MockServer, which renders
its response templates with Velocity and needs the output MockServer produces. It is useful wherever a Velocity
template has to render the same way outside the JVM.

## Install

The crate is `velocity-rs` and is used in code as `velocity`. It is not published on crates.io yet; once it is:

```toml
[dependencies]
velocity-rs = "0.1"                 # not published yet
java-compat-rs = "0.1"              # the Java values a template sees; not published yet
```

Until then, depend on a checkout: `velocity-rs = { path = "../velocity-rs" }`, with
[java-compat-rs](https://github.com/oxidestack/java-compat-rs) checked out beside it.

## Quick start

```rust
use java_compat::lang::JValue;
use velocity::{Context, Engine};

fn main() -> Result<(), velocity::VelocityError> {
    let engine = Engine::mockserver();
    let mut context = Context::new();
    context.put("name", JValue::str("world"));
    let out = engine.render("Hello, $name! #foreach($i in [1..3])$i#if($foreach.hasNext), #end#end", &mut context)?;
    assert_eq!(out, "Hello, world! 1, 2, 3");
    Ok(())
}
```

`Engine::new()` is Velocity's default configuration, and `Engine::with_properties` takes `velocity.properties`
overrides as Velocity's `RuntimeInstance` does. The API reference will be on docs.rs once the crate is published;
`cargo doc --open` builds it from a checkout.

## Guides

[Upgrading](docs/guide/upgrading.md): what each release asks of code that uses the crate. This README and the guides
are also a site, `https://oxidestack.github.io/velocity-rs/`, once the repository is public.

## Build and test

```sh
git submodule update --init upstream   # the ported source, read-only reference
git clone https://github.com/oxidestack/java-compat-rs ../java-compat-rs   # the JDK semantics this crate builds on
cargo test
make lint        # clippy, warnings are errors
make doc         # the API documentation, rustdoc's warnings are errors
make deny        # licences, advisories, sources (cargo install cargo-deny --locked)
make public-check
make site        # the documentation site (site/README.md), built in a pinned node container
```

`tests/cases/*.expected.json` hold outputs of the real Java libraries; `reference/` and `generators/` regenerate them
(they need a JDK and Maven). `upstream/` is the Velocity Engine source pinned to the ported release, for reading.

## Getting help

A question about how the crate works can go to [DeepWiki](https://deepwiki.com/oxidestack/velocity-rs) first: it
answers from this repository's code and documentation, and where it disagrees with the documentation, the
documentation is right. Questions it does not settle go to
[Discussions](https://github.com/oxidestack/velocity-rs/discussions), bugs to
[issues](https://github.com/oxidestack/velocity-rs/issues). <!-- these three links need the public repository; it is not published yet -->

## Licence

The code is licensed under the [Apache License, Version 2.0](LICENSE) (`Apache-2.0`); what it ports is attributed in
[`NOTICE`](NOTICE). The documentation — this README and the guides under `docs/` — is licensed under the
[Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/) licence
(`CC-BY-4.0`); its legal text is in [`LICENSE-docs`](LICENSE-docs). See [`CONTRIBUTING.md`](CONTRIBUTING.md),
[`GOVERNANCE.md`](GOVERNANCE.md) and [`SECURITY.md`](SECURITY.md).
