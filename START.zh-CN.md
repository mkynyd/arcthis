# arcthis 使用指南

[English](./START.md)

本文只说明截至 v0.4 已真实实现的 CLI。产品目标和后续规划见 [docs/PRODUCT.md](./docs/PRODUCT.md) 与 [ROADMAP.md](./ROADMAP.md)。

## 构建与安装

仓库固定 Rust 1.98.0。

RAR support 使用静态链接的 libarchive。源码构建前需要安装 native dependencies：macOS 使用 Homebrew `libarchive libb2 bzip2 lz4 xz zstd`；Debian/Ubuntu 使用 `libarchive-dev libb2-dev libbz2-dev liblz4-dev liblzma-dev libxml2-dev libzstd-dev zlib1g-dev`。

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

输入格式根据内容识别。当前支持 ZIP、7z、RAR/RAR5、TAR、使用 Gzip/Bzip2/XZ/Zstandard 的压缩 TAR，以及这四种 codec 的单 payload stream。压缩 TAR、单 stream、RAR 和 solid 7z 可能需要扫描或解码前面的数据；`inspect` 会报告已实现的访问成本。

## 命令形式与全局选项

```text
arcthis <command> <archive> [entry] [options]
```

- `--json`：对支持结构化结果的命令输出 schema-versioned JSON。
- `--no-color`：禁用终端颜色；当前输出不依赖颜色表达信息。
- 可重复的 `--within <entry>`：让访问/查询命令显式进入 nested archive。
- `--max-nested-entry-size <bytes>`：限制每层 inner archive 的解码大小，默认 256 MiB。
- `--password-file <path>`：从文件读取密码并移除末尾 CR/LF，避免把 secret 暴露为进程参数。
- 可重复的 `--volume <path>`：按给定顺序把 byte-stream 分卷接在首卷之后。
- `--index-directory <path>`：覆盖持久化 metadata index 使用的平台 cache 根目录。
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
- `encrypted_entries`：存在加密内容，content operation 需要正确密码。
- `non_regular_entries`：存在 extraction 会拒绝的 link/special entry。
- `duplicate_entry_paths`：命名访问有歧义，提取会拒绝。
- `unsafe_entry_paths`：至少一个路径无法通过安全提取校验。
- `default_extraction_limits_exceeded`：声明 metadata 超过默认资源限制。
- `high_compression_ratio`：至少一个 entry 声明的展开比超过 1000:1。
- `single_stream_metadata_scan`：确定单 stream 隐式 payload 大小时执行了顺序解码。
- `multipart_byte_stream`：当前 source 由显式排序的 byte-stream 分卷组成。
- `rar_metadata_limited`：当前 RAR backend 可能无法提供 solid、encryption 或 compressed-size metadata。

Warning 只用于决策；真正的 extraction 路径会独立强制执行安全检查。

## `list`：列举 entry

```sh
arcthis list archive.zip
arcthis list archive.zip --json
```

Human mode 输出 `KIND`、`SIZE`、`PATH` 三列；JSON 保留 archive 顺序和重复 entry。

Entry 对象包含：

- `archive_index`，保持源 archive 顺序；
- `path` 与 `path_encoding`（`utf8` 或 `escaped_bytes`）；
- `kind`：`file`、`directory`、`symlink`、`hardlink`、`other`；
- `size` 与可选 `compressed_size`；
- 可选 `modified_time`；
- `encrypted`、`executable`、可选 `symlink_target` 和 `crc32`。
- 可选 `mime_guess`，只根据路径扩展名推断，不读取内容。

无效 UTF-8 字节使用 `%XX` 显示，并标记 `path_encoding: "escaped_bytes"`。可以用显示值定位；extraction 会拒绝物化，因为它无法无歧义恢复原始文件系统名称。

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

压缩 TAR 与 solid 7z 可能解码目标之前的数据。单 stream 会暴露一个由文件名派生的 entry，例如 `report.txt.gz` → `report.txt`。这些操作都不会把完整 archive 物化到磁盘。

## `find`：按路径筛选 entry

```sh
arcthis find dataset.7z --glob '**/*.json'
arcthis find dataset.7z --glob '**/*.json' --json
```

`find` 匹配完整 normalized archive path，返回完整 entry metadata，且不会解码 entry 内容。

## `grep`：有界流式内容搜索

```sh
arcthis grep source.tar.gz TODO --glob '**/*.rs'
arcthis grep papers.zip transformer --glob '**/*.md' --json
```

Pattern 是 literal byte sequence，不是正则表达式。超过 `--max-entry-size` 的文件会跳过（默认 16 MiB），达到 `--max-matches` 后停止收集（默认 10,000），单行最多保留 1 MiB。前 8 KiB 出现 NUL 会判定为 binary；默认跳过，只有显式 `--binary` 才扫描。JSON 会报告扫描、跳过、byte 和截断计数。

## `hash`：计算一个流式 entry digest

```sh
arcthis hash models.zip model.bin
arcthis hash models.zip model.bin --algorithm sha512 --json
```

默认 SHA-256，也支持 SHA-512。Entry bytes 直接流入 digest，不写入磁盘。

## 使用 `--password-file` 访问加密 archive

```sh
arcthis inspect private.zip --json
arcthis read private.zip report.txt --password-file ./password.txt
arcthis verify private.7z --password-file ./password.txt --json
```

密码不会作为普通 CLI value 接收。密码文件按 bytes 读取，并移除末尾 CR/LF，适合保存单行 secret。ZIP 支持 ZipCrypto/AES 解密，7z 支持 AES 解密。当前创建的 archive 不加密，因此 `pack --password-file` 会明确返回 `unsupported_operation`，不会静默忽略。缺少密码和密码错误分别使用稳定分类 `password_required` 与 `wrong_password`。

RAR 使用同一接口，但实际加密支持取决于 libarchive 和具体 RAR variant；不支持时会返回明确 backend error。详见 [docs/RAR.md](./docs/RAR.md)。

## 使用 `--volume` 访问 multipart byte stream

```sh
arcthis inspect dataset.7z.001 \
  --volume dataset.7z.002 \
  --volume dataset.7z.003 \
  --json
arcthis read dataset.7z.001 data.csv \
  --volume dataset.7z.002 \
  --volume dataset.7z.003
```

位置参数中的 archive 是首卷，后续每个 `--volume` 都按提供顺序连接，组合后的 seekable byte stream 再进入正常 content detection。所有路径必须唯一且存在。这适用于按 byte boundary 切分的 archive，例如 split 7z；它不会把 format-native RAR volume set 假装成简单拼接。`inspect` 会报告 `multipart` 与 `volume_count`。Multipart extraction/conversion 禁止 `--delete-source`，因为多源删除还无法满足单源生命周期保证。详见 [RFC 0002](./docs/RFC-0002-MULTIPART-SOURCES.md)。

## `index`：管理持久化 metadata cache

```sh
arcthis index dataset.7z --json
arcthis index dataset.7z --refresh --json
arcthis index dataset.7z --delete --dry-run --json
arcthis index dataset.7z --delete --json
```

`index` 把枚举得到的 entry metadata 写入平台 cache，后续打开会自动复用有效 index。Cache key 使用 canonical source path，源文件大小和纳秒修改时间变化会使其失效。`--refresh` 强制重新枚举并事务式替换，`--delete` 只删除当前 archive 的 index，`--dry-run` 仅报告 `would_create`、`would_refresh`、`would_reuse` 或 `would_delete`。`--index-directory` 可用于隔离或迁移 cache。

Cache 文件是不可信优化数据：JSON 损坏、schema/format 不匹配或过期时会被忽略。当前 index 只保存 metadata，不保存解码内容或 TAR seek point。

## 使用 `--within` 访问 nested archive

```sh
arcthis tree backup.zip --within project.tar.gz --json
arcthis read backup.zip README.md --within project.tar.gz
arcthis grep bundle.zip TODO --within layer.7z --within source.tar.zst
```

每个 `--within` 都命名当前层的一个 archive entry。它会被解码成受限制的 immutable memory source，再走正常 content detection 打开，不创建具名临时 inner 文件。最大深度为 8，每层默认上限 256 MiB。v0.4 不支持 nested extraction 或 conversion。详见 [RFC 0001](./docs/RFC-0001-NESTED-ARCHIVES.md)。

## `extract`：安全物化内容

### 提取全部内容

```sh
arcthis extract archive.zip
arcthis extract archive.tar.gz --output ./restored
```

显式 `--output` 是完整提取根目录。不提供时：

- 如果所有 entry 都位于一个真实顶层目录下，直接提交该目录；
- 否则按完整 archive 后缀生成目录名，例如 `backup.tar.gz` → `backup/`；
- 目标默认必须不存在，除非显式选择 collision policy。

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

命令写入同目录临时文件，flush/sync 后按选定 policy 提交。目录和 link entry 不支持单文件提取。

### 资源限制

```sh
arcthis extract archive.zip \
  --max-entries 50000 \
  --max-total-size 8589934592 \
  --max-entry-size 2147483648 \
  --max-compression-ratio 1000 \
  --max-entry-duration-seconds 300
```

参数使用原始 bytes/数量：

| 选项 | 默认值 |
| --- | ---: |
| `--max-entries` | 100,000 |
| `--max-total-size` | 16 GiB |
| `--max-entry-size` | 4 GiB |
| `--max-compression-ratio` | 默认不启用，需显式指定 |
| `--max-entry-duration-seconds` | 默认不启用，需显式指定 |

声明 metadata 在 staging 前检查，实际 bytes 在 streaming 时计数。超限返回 `resource_limit`，不会提交目标。

### 计划、冲突与源文件生命周期

`--dry-run` 会解析真实目标、冲突状态、warning、预计大小和删除意图，但不会写入或删除任何内容：

```sh
arcthis extract archive.tar.zst --dry-run --delete-source --json
```

默认 collision policy 拒绝已有目标。需要时只能选择一个替代策略：

- `--overwrite`：事务式替换目标；提交失败时恢复原路径；
- `--skip-existing`：报告成功跳过，且绝不删除 source；
- `--rename`：选择第一个可用的编号 sibling，例如 `bundle.1`。

`--delete-source` 只会在 extraction 完整 staging、验证整个 source archive 并成功 commit 后执行。即使只提取一个 entry，删除 source 前也会验证未选中的 entry，因此可能需要额外解码。计划、解码、验证、写入或提交中的任何失败都会保留 source archive。可能导致 destination 被删除的 source/destination alias 或祖先/后代重叠会在写入前以 `collision` 拒绝。

### 提取安全

Extraction 拒绝绝对路径、`.`/`..`、反斜杠、Windows drive/UNC prefix、NUL、重复路径、大小写不敏感冲突、file-as-parent、超长路径、无效 UTF-8 名称、symlink、hardlink 与 special file。内容写入目标同文件系统的 staging，所有计划 entry 成功后才提交。精确定义见 [docs/SECURITY.md](./docs/SECURITY.md)。

## `extract-all`：批量处理目录中的 archive

```sh
arcthis extract-all ./downloads --dry-run --json
arcthis extract-all ./downloads --recursive --workers 4
arcthis extract-all ./downloads --recursive --delete-source
```

发现过程按内容而不是后缀识别支持的 archive。默认只扫描指定目录；`--recursive` 递归文件系统目录，但不会进入 archive 内继续发现 nested archive。`--workers` 将独立 archive 的并发数限制在 1 到 64。

命令会先为全部 archive 生成计划，并在写入前拒绝批次内目标冲突。每个 archive 使用与 `extract` 相同的资源限制和 collision policy。混合结果返回 `partial_failure`；JSON 按路径稳定排序并报告每个已发现 archive 的结果。

## `pack`：创建并验证 archive

```sh
arcthis pack ./project --output project.zip
arcthis pack ./project --output project.7z
arcthis pack ./project --output project.tar.zst --json
arcthis pack ./report.txt --output report.txt.xz
```

输出后缀可选择 ZIP、7z、TAR、TAR.GZ/TGZ、TAR.BZ2/TBZ2、TAR.XZ/TXZ、TAR.ZST/TZST，或单 Gzip/Bzip2/XZ/Zstandard stream。单 stream 的 source 必须是普通文件。目录打包会把源目录本身作为顶层 entry，保留空目录和普通文件；ZIP 使用 Deflate。

当前 `pack` 只创建未加密输出。传入 `--password-file`、`--volume` 或 `--within` 会返回 `unsupported_operation`，不会静默忽略。

Symlink/special file 会被拒绝。默认拒绝目标冲突；`--overwrite`、`--skip-existing`、`--rename` 与 extraction 含义一致。`--dry-run` 返回解析后的计划。生命周期为：

```text
扫描 source -> 写同目录临时文件 -> finalize -> sync -> reopen -> verify -> commit -> 可选删除 source
```

启用 `--delete-source` 后，也只有已提交 archive 能够重新打开并验证成功时才删除 source；此前任何失败都会保留它。

输出路径必须位于目录 source 之外，也不能与文件 source 解析为同一路径，防止 archive 把自身输出打包进去、替换掉或在后续删除 source 时一并删除。

## `convert`：按验证生命周期转换 archive

```sh
arcthis convert backup.zip --output backup.tar.zst --dry-run --json
arcthis convert backup.zip --output backup.7z --delete-source
arcthis convert data.7z.001 --volume data.7z.002 --output data.tar.zst
```

输出 suffix 使用与 `pack` 相同的可写格式；RAR 创建明确不支持。v0.4 conversion 使用 `staged_materialization`：先通过统一 backend 打开 source，强制执行 extraction path 和 resource limits，把通过校验的普通 entry 物化进系统临时目录，再创建临时 target、重新打开验证、按 collision policy 提交，最后才可选删除单个 source archive。

```text
open -> validate -> stage entries -> pack/finalize -> reopen/verify -> commit -> 可选删除 source
```

`--dry-run` 会执行 source 枚举、路径/资源校验、target shape 校验和 collision resolution，然后输出 typed plan；不会创建 target 或 staging directory。转换保持原 archive entry path，不会把临时目录名带进 target。单 stream target（`.gz`、`.bz2`、`.xz`、`.zst`）只接受一个 root-level 普通 entry，且不能有其他 entry。

`convert` 支持 `--overwrite`、`--skip-existing`、`--rename`、extraction limits 和 `--password-file`。复合后缀重命名会保持格式，例如 `backup.tar.zst` 变为 `backup.1.tar.zst`。Nested conversion 和 multipart `--delete-source` 会被拒绝。Target commit 之前的任何失败都保留 source。

## `verify`：验证可读 archive 数据

```sh
arcthis verify archive.zip
arcthis verify archive.tar.gz --json
```

ZIP、7z 与 RAR 会通过 backend 完整性检查流式读取每个可读 entry。TAR 和全部 compressed TAR variant 会解析每个 header，并通过 codec trailer 流式读取 entry。单 stream 会完整解码。它验证结构和 codec 完整性，不提供密码学真实性或内容安全保证。

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

命令专属字段包括 `entries`、`tree`、`entry`、inspect 字段、`find`、`grep`、`hash`、`extraction`、`verification`、`pack`、`index` 与 `convert`。Dry-run 使用 `operation` 和 typed `plan`，batch execution 使用 `result`。`pack`、`index` 与 `convert` 使用各自的 operation envelope，不伪装成普通 entry query。

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

### 单 compressed stream 的 entry 名与预期不同

匹配的 codec suffix 会被移除，例如 `report.txt.gz` 变成 `report.txt`。如果文件名与实际 codec 不匹配，则使用 `.out` payload 名，避免悄悄伪造 suffix 语义。

### ZIP 可以 list，但无法 read/verify

Rust ZIP backend 支持 dependency build 启用的方法；超出支持集的方法返回 `unsupported_operation`，不会调用外部工具兜底。

### Extraction 返回 `collision`

可能是目标已存在、archive 内有重复/大小写冲突路径、entry 与父路径冲突，或两个 `extract-all` 计划解析到同一目标。可以保持默认拒绝、换用新的 `--output`，或显式选择一种 collision policy。

### Extraction 拒绝 link

这是保守默认。恢复 link 需要额外的 target 与顺序安全规则，只有具备回归测试后才会加入。

### RAR 能读取，但 metadata 不完整

libarchive adapter 不会暴露所有 RAR 属性。`inspect` 会输出 `rar_metadata_limited`；真正的 read/extract/verify 结果才是行为依据。当前不支持 RAR 创建和 format-native RAR multi-volume traversal。

### 使用 `--volume` 后 split archive 仍然失败

确认位置参数是第一个 byte segment，并且后续 `--volume` 按完整顺序提供。该能力组合 byte-stream split，不会重新解释 format-native volume protocol。

### Recursive cross-archive search 在哪里？

Recursive cross-archive search 仍是 v0.4 之后的 Roadmap 能力。已知 inner archive 请使用显式 `--within` 链，不要依赖未文档化 locator syntax。
