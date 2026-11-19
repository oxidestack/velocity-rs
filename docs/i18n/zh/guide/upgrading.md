# 升级

<!-- languages:begin -->
[English](../../../guide/upgrading.md) · [Русский](../../ru/guide/upgrading.md) · **中文** · [Español](../../es/guide/upgrading.md)
<!-- languages:end -->
<!-- translation of docs/guide/upgrading.md: 2292f213a8cc -->

如何把使用 velocity-rs 的代码从它所依赖的版本迁移到较新的版本。找出每个比你的版本更新的版本的章节，并按从旧到新的顺序应用：从
0.1.0 升级到 0.3.0，意味着先应用 0.2.0 的章节，再应用 0.3.0 的章节。无需任何改动的版本也会写明这一点。然后一步[升级](#如何升级)到最新版本：
从来不需要先安装某个中间版本。

每个章节列出该版本对使用本 crate 的代码提出的要求：**Action required** 下是升级时需要做的改动；**Deprecated**
下是仍然可用、但会在以后的版本中停止工作的内容；**Removed** 下是已经停止工作的内容。每个版本的其他所有改动见
[变更日志](https://github.com/oxidestack/velocity-rs/blob/main/CHANGELOG.md)。只有正式版本才有章节：候选版本（`-rc.N`）在其 GitHub release 中说明，它带来的改动写在包含这些改动的正式版本之下。

你所使用的版本的文档也在本站点上：每页顶部的版本选择器可在各版本之间切换，旧版本的页面会链接到最新版本中的同一页面。

## 如何升级

velocity-rs 是一个库：除非下面的章节另有要求，升级只需修改 `Cargo.toml` 中的一行，你的其他代码无需改动。版本号遵循
[Semantic Versioning](https://semver.org/spec/v2.0.0.html)：在 1.0.0 之前，新的次版本（0.1 到 0.2）可能改变 API，补丁版本（0.1.0 到 0.1.1）则不会。

1. 阅读下面每个比你的版本更新的版本的章节，并对你的代码做出相应改动。
2. 修改 `Cargo.toml` 中的版本（`velocity-rs = "<version>"`），然后运行 `cargo update -p velocity-rs`。
3. 构建并运行你的测试。跟进较新 upstream 版本的版本会在其章节中说明，并写明输出有哪些变化。要回退，把 `Cargo.toml` 中的版本改回原来的版本即可。

## 升级到 0.1.0（首个版本，日期尚未确定）

velocity-rs 的首个版本：Apache Velocity Engine 2.4.1 及 Velocity Tools 3.1 通用工具（generic tools）的移植。没有可以从中升级的更早版本。

### Action required

无。

### Deprecated

无。

### Removed

无。
