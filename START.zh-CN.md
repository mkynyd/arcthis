# arcthis 使用指南

[English](./START.md)

本文只说明 v0.1 已真实实现的 CLI。产品目标和规划命令见 [docs/PRODUCT.md](./docs/PRODUCT.md) 与 [ROADMAP.md](./ROADMAP.md)。

## 构建与安装

仓库固定 Rust 1.98.0。

```sh
cargo build --release --locked
./target/release/arcthis --version
cargo install --path . --locked
```

## 核心工作流

先访问，再提取：

```text
inspect -> list/tree -> stat -> read -> 确实需要时再 extract
```

典型 Agent 流程：

```sh
arcthis inspect dataset.tar.gz --json
arcthis tree dataset.tar.gz --json
arcthis stat dataset.tar.gz train/data.csv --json
arcthis read dataset.tar.gz train/data.csv | head -n 20
```

输入格式根据内容识别。当前 ZIP build 支持 Stored 与 Deflate 内容访问。TAR/TAR.GZ 是顺序格式，读取靠后的 entry 可能需要扫描之前的数据；`inspect` 会以 `random_access: false` 和 `sequential_access` warning 明确报告。

## 命令形式与全局选项

```text
arcthis <command> <archive> [entry] [options]
```

- `--json`：对支持结构化结果的命令输出 schema-versioned JSON。
- `--no-color`：禁用终端颜色；v0.1 当前不依赖颜色表达信息。
- `-h`/`--help`：查看帮助。
- `-V`/`--version`：查看版本。

`NO_COLOR`、非 TTY 与 JSON 输出都不包含 ANSI decoration。

## `inspect`：了解访问成本与风险

```sh
arcthis inspect archive.tar.gz
arcthis inspect archive.tar.gz --json
```

`inspect` 枚举 metadata，但不会为了探测而读取每个文件的内容。结果包含格式、compression、entry 数、声明大小、近似压缩比、capabilities 和 warnings。

重要 warning code：

- `sequential_access`：目标读取可能需要从头顺序扫描。
- `encrypted_entries_unsupported`：存在加密内容，但当前不支持。
- `non_regular_entries`：存在 extraction 会拒绝的 link/special entry。
- `duplicate_entry_paths`：命名访问有歧义，提取会拒绝。
- `unsafe_entry_paths`：至少一个路径无法通过安全提取校验。
- `default_extraction_limits_exceeded`：声明 metadata 超过默认资源限制。

Warning 只用于决策；真正的 extraction 路径会独立强制执行安全检查。

## `list`：列举 entry

```sh
arcthis list archive.zip
arcthis list archive.zip --json
```

Human mode 输出 `KIND`、`SIZE`、`PATH` 三列；JSON 保留 archive 顺序和重复 entry。

Entry 对象包含：

- `path` 与 `path_encoding`（`utf8` 或 `escaped_bytes`）；
- `kind`：`file`、`directory`、`symlink`、`hardlink`、`other`；
- `size` 与可选 `compressed_size`；
- 可选 `modified_time`；
- `encrypted`、`executable`、可选 `symlink_target` 和 `crc32`。

无效 UTF-8 字节使用 `%XX` 显示，并标记 `path_encoding: "escaped_bytes"`。list/read 可用显示值定位；v0.1 extraction 会拒绝物化，因为它无法无歧义恢复原始文件系统名称。

## `tree`：查看逻辑文件树

```sh
arcthis tree source.tar
arcthis tree source.tar --json
```

Human mode 使用 tree characters。JSON node 包含 `name`、逻辑 `path`、`kind`、可选源 `entry` 和 `children`。隐式目录的 `entry` 为 `null`，重复文件 leaf 会保留。

## `stat`：查看一个命名 entry

```sh
arcthis stat archive.zip README.md
arcthis stat archive.zip README.md --json
```

路径必须与 `list` 显示一致。不存在时返回 `entry_not_found`；同名 entry 超过一个时返回 `collision`，不会静默选择。

## `read`：流式读取一个 entry

```sh
arcthis read archive.zip README.md
arcthis read source.tar.gz src/lib.rs | rg unsafe
arcthis read media.zip video.mp4 | ffprobe -i pipe:0
```

`read` 是核心内容 primitive，只向 stdout 写原始 entry bytes，诊断写 stderr。它不会把 bytes 包装进 JSON，因此 `--json` 会返回 `unsupported_operation`。

只支持普通文件；目录、link 和 special entry 会被拒绝。BrokenPipe 视为消费者正常提前结束，所以 `arcthis read ... | head` 可以自然工作。

对 TAR/TAR.GZ，v0.1 会先确认目标路径唯一，再做顺序内容扫描，可能解码流不止一次，但不会把整个 archive 物化到磁盘。

## `extract`：安全物化内容

### 提取全部内容

```sh
arcthis extract archive.zip
arcthis extract archive.tar.gz --output ./restored
```

显式 `--output` 是完整提取根目录。不提供时：

- 如果所有 entry 都位于一个真实顶层目录下，直接提交该目录；
- 否则按完整 archive 后缀生成目录名，例如 `backup.tar.gz` → `backup/`；
- 目标必须不存在。

```text
archive: project/README.md, project/src/lib.rs
result:  ./project/README.md, ./project/src/lib.rs

archive: README.md, src/lib.rs
input:   bundle.tar.gz
result:  ./bundle/README.md, ./bundle/src/lib.rs
```

### 提取单个普通文件

单 entry 提取必须显式给出输出文件：

```sh
arcthis extract archive.zip README.md --output ./README.md
```

命令写入同目录临时文件，flush/sync 后执行 no-clobber commit。目录和 link entry 不支持单文件提取。

### 资源限制

```sh
arcthis extract archive.zip \
  --max-entries 50000 \
  --max-total-size 8589934592 \
  --max-entry-size 2147483648
```

参数使用原始 bytes/数量：

| 选项 | 默认值 |
| --- | ---: |
| `--max-entries` | 100,000 |
| `--max-total-size` | 16 GiB |
| `--max-entry-size` | 4 GiB |

声明 metadata 在 staging 前检查，实际 bytes 在 streaming 时计数。超限返回 `resource_limit`，不会提交目标。

### 提取安全

v0.1 拒绝绝对路径、`.`/`..`、反斜杠、Windows drive/UNC prefix、NUL、重复路径、大小写不敏感冲突、file-as-parent、超长路径、无效 UTF-8 名称、symlink、hardlink 与 special file。内容写入目标同文件系统的 staging，全部成功后才 rename commit。

v0.1 没有 `--overwrite`、`--skip-existing` 或 `--rename`。精确定义见 [docs/SECURITY.md](./docs/SECURITY.md)。

## `pack`：创建并验证 archive

```sh
arcthis pack ./project --output project.zip
arcthis pack ./project --output project.tar
arcthis pack ./project --output project.tar.gz --json
```

输出后缀选择 ZIP、TAR 或 TAR.GZ。目录打包会把源目录本身作为顶层 entry，保留空目录和普通文件；ZIP 使用 Deflate。

Symlink/special file 会被拒绝，目标必须不存在。生命周期为：

```text
扫描 source -> 写同目录临时文件 -> finalize -> sync -> reopen -> verify -> no-clobber commit
```

源文件永远不会被删除；`--delete-source` 是规划能力。

## `verify`：验证可读 archive 数据

```sh
arcthis verify archive.zip
arcthis verify archive.tar.gz --json
```

ZIP 会打开并流式读取每个 entry，使 CRC 校验真正执行。TAR/TAR.GZ 会解析每个 header、流式读取每个 entry，并消费 Gzip integrity trailer。它验证结构和 codec 完整性，不提供密码学真实性或内容安全保证。

## Machine output

所有成功的结构化文档都以 `"schema_version": "1"` 开始。基于 archive 的结果包含：

```json
{
  "archive": {
    "path": "dataset.zip",
    "path_lossy": false,
    "format": "zip"
  }
}
```

命令专属字段分别为 `entries`、`tree`、`entry`、inspect 字段、`extraction`、`verification` 或 `pack`。`pack` 的输入是文件系统 source，因此没有 input archive envelope。

启用 `--json` 后，runtime error 作为单个 JSON 文档写入 stderr，stdout 保持为空：

```json
{
  "schema_version": "1",
  "error": {
    "code": "entry_not_found",
    "message": "archive entry not found: missing.txt",
    "details": { "entry": "missing.txt" }
  }
}
```

完整 schema 见 [docs/CLI.md](./docs/CLI.md)。

## 退出码

| 退出码 | 分类 |
| ---: | --- |
| 0 | 成功，包括 BrokenPipe consumer stop |
| 1 | 通用 I/O error |
| 2 | clap 命令语法/用法错误 |
| 3 | `unsupported_format` |
| 4 | `invalid_archive` / `corrupted_archive` |
| 5 | `entry_not_found` |
| 6 | `permission_denied` |
| 7 | `unsafe_path` |
| 8 | `resource_limit` |
| 9 | `collision` |
| 10 | `unsupported_operation`、password 分类 |
| 11 | `verification_failed` |
| 12 | `partial_failure` |

## 常见问题

### `.gz` 文件返回 `unsupported_format`

v0.1 支持 TAR.GZ，不支持任意单文件 Gzip stream；解码后必须能验证 TAR 结构。

### ZIP 可以 list，但无法 read/verify

当前 build 支持 Stored/Deflate entry。其他 ZIP compression method 会返回 `unsupported_operation`。

### Extraction 返回 `collision`

可能是目标已存在、archive 内有重复/大小写冲突路径，或 entry 与父路径冲突。v0.1 不覆盖已有内容，请选择新的 `--output` 或先 inspect。

### Extraction 拒绝 link

这是保守默认。恢复 link 需要额外的 target 与顺序安全规则，只有具备回归测试后才会加入。

### `find`、`grep`、`hash`、`extract-all`、`convert`、`--dry-run`、`--delete-source` 在哪里？

它们都属于 Roadmap。当前优先用 `read` 与现有工具组合，不要依赖未文档化占位功能。
