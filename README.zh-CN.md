# arcthis

**面向 AI Agent 的压缩文件访问 CLI。**

`arcthis` 是一层统一的 Archive Access Layer，让人类和 AI Agent 在不默认完整释放 archive 的前提下，发现、列举、流式读取、提取、创建和验证压缩文件内容。

[English](./README.md)

## 为什么需要 arcthis？

Agent 不应该为了找到一个 `README.md`，先把数 GB 数据集完整解压到临时目录。`arcthis` 把 archive 视为可访问的文件树：

```sh
arcthis inspect dataset.tar.gz --json
arcthis tree dataset.tar.gz --json
arcthis stat dataset.tar.gz train/data.csv --json
arcthis read dataset.tar.gz train/data.csv | head
```

`read` 会把一个 entry 直接流式写入 stdout，因此可以自然组合现有 Unix 工具：

```sh
arcthis read source.zip src/lib.rs | rg unsafe
arcthis read media.zip video.mp4 | ffprobe -i pipe:0
```

“不完整物化整个 archive”并不等于所有格式都拥有常量时间随机访问。ZIP 通常可以直接解码目标 entry；TAR 和 TAR.GZ 需要顺序扫描。`inspect --json` 会明确报告当前 backend 的能力和访问成本。

## 当前状态

仓库已经完成并测试 v0.1 基础实现，但尚未发布到 crates.io，也没有预构建二进制 release。

当前正式命令包括：

- `list`、`tree`、`stat`、`inspect` 和流式 `read`
- 安全的完整或单 entry `extract`
- 事务式 `pack`
- 全流式 `verify`
- 所有结构化命令的 schema-versioned JSON

尚未实现的格式和命令只记录在 [ROADMAP.md](./ROADMAP.md)，不会在当前能力列表中冒充可用。

## 支持格式

| 格式 | 输入识别 | 访问/提取 | 创建 | 访问模型 |
| --- | --- | --- | --- | --- |
| ZIP | Magic bytes | Stored 与 Deflate entry | Deflate | 随机 entry 访问 |
| TAR | 校验 TAR header | 支持 | 支持 | 顺序访问 |
| TAR.GZ / TGZ | Gzip magic + TAR 校验 | 支持 | 支持 | 顺序解码 |

输入 archive 以内容识别为主，伪装扩展名不会覆盖有效 signature。`pack` 则根据用户要求的输出后缀选择新 archive 格式。

当前 ZIP 构建只启用 Stored/Deflate。即使 ZIP 内含其他压缩方法，metadata listing 通常仍能工作；需要读取或验证对应内容时会返回 `unsupported_operation`。

## 从源码安装

项目通过 `rust-toolchain.toml` 固定 Rust 1.98.0。

```sh
cargo build --release --locked
./target/release/arcthis --help
```

把当前 checkout 安装到 Cargo 二进制目录：

```sh
cargo install --path . --locked
```

## 快速开始

```sh
# 先发现，再决定是否提取
arcthis inspect archive.zip
arcthis list archive.zip
arcthis tree archive.zip --json

# 流式读取一个 entry
arcthis read archive.zip README.md

# 通过同目录临时文件安全提交一个普通文件
arcthis extract archive.zip README.md --output ./README.md

# 安全释放整个 archive
arcthis extract archive.tar.gz

# 创建、重新打开、验证并提交新 archive
arcthis pack ./project --output project.tar.gz

# 验证每个可读 entry
arcthis verify project.tar.gz --json
```

完整的目标目录规则、资源限制、JSON schema、退出码和命令说明见 [START.zh-CN.md](./START.zh-CN.md)。

## 安全模型

完整提取会先完成 metadata 预检和路径校验，拒绝 link/special file，执行声明大小与真实写入大小限制，然后只向同文件系统 staging 目录写入；所有 entry 成功后才提交。已有目标默认拒绝，v0.1 没有覆盖模式。

压缩创建会先写入同目录临时 archive，完成 encoder finalize，重新通过统一 archive interface 打开并逐 entry 验证，最后才提交到目标路径。源文件不会被删除。

精确定义和已知限制见 [docs/SECURITY.md](./docs/SECURITY.md)。`--dry-run`、`--delete-source`、覆盖策略、嵌套 archive 和加密 archive 都属于规划能力，当前没有半成品实现。

## Agent interface

- 成功的结构化输出使用 `schema_version: "1"`。
- 结果写 stdout；警告与错误写 stderr。
- 使用 `--json` 时，machine error 是 stderr 中的 JSON。
- `read` 始终输出原始 bytes，并拒绝 `--json`。
- BrokenPipe 按消费者正常提前结束处理。
- Entry metadata 使用 `path_encoding` 明确标识非 UTF-8 名称。
- 非 TTY 和 JSON 输出不包含 ANSI decoration。

公共 schema 与错误模型见 [docs/CLI.md](./docs/CLI.md)。

## 平台支持

v0.1 面向 macOS 与 Linux 开发和测试，并配置了两个平台的 GitHub Actions。架构避免把 Unix-only 类型暴露为 public interface，但 Windows 暂时不是正式 CI 目标。

## 开发入口

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --release --locked
```

重要文档：

- [START.zh-CN.md](./START.zh-CN.md) — 完整使用指南
- [INDEX.md](./INDEX.md) — 简洁代码库地图
- [docs/PRODUCT.md](./docs/PRODUCT.md) — 产品定义与非目标
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — archive interface 与 backend 设计
- [docs/SECURITY.md](./docs/SECURITY.md) — 提取与生命周期安全
- [AGENTS.md](./AGENTS.md) — 代码 Agent 的长期仓库约束
- [CONTRIBUTING.md](./CONTRIBUTING.md) — 贡献流程

## Roadmap、贡献与许可证

分阶段格式与能力计划见 [ROADMAP.md](./ROADMAP.md)。贡献必须保持统一命令语义、流式 I/O、schema 兼容性和保守的 extraction 默认值，修改公共行为前请阅读 [CONTRIBUTING.md](./CONTRIBUTING.md)。

`arcthis` 使用 [MIT License](./LICENSE)。
