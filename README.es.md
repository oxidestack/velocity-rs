# velocity-rs

<!-- languages:begin -->
[English](README.md) · [Русский](README.ru.md) · [中文](README.zh-CN.md) · **Español**
<!-- languages:end -->
<!-- translation of README.md: 26b9e44612ae -->

Un port no oficial a Rust de [Apache Velocity Engine](https://github.com/apache/velocity-engine) 2.4.1 y de las
herramientas genéricas (generic tools) de [Apache Velocity Tools](https://github.com/apache/velocity-tools) 3.1, fiel
a la salida del renderizado: la misma plantilla, las mismas variables, los mismos bytes, incluido el texto de los
errores. Apache, Apache Velocity y la pluma de Apache son marcas registradas de la Apache Software Foundation; este
proyecto no está afiliado a la Foundation ni cuenta con su aval ([`NOTICE`](NOTICE)).

Estado: previo a la primera versión. El crate aún no está publicado en crates.io; hasta entonces se usa desde una copia
local de este repositorio.

## Qué es

- **El Velocity Template Language tal como lo renderiza Velocity Engine**: el parser, las directivas (`#set`, `#if`,
  `#foreach`, `#macro`, `#parse`, `#include`, `#define`, `#evaluate`, `#break`, `#stop`), la introspección de valores
  (`UberspectImpl`, `SecureUberspector`), los manejadores de eventos, los cargadores de recursos y las propiedades de
  configuración, con los propios mensajes de error de Velocity.
- **Valores de Java**: una plantilla ve valores de Java del crate `java-compat-rs` (`JValue`: cadenas, números,
  listas, mapas, arrays, objetos), introspeccionados como los introspecciona Velocity, de modo que `$list.size()` o
  `$map.key` se comportan como en la JVM.
- **Las herramientas genéricas** de Velocity Tools 3.1 (`$math`, `$number`, `$date`, `$esc`, `$json`, `$xml` y las
  demás), en `velocity::tools`.
- **La configuración de MockServer en una llamada**: `Engine::mockserver()` configura el motor como lo hace MockServer
  (un cargador de recursos de cadenas, su directiva `#ifnull`, el gestor de herramientas con la caja de herramientas
  genérica, el plazo de renderizado).

Existe para [mock-server](https://github.com/oxidestack/mock-server), un sustituto directo de MockServer en Rust, que
renderiza sus plantillas de respuesta con Velocity y necesita la salida que produce MockServer. Es útil allí donde
una plantilla de Velocity tenga que renderizarse igual fuera de la JVM.

## Instalación

El crate se llama `velocity-rs` y en el código se usa como `velocity`. Aún no está publicado en crates.io; cuando lo
esté:

```toml
[dependencies]
velocity-rs = "0.1"                 # not published yet
java-compat-rs = "0.1"              # the Java values a template sees; not published yet
```

Hasta entonces, depende de una copia local: `velocity-rs = { path = "../velocity-rs" }`, con
[java-compat-rs](https://github.com/oxidestack/java-compat-rs) clonado a su lado.

## Inicio rápido

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

`Engine::new()` es la configuración por defecto de Velocity, y `Engine::with_properties` acepta sobrescrituras de
`velocity.properties` como lo hace `RuntimeInstance` de Velocity. La referencia de la API estará en docs.rs cuando el
crate se publique; desde una copia local la genera `cargo doc --open`.

## Guías

[Actualización](docs/i18n/es/guide/upgrading.md): lo que cada versión pide al código que usa el crate. Este README y las
guías son también un sitio, `https://oxidestack.github.io/velocity-rs/`, cuando el repositorio sea público.

## Compilar y probar

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

`tests/cases/*.expected.json` contienen salidas de las bibliotecas reales de Java; `reference/` y `generators/` las
regeneran (necesitan un JDK y Maven). `upstream/` es el código fuente de Velocity Engine fijado en la versión portada,
para consulta.

## Obtener ayuda

Una pregunta sobre cómo funciona el crate puede ir primero a [DeepWiki](https://deepwiki.com/oxidestack/velocity-rs):
responde a partir del código y la documentación de este repositorio, y donde no coincide con la documentación, la
documentación tiene razón. Las preguntas que no resuelva van a
[Discussions](https://github.com/oxidestack/velocity-rs/discussions), y los errores a
[issues](https://github.com/oxidestack/velocity-rs/issues). <!-- these three links need the public repository; it is not published yet -->

## Licencia

El código se distribuye bajo la [Apache License, Version 2.0](LICENSE) (`Apache-2.0`); lo que porta se atribuye en
[`NOTICE`](NOTICE). La documentación —este README y las guías de `docs/`— se distribuye bajo la licencia
[Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/) (`CC-BY-4.0`); su texto
legal está en [`LICENSE-docs`](LICENSE-docs). Consulta también [`CONTRIBUTING.md`](CONTRIBUTING.md),
[`GOVERNANCE.md`](GOVERNANCE.md) y [`SECURITY.md`](SECURITY.md).
