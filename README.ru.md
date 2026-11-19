# velocity-rs

<!-- languages:begin -->
[English](README.md) · **Русский** · [中文](README.zh-CN.md) · [Español](README.es.md)
<!-- languages:end -->
<!-- translation of README.md: 26b9e44612ae -->

Неофициальный порт [Apache Velocity Engine](https://github.com/apache/velocity-engine) 2.4.1 и общих инструментов
(generic tools) [Apache Velocity Tools](https://github.com/apache/velocity-tools) 3.1 на Rust, верный результату
рендеринга: тот же шаблон, те же переменные — те же байты, включая текст ошибок. Apache, Apache Velocity и перо Apache —
товарные знаки Apache Software Foundation; этот проект не связан с Foundation и не одобрен ею ([`NOTICE`](NOTICE)).

Статус: до первого выпуска. Крейт ещё не опубликован на crates.io; пока этого не произошло, его используют из
локальной копии этого репозитория.

## Что это

- **Velocity Template Language так, как его рендерит Velocity Engine**: парсер, директивы (`#set`, `#if`,
  `#foreach`, `#macro`, `#parse`, `#include`, `#define`, `#evaluate`, `#break`, `#stop`), интроспекция значений
  (`UberspectImpl`, `SecureUberspector`), обработчики событий, загрузчики ресурсов и свойства конфигурации — с
  собственными сообщениями об ошибках Velocity.
- **Значения Java**: шаблон видит значения Java из крейта `java-compat-rs` (`JValue`: строки, числа, списки, map,
  массивы, объекты), и интроспекция их такая же, как у Velocity, поэтому `$list.size()` или `$map.key` ведут себя
  так же, как на JVM.
- **Общие инструменты** Velocity Tools 3.1 (`$math`, `$number`, `$date`, `$esc`, `$json`, `$xml` и другие) — в
  `velocity::tools`.
- **Конфигурация MockServer одним вызовом**: `Engine::mockserver()` настраивает движок так же, как MockServer
  (строковый загрузчик ресурсов, его директива `#ifnull`, менеджер инструментов с общим набором, дедлайн рендеринга).

Крейт создан для [mock-server](https://github.com/oxidestack/mock-server) — замены MockServer на Rust, которая
рендерит шаблоны ответов через Velocity и должна выдавать тот же результат, что и MockServer. Он полезен везде, где
шаблон Velocity нужно отрендерить так же, но вне JVM.

## Установка

Крейт называется `velocity-rs`, а в коде используется как `velocity`. На crates.io он ещё не опубликован; после
публикации:

```toml
[dependencies]
velocity-rs = "0.1"                 # not published yet
java-compat-rs = "0.1"              # the Java values a template sees; not published yet
```

До тех пор зависьте от локальной копии: `velocity-rs = { path = "../velocity-rs" }`, а рядом положите копию
[java-compat-rs](https://github.com/oxidestack/java-compat-rs).

## Быстрый старт

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

`Engine::new()` — конфигурация Velocity по умолчанию, а `Engine::with_properties` принимает переопределения
`velocity.properties`, как это делает `RuntimeInstance` в Velocity. Справочник API появится на docs.rs после
публикации крейта; из локальной копии его собирает `cargo doc --open`.

## Руководства

[Обновление](docs/i18n/ru/guide/upgrading.md): что каждый выпуск требует от кода, использующего крейт. Этот README и
руководства доступны также в виде сайта, `https://oxidestack.github.io/velocity-rs/`, после того как репозиторий
станет публичным.

## Сборка и тесты

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

`tests/cases/*.expected.json` содержат вывод настоящих библиотек Java; `reference/` и `generators/` генерируют их
заново (для этого нужны JDK и Maven). `upstream/` — исходный код Velocity Engine, закреплённый на портированном
выпуске, для чтения.

## Помощь

Вопрос о том, как работает крейт, можно сначала задать [DeepWiki](https://deepwiki.com/oxidestack/velocity-rs): она
отвечает по коду и документации этого репозитория, а там, где она расходится с документацией, права документация.
Вопросы, которые она не решает, — в [Discussions](https://github.com/oxidestack/velocity-rs/discussions), ошибки — в
[issues](https://github.com/oxidestack/velocity-rs/issues). <!-- these three links need the public repository; it is not published yet -->

## Лицензия

Код распространяется по лицензии [Apache License, Version 2.0](LICENSE) (`Apache-2.0`); то, что он портирует,
указано в [`NOTICE`](NOTICE). Документация — этот README и руководства в `docs/` — распространяется по лицензии
[Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/) (`CC-BY-4.0`); её
юридический текст — в [`LICENSE-docs`](LICENSE-docs). См. также [`CONTRIBUTING.md`](CONTRIBUTING.md),
[`GOVERNANCE.md`](GOVERNANCE.md) и [`SECURITY.md`](SECURITY.md).
