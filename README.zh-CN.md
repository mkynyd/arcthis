# arcthis

**面向 AI Agent 的压缩文件命令行工具。**

`arcthis` 是一个统一的压缩文件访问工具，让人类和 AI Agent 不必先把整个压缩包解压出来，就能查看、列出、查找、搜索、计算校验值、直接读取、解压、创建和验证压缩文件内容。

[English](./README.md)

## 为什么需要 arcthis？

Agent 不应该为了找到一个 `README.md`，先把数 GB 数据集完整解压到临时目录。`arcthis` 把压缩包当成一棵可以浏览的文件树：

```sh
arcthis inspect dataset.tar.gz --json
arcthis tree dataset.tar.gz --json
arcthis stat dataset.tar.gz train/data.csv --json
arcthis find dataset.tar.gz --glob '**/*.csv' --json
arcthis read dataset.tar.gz train/data.csv | head
```

`read` 会把一个文件的内容直接输出到终端（stdout），因此可以自然地和其他 Unix 工具组合使用：

```sh
arcthis read source.zip src/lib.rs | rg unsafe
arcthis read media.zip video.mp4 | ffprobe -i pipe:0
```

“不必解压整个压缩包”并不等于所有格式都能瞬间定位到某个文件。ZIP 通常可以直接读取目标文件；TAR 和 TAR.GZ 需要从头按顺序扫描。`inspect --json` 会明确报告当前格式实际支持的能力和访问成本。

## 当前状态

仓库已经完成并测试 v0.5 基础实现，但尚未发布到 crates.io，也没有编译好的可执行文件 release。

当前正式命令包括：

- `list`、`tree`、`stat`、`inspect` 和直接读取的 `read`
- 基于通配符的 `find`、有上限的逐行文本搜索 `grep`，以及 SHA-256/SHA-512 校验值 `hash`
- 使用可重复 `--within` 明确访问嵌套的压缩包
- 通过 `--password-file` 访问加密 ZIP 与 7z
- 通过可重复 `--volume` 按顺序组合分卷文件
- 可显式创建、刷新和删除的持久化文件列表缓存 `index`
- 安全的完整解压或单个文件解压 `extract`
- 有并发上限和递归发现的 `extract-all`
- 全部成功才保存的 `pack`、`--dry-run`、目标已存在处理方式与 `--delete-source`
- 边读边校验的 `verify`
- 先写临时文件、重新验证后才保存的格式转换 `convert`
- 可选的本地 stdio MCP 入口：9 个有上限的只读工具，以及 6 个需显式授权的先计划后执行写工具
- 所有结构化命令的 JSON 输出都带版本号

尚未实现的格式和命令只记录在 [ROADMAP.md](./ROADMAP.md)，不会在当前能力列表中冒充可用。

## 支持格式

| 格式 | 识别方式 | 访问/解压 | 创建 | 读取方式 |
| --- | --- | --- | --- | --- |
| ZIP | 文件特征 | Stored/Deflate，支持 ZipCrypto/AES 解密 | Deflate，不加密 | 可直接定位到单个文件 |
| 7z | 文件特征 | 支持，包括 AES 解密 | LZMA2，不加密 | 取决于压缩块/solid |
| RAR / RAR5 | 文件特征 | 通过 libarchive 读取/解压 | 不支持 | 按顺序读取，受专有格式限制 |
| TAR | 校验 TAR 文件头 | 支持 | 支持 | 按顺序读取 |
| TAR.GZ / TGZ | Gzip 特征 + TAR 校验 | 支持 | 支持 | 按顺序解压 |
| TAR.BZ2 / TBZ2 | Bzip2 特征 + TAR 校验 | 支持 | 支持 | 按顺序解压 |
| TAR.XZ / TXZ | XZ 特征 + TAR 校验 | 支持 | 支持 | 按顺序解压 |
| TAR.ZST / TZST | Zstandard 特征 + TAR 校验 | 支持 | 支持 | 按顺序解压 |
| GZIP | 文件特征，非 TAR 内容 | 一个隐式文件 | 支持 | 按顺序解压 |
| BZIP2 | 文件特征，非 TAR 内容 | 一个隐式文件 | 支持 | 按顺序解压 |
| XZ | 文件特征，非 TAR 内容 | 一个隐式文件 | 支持 | 按顺序解压 |
| Zstandard | 文件特征，非 TAR 内容 | 一个隐式文件 | 支持 | 按顺序解压 |

输入压缩包以内容识别为主，伪装扩展名不会覆盖有效格式特征。`pack` 则根据用户指定的输出后缀选择新压缩格式。

当前 ZIP 构建启用 Stored/Deflate 与 AES 解密。即使 ZIP 使用其他压缩方法，通常仍能列出文件；需要读取或验证对应内容时会返回 `unsupported_operation`。RAR 明确保持只读，底层实现、许可、加密和原生分卷边界见 [docs/RAR.md](./docs/RAR.md)。

## 从源码安装

项目通过 `rust-toolchain.toml` 固定 Rust 1.98.0。

RAR 支持会把 libarchive 静态链接进编译好的可执行文件，源码构建仍需要本机开发依赖。macOS 可通过 Homebrew 安装 `libarchive libb2 bzip2 lz4 xz zstd`；Debian/Ubuntu 需要 `libarchive-dev libb2-dev libbz2-dev liblz4-dev liblzma-dev libxml2-dev libzstd-dev zlib1g-dev`。

```sh
cargo build --release --locked
./target/release/arcthis --help
```

把当前源码安装到 Cargo 二进制目录：

```sh
cargo install --path . --locked
```

默认构建仍然是不携带 MCP 依赖的命令行工具和代码库。需要本地 MCP 入口时显式启用功能开关：

```sh
cargo build --release --locked --features mcp
arcthis mcp --allow-root ./archives
# 只有配置输出目录后，写工具才会出现：
arcthis mcp --allow-root ./archives --allow-output-root ./outputs
```

stdio 服务固定使用 MCP `2025-06-18` 协议版本。`archive_read` 强制提供 offset/length（偏移量和长度），并受单次窗口上限约束。删除 source 还必须同时满足服务的 `--allow-source-deletion` 与 plan/execute 请求里的 `delete_source: true`。完整规则见 [RFC 0003](./docs/RFC-0003-MCP-INTEGRATION.md)。

## 快速开始

```sh
# 先发现，再决定是否解压
arcthis inspect archive.zip
arcthis list archive.zip
arcthis tree archive.zip --json

# 直接读取一个文件
arcthis read archive.zip README.md

# 解压前发现并搜索内容
arcthis find source.tar.zst --glob '**/*.rs' --json
arcthis grep source.tar.zst TODO --glob '**/*.rs' --json
arcthis hash archive.zip model.bin --algorithm sha256

# 不创建临时中间文件，直接浏览内层压缩包
arcthis tree backup.zip --within project.tar.gz --json

# 从文件读取密码，避免把密码写进进程参数
arcthis read secret.7z data.txt --password-file ./password.txt

# 按指定顺序访问分卷压缩包
arcthis inspect dataset.7z.001 --volume dataset.7z.002 --volume dataset.7z.003 --json

# 创建或刷新持久化文件列表缓存
arcthis index dataset.7z --json

# 先写同目录临时文件，再安全保存成一个普通文件
arcthis extract archive.zip README.md --output ./README.md

# 安全解压整个压缩包
arcthis extract archive.tar.gz

# 在执行会删除文件的批量操作前，先查看结构化计划
arcthis extract-all ./downloads --recursive --delete-source --dry-run --json

# 创建、重新打开、验证并保存新压缩包
arcthis pack ./project --output project.tar.gz

# 验证每个可读文件
arcthis verify project.tar.gz --json

# 先查看转换计划，再经安全临时写入和验证后保存
arcthis convert project.zip --output project.tar.zst --dry-run --json
```

完整的目标目录规则、资源限制、JSON 格式、退出码和命令说明见 [START.zh-CN.md](./START.zh-CN.md)。

## 安全模型

完整解压会先完成文件信息提前检查和路径检查，拒绝链接和特殊文件，执行声明大小、真实写入、时间和压缩比限制，然后只写入同文件系统的临时目录；所有文件都成功后才会保存为最终结果。已有目标默认拒绝覆盖，`--overwrite`、`--skip-existing` 与 `--rename` 是互斥的显式处理方式。

创建压缩包会先写入同目录临时文件，完成收尾后，再通过统一的压缩包接口重新打开并逐文件验证，最后才保存到目标路径。输出位置位于源目录内部、或源与目标指向同一位置时都会被拒绝；`--delete-source` 只在保存后、且确认删除源文件不会删掉目标文件时才执行。dry-run 永远不会写入或删除。

访问嵌套压缩包时，会把选中的内层文件解码到受资源上限约束的只读内存中，不会创建临时中间文件。转换会先把通过安全检查的文件写到系统临时目录，再执行 pack、重新打开验证并保存。持久化文件列表缓存被视为不可信缓存，并通过源文件大小与修改时间自动失效。精确定义和已知限制见 [docs/SECURITY.md](./docs/SECURITY.md)。

## Agent 接口

- 成功的结构化输出使用 `schema_version: "1"`。
- 结果写 stdout；警告与错误写 stderr。
- 使用 `--json` 时，程序错误以 JSON 写到 stderr。
- `read` 始终输出原始字节，并拒绝 `--json`。
- BrokenPipe 按接收方正常提前结束处理。
- 文件信息使用 `path_encoding` 明确标识非 UTF-8 名称。
- 文件信息包含稳定的压缩包内顺序和基于扩展名的轻量文件类型推断。
- 非 TTY 和 JSON 输出不包含 ANSI 颜色装饰。

公开的 JSON 格式与错误模型见 [docs/CLI.md](./docs/CLI.md)。

## 平台支持

v0.5 面向 macOS 与 Linux 开发和测试，并配置了覆盖 all-features/MCP 的双平台 GitHub Actions。架构避免把 Unix-only 类型暴露为对外接口，但 Windows 暂时不是正式的自动化测试目标。

## 开发入口

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --release --locked --all-features
```

重要文档：

- [START.zh-CN.md](./START.zh-CN.md) — 完整使用指南
- [INDEX.md](./INDEX.md) — 简洁代码库地图
- [docs/PRODUCT.md](./docs/PRODUCT.md) — 产品定义与非目标
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — 压缩包接口与底层实现设计
- [docs/SECURITY.md](./docs/SECURITY.md) — 解压与生命周期安全
- [AGENTS.md](./AGENTS.md) — 代码 Agent 的长期仓库约束
- [CONTRIBUTING.md](./CONTRIBUTING.md) — 贡献流程

## Roadmap、贡献与许可证

分阶段格式与能力计划见 [ROADMAP.md](./ROADMAP.md)，六阶段 MCP/remote/service/binding 计划详见 [docs/V0.5-INTEGRATIONS-PLAN.md](./docs/V0.5-INTEGRATIONS-PLAN.md)。贡献必须保持统一命令含义、直接读写、JSON 格式兼容性和保守的解压默认值，修改公共行为前请阅读 [CONTRIBUTING.md](./CONTRIBUTING.md)。

`arcthis` 使用 [MIT License](./LICENSE)。
发布时需要关注的 native 与 Rust dependency 许可摘要见 [THIRD_PARTY_LICENSES.md](./THIRD_PARTY_LICENSES.md)。
