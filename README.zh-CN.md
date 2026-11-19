# velocity-rs

<!-- languages:begin -->
[English](README.md) · [Русский](README.ru.md) · **中文** · [Español](README.es.md)
<!-- languages:end -->
<!-- translation of README.md: 26b9e44612ae -->

[Apache Velocity Engine](https://github.com/apache/velocity-engine) 2.4.1 及
[Apache Velocity Tools](https://github.com/apache/velocity-tools) 3.1 通用工具（generic tools）的非官方 Rust
移植，忠实于渲染输出：同样的模板、同样的变量，得到同样的字节，连错误文本也相同。Apache、Apache Velocity 和 Apache
羽毛标志是 Apache Software Foundation 的商标；本项目与该基金会无关联，也未获其认可（[`NOTICE`](NOTICE)）。

状态：尚未发布。本 crate 还没有发布到 crates.io；在此之前，请从本仓库的本地检出使用它。

## 简介

- **按 Velocity Engine 的方式渲染 Velocity Template Language**：解析器、指令（`#set`、`#if`、`#foreach`、
  `#macro`、`#parse`、`#include`、`#define`、`#evaluate`、`#break`、`#stop`）、值的内省（`UberspectImpl`、
  `SecureUberspector`）、事件处理器、资源加载器和配置属性，并使用 Velocity 自己的错误消息。
- **Java 值**：模板看到的是 `java-compat-rs` crate 中的 Java 值（`JValue`：字符串、数字、列表、map、数组、对象），
  内省方式与 Velocity 相同，因此 `$list.size()` 或 `$map.key` 的行为与在 JVM 上一致。
- **Velocity Tools 3.1 的通用工具**（`$math`、`$number`、`$date`、`$esc`、`$json`、`$xml` 等），位于
  `velocity::tools`。
- **一次调用即得 MockServer 的配置**：`Engine::mockserver()` 按 MockServer 的方式配置引擎（字符串资源加载器、它的
  `#ifnull` 指令、带通用工具箱的工具管理器、渲染截止时间）。

它是为 [mock-server](https://github.com/oxidestack/mock-server) 而写的。mock-server 是 MockServer 的 Rust
替代品，用 Velocity 渲染响应模板，需要与 MockServer 相同的输出。凡是需要在 JVM 之外以相同方式渲染 Velocity
模板的地方，它都能派上用场。

## 安装

crate 名为 `velocity-rs`，在代码中以 `velocity` 使用。它尚未发布到 crates.io；发布之后：

```toml
[dependencies]
velocity-rs = "0.1"                 # not published yet
java-compat-rs = "0.1"              # the Java values a template sees; not published yet
```

在此之前，请依赖本地检出：`velocity-rs = { path = "../velocity-rs" }`，并在其旁边检出
[java-compat-rs](https://github.com/oxidestack/java-compat-rs)。

## 快速开始

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

`Engine::new()` 是 Velocity 的默认配置，`Engine::with_properties` 则像 Velocity 的 `RuntimeInstance` 一样接受
`velocity.properties` 的覆盖项。crate 发布后，API 参考将位于 docs.rs；在本地检出中可用 `cargo doc --open` 生成。

## 指南

[升级](docs/i18n/zh/guide/upgrading.md)：每个版本对使用本 crate 的代码有哪些要求。仓库公开后，本 README 和这些指南也会以
站点形式提供：`https://oxidestack.github.io/velocity-rs/`。

## 构建与测试

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

`tests/cases/*.expected.json` 保存真实 Java 库的输出；`reference/` 和 `generators/` 用于重新生成它们（需要 JDK 和
Maven）。`upstream/` 是固定在所移植版本上的 Velocity Engine 源码，仅供阅读。

## 获取帮助

关于本 crate 如何工作的问题，可以先问 [DeepWiki](https://deepwiki.com/oxidestack/velocity-rs)：它根据本仓库的代码和文档
作答；若它与文档不一致，以文档为准。它解决不了的问题请发到
[Discussions](https://github.com/oxidestack/velocity-rs/discussions)，缺陷请提交到
[issues](https://github.com/oxidestack/velocity-rs/issues)。 <!-- these three links need the public repository; it is not published yet -->

## 许可证

代码采用 [Apache License, Version 2.0](LICENSE)（`Apache-2.0`）许可；所移植的内容在 [`NOTICE`](NOTICE) 中注明出处。
文档（本 README 及 `docs/` 下的指南）采用
[Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/)（`CC-BY-4.0`）许可；
其法律文本见 [`LICENSE-docs`](LICENSE-docs)。另见 [`CONTRIBUTING.md`](CONTRIBUTING.md)、
[`GOVERNANCE.md`](GOVERNANCE.md) 和 [`SECURITY.md`](SECURITY.md)。
