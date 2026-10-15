# arcthis 使用指南

[English](./START.md)

本文按尚未发布的 **0.6.0-rc.1 源码（JSON schema 2）** 编写。公开 0.5.1 二进制仍使用 schema 1，不包含下文新增选项；使用前请源码构建。迁移见 [ADR 0002](./docs/ADR-0002-EXPLORATION-CONTRACT.md)，改动与验证见[迭代验收](./docs/EXPLORATION-ITERATION-PLAN.md)。

v0.5.1 增加写入路径重叠检查、文件树层数限制、MCP 并发与响应大小限制，以及 `grep --max-result-bytes`。这段是公开 0.5.1 的历史说明；当前源码迁移见上方 ADR。

格式识别以内容为主。空的压缩 TAR 只有全零块，需要保留 `.tar.gz`、`.tar.bz2`、`.tar.xz`、`.tar.zst` 或对应缩写后缀。其他以零字节开头的压缩文件保持单文件读取。

## 0.6.0-rc.1 源码新增选项

```sh
arcthis read bundle.zip --entry-index 3 --verify-first
arcthis stat legacy.zip --encoding gbk --entry-index 0 --verbose
cat bundle.zip | arcthis list - --json
arcthis extract files.tar --output out --no-sync
arcthis extract-all incoming --continue-on-error --json
arcthis pack folder --strip-root --output contents.zip
arcthis index --list --json
arcthis index --prune --dry-run --max-age-seconds 2592000 --json
```

index 只选择 within 链最终层的实例。stat/read/hash 只有一个普通文件或指定 index 时可省略 entry。名称编码严格使用 utf8/cp437/gbk/shift-jis，原始字节保留在 path_raw_hex，解码后继续执行路径与冲突检查。modified_time 只放绝对 RFC3339 或 null；ZIP 无时区时间放 modified_time_local。目录 executable/CRC 与未知元数据为 null。

verify-first 使用匿名临时文件，实际字节默认最多 4 GiB；默认 raw streaming 可能在失败前输出部分字节，必须检查退出码。stdin 为支持 seek/重开而缓冲压缩源，默认 max-stdin-bytes 为 64 MiB，stdin-name 可提供名字或空 TAR 后缀；仅限只读，不能与 volume 组合。no-sync 以掉电耐久性换速度，保留 staging 与生命周期检查。continue-on-error 在 plan.errors／逐档结果中记录失败，返回 partial_failure/12，仍隔离 source/destination 重叠。pack 产物遵守 umask，gzip 写 FNAME/mtime，改外层名字后仍能读取原名。

index 清理仅作用于索引缓存中符合哈希命名的文档，默认年龄 30 天，dry-run 不删除，其他文件不动。Unix ctime/dev/inode 补充 size/mtime，能检测常见的同大小／恢复 mtime 替换；archive.index_used 可观察缓存消费，指纹不证明内容真实性。read --json 返回 invalid_argument/2。生命周期 source/destination 在计划与结果中使用同样的规范化绝对路径。

MCP 只读请求可引用授权输入根内的 password_file（最多 4096 字节），不接受明文密码值。正常 EOF 退出 0，JSON-RPC 服务拒绝 --json。CLI/MCP 的成功与错误信封均使用 schema 2。完整格式见 [CLI 契约](./docs/CLI.md)和[迁移 ADR](./docs/ADR-0002-EXPLORATION-CONTRACT.md)。

## 安装

任选一种公开安装方式。所有方式默认都包含 MCP：

```sh
cargo install arcthis --locked
brew install mkynyd/tap/arcthis
npm install -g arcthis
pnpm add -g arcthis
```

pnpm 11 可能暂缓安装发布不足 24 小时的新包。首发当天如果出现 `ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION`，可在命令末尾临时加 `--config.minimumReleaseAge=0`。

Apple Silicon macOS、Intel macOS 与 Linux x86_64 的编译成品可从 [v0.5.1 GitHub Release](https://github.com/mkynyd/arcthis/releases/tag/v0.5.1) 下载。

### 从源码构建

仓库固定 Rust 1.98.0。

RAR 支持使用静态链接的 libarchive。源码构建前需要安装本机开发依赖：macOS 使用 Homebrew `libarchive libb2 bzip2 lz4 xz zstd`；Debian/Ubuntu 使用 `libarchive-dev libb2-dev libbz2-dev liblz4-dev liblzma-dev libxml2-dev libzstd-dev zlib1g-dev`。

```sh
cargo build --release --locked
./target/release/arcthis --version
cargo install --path . --locked
```

## 本地 MCP 入口

默认构建启用 MCP，因此 Cargo、Homebrew、npm 与 pnpm 安装得到的命令保持一致。启动时必须显式授权至少一个输入根目录：

```sh
./target/release/arcthis mcp --allow-root ./archives
```

只使用代码库且不需要 MCP 时，仍可通过 `cargo build --no-default-features` 关闭默认功能。

stdio 传输固定协议版本 `2025-06-18`，stdout 只允许 JSON-RPC。只读工具包括 `archive_inspect`、`archive_list`、`archive_tree`、`archive_stat`、`archive_read`、`archive_find`、`archive_grep`、`archive_hash` 与 `archive_verify`。所有请求都有有限的文件数（`--max-entries`，100,000）、解码字节数（`--max-decoded-bytes`，1 GiB）和结果数（`--max-results`，10,000）上限；`archive_read` 还强制提供 `offset`（偏移量）与 `length`（长度），默认单次最多返回 1 MiB（`--max-read-window`）。

Extract、pack 与 convert 分别使用独立的 `_plan`（计划）和 `_execute`（执行）工具。未配置 `--allow-output-root` 时，这 6 个工具不会出现在工具列表中。执行必须回传完全一致的 SHA-256 计划摘要；计划后 source 或 destination 发生变化都会拒绝执行。删除 source 只有在服务启用 `--allow-source-deletion` 且请求明确要求删除时才允许。MCP 不接受明文密码值；只读请求可引用授权根内的 password_file。

```sh
./target/release/arcthis mcp \
  --allow-root ./archives \
  --allow-output-root ./outputs
```

授权、JSON 格式、取消与二进制传输规则见 [RFC 0003](./docs/RFC-0003-MCP-INTEGRATION.md)。

MCP 默认同时运行最多 4 个工具，单次工具响应最多 16 MiB，包含结构化 JSON 与文本副本。可用 `--max-concurrent-requests`（1–64）和 `--max-response-bytes` 调整；后者只读模式至少 1024 字节，启用输出根时至少 131072 字节。并发超额或响应超限返回 `resource_limit`。并发繁忙时可等已有调用完成后重试，大查询可缩小范围或使用读取窗口。grep 保留的匹配结果最多占响应预算的四分之一，跨文件的实际扫描字节共用 `--max-decoded-bytes`。

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

输入格式根据内容识别。当前支持 ZIP、7z、RAR/RAR5、TAR、使用 Gzip/Bzip2/XZ/Zstandard 的压缩 TAR，以及这四种压缩算法的单文件格式。压缩 TAR、单文件格式、RAR 和 solid 7z 可能需要扫描或解码前面的数据；`inspect` 会报告实际的访问成本。

## 当前源码的根帮助

运行 `arcthis`、`arcthis -h` 或 `arcthis --help` 可查看 ASCII Logo、简介与根命令列表。无参数运行保留缺少命令的行为：帮助写入 stderr，退出码为 2；显式帮助写入 stdout，退出码为 0。子命令帮助、版本信息、压缩包结果和 MCP 响应不含 Logo。标志只使用 ASCII 字符，不含 ANSI 颜色。这是当前源码新增的表现，已发布的 v0.5.1 二进制尚不包含。

## 命令形式与全局选项

```text
arcthis <command> <archive> [entry] [options]
```

- `--json`：对支持结构化结果的命令输出带版本号的 JSON。
- `--no-color`：禁用终端颜色；当前输出不依赖颜色表达信息。
- 可重复的 `--within <entry>`：让访问/查询命令显式进入嵌套的压缩包。
- `--max-nested-entry-size <bytes>`：限制每层内层压缩包的解码大小，默认 256 MiB。
- `--password-file <path>`：从文件读取密码并移除末尾 CR/LF，避免把密码暴露为进程参数。
- 可重复的 `--volume <path>`：按给定顺序把分卷文件接在首卷之后。
- `--index-directory <path>`：覆盖持久化文件列表缓存使用的平台缓存根目录。
- `-h`/`--help`：查看帮助。
- `-V`/`--version`：查看版本。

`NO_COLOR`、非 TTY 与 JSON 输出都不包含 ANSI 颜色装饰。当前构建完全不输出 ANSI 装饰，因此 `--no-color` 与 `NO_COLOR` 只是为兼容性而接受，现阶段不改变任何输出。

## `inspect`：了解访问成本与风险

```sh
arcthis inspect archive.tar.gz
arcthis inspect archive.tar.gz --json
```

`inspect` 列出文件信息，但不会为了探测而读取每个文件的内容。普通模式输出格式、compression（压缩方式）、文件数、声明大小、访问成本、分卷状态和 warnings（警告）；`--json` 另外提供压缩比、`encrypted`/`solid` 标记、`capabilities` 对象和完整警告列表。

重要的警告代码（warning code）：

- `sequential_access`：目标读取可能需要从头顺序扫描。
- `duplicate_entry_paths`：包含 ZIP 在内的重复显示路径需要显式 index 选择，整体解压会拒绝。
- `encrypted_entries`：存在加密内容，读取内容需要正确密码。
- `non_regular_entries`：存在解压时会拒绝的链接/特殊条目。
- `unsafe_entry_paths`：至少一个路径无法通过安全解压校验。
- `default_extraction_limits_exceeded`：声明文件信息超过默认资源限制。
- `high_compression_ratio`：至少一个文件声明的展开比超过 1000:1。
- `single_stream_metadata_scan`：确定单文件格式的隐式内容大小时执行了顺序解码。
- `multipart_byte_stream`：当前 source 由显式排序的分卷文件组成。
- `rar_metadata_limited`：当前 RAR 底层实现可能无法提供 solid、encryption 或 compressed-size 文件信息。

警告只用于决策；真正的解压路径会独立强制执行安全检查。

## `list`：列出文件

```sh
arcthis list archive.zip
arcthis list archive.zip --json
```

普通模式输出 `KIND`、`SIZE`、`PATH` 三列；JSON 保留全部实例，包括 ZIP 的重复中央目录名称。使用 `archive_index` / `--entry-index` 选择实例，按名称访问遇到歧义时返回 collision。

每个文件对象包含：

- `archive_index`，保持源压缩包内顺序；
- `path`、`path_raw_hex` 与 `path_encoding`（utf8/escaped_bytes/cp437/gbk/shift_jis）；
- `kind`：`file`、`directory`、`symlink`、`hardlink`、`other`；
- `size` 与可选 `compressed_size`；
- 可选 `modified_time`；
- `encrypted`、`executable`、可选 `symlink_target` 和 `crc32`。
- 可选 `mime_guess`，只根据路径扩展名推断，不读取内容。

无效 UTF-8 字节使用 `%XX` 显示，并标记 `path_encoding: "escaped_bytes"`。可以用显示值定位；解压时会拒绝写出，因为它无法无歧义恢复原始文件系统名称。

## `tree`：查看逻辑文件树

```sh
arcthis tree source.tar
arcthis tree source.tar --json
```

普通模式使用树状字符。JSON node 包含 `name`、逻辑 `path`、`kind`、可选源文件 `entry` 和 `children`。隐式目录的 `entry` 为 `null`，重复文件叶节点在底层实现暴露时保留。树路径最多 4096 字节、256 个非空组件；超限会返回 `resource_limit`，不会继续构造无界深度的结果。

## `stat`：查看一个指定文件

```sh
arcthis stat archive.zip README.md
arcthis stat archive.zip README.md --json
```

路径须与 `list` 一致。不存在时返回 entry_not_found，同显示名重复时返回 collision。`--entry-index N` 精确选择实例；stat/read/hash 在只有一个普通文件或指定 index 时可省略 entry。配合 `--within` 时，index 只作用于最终内层压缩包。

## `read`：直接读取一个文件

```sh
arcthis read archive.zip README.md
arcthis read source.tar.gz src/lib.rs | rg unsafe
arcthis read media.zip video.mp4 | ffprobe -i pipe:0
```

`read` 是核心的内容读取命令，只向 stdout 写原始文件字节，诊断写 stderr。它不会把字节包装进 JSON，因此 `--json` 会返回 `invalid_argument`（退出 2）。

只支持普通文件；目录、链接和特殊条目会被拒绝。BrokenPipe 视为接收方正常提前结束，所以 `arcthis read ... | head` 可以自然工作。

压缩 TAR 与 solid 7z 可能解码目标之前的数据。单文件格式会暴露一个由文件名派生的文件，例如 `report.txt.gz` → `report.txt`。这些操作都不会把完整压缩包写到磁盘。

## `find`：按路径筛选文件

```sh
arcthis find dataset.7z --glob '**/*.json'
arcthis find dataset.7z --glob '**/*.json' --json
```

`find` 匹配完整的规范化压缩包内路径，返回完整文件信息，且不会解码文件内容。

## `grep`：有上限的内容搜索

```sh
arcthis grep source.tar.gz TODO --glob '**/*.rs'
arcthis grep papers.zip transformer --glob '**/*.md' --json
```

匹配内容是一段原始字节序列，不是正则表达式。超过 `--max-entry-size` 的文件会跳过（默认 16 MiB），达到 `--max-matches` 后停止收集（默认 10,000），单行最多保留 1 MiB。前 8 KiB 出现 NUL 会判定为二进制文件；默认跳过，只有显式 `--binary` 才扫描。JSON 会报告扫描、跳过、字节和截断计数。

ZIP 搜索复用中央目录；TAR 系列、7z 和 RAR 搜索复用一次内容遍历。通配符与声明大小过滤先于条目内容复制。顺序格式访问选中条目仍可能解码前面的字节；visitor 保留扫描／结果预算与实例身份，不把文件解压到磁盘。

`--max-result-bytes` 限制跨文件累计的匹配行 JSON 大小，默认 16 MiB，计入路径、转义和 UTF-8 替换字节。完整匹配行放不下时会省略该行，并设置 `matches_truncated: true`；设为 0 则不保留匹配。如需更多输出，可以显式提高此上限。

## `hash`：计算一个文件的校验值

```sh
arcthis hash models.zip model.bin
arcthis hash models.zip model.bin --algorithm sha512 --json
```

默认 SHA-256，也支持 SHA-512。文件字节直接参与校验值计算，不写入磁盘。

## 使用 `--password-file` 访问加密压缩包

```sh
arcthis inspect private.zip --json
arcthis read private.zip report.txt --password-file ./password.txt
arcthis verify private.7z --password-file ./password.txt --json
```

密码不会作为普通命令行参数接收。密码文件按字节读取，并移除末尾 CR/LF，适合保存单行密码。ZIP 支持 ZipCrypto/AES 解密，7z 支持 AES 解密。当前创建的压缩包不加密，因此 `pack --password-file` 会明确返回 `unsupported_operation`，不会静默忽略。缺少密码和密码错误分别使用稳定分类 `password_required` 与 `wrong_password`。

RAR 使用同一接口，但实际加密支持取决于 libarchive 和具体 RAR 版本；不支持时会返回明确的底层实现错误。详见 [docs/RAR.md](./docs/RAR.md)。

## 使用 `--volume` 访问分卷文件

```sh
arcthis inspect dataset.7z.001 \
  --volume dataset.7z.002 \
  --volume dataset.7z.003 \
  --json
arcthis read dataset.7z.001 data.csv \
  --volume dataset.7z.002 \
  --volume dataset.7z.003
```

位置参数中的压缩包是首卷，后续每个 `--volume` 都按提供顺序连接，组合后的可定位字节流再进入正常的格式识别。所有路径必须唯一且存在。这适用于按字节边界切分的压缩包，例如 split 7z；它不会把 RAR 的原生分卷格式假装成简单拼接。`inspect` 会报告 `multipart` 与 `volume_count`。分卷的解压/转换禁止 `--delete-source`，因为同时删除多个源文件还无法满足单源生命周期保证。详见 [RFC 0002](./docs/RFC-0002-MULTIPART-SOURCES.md)。

## `index`：管理持久化文件列表缓存

```sh
arcthis index dataset.7z --json
arcthis index dataset.7z --refresh --json
arcthis index dataset.7z --delete --dry-run --json
arcthis index dataset.7z --delete --json
```

`index` 把列出得到的文件信息写入平台缓存，后续打开会自动复用有效缓存。缓存的标识使用规范化源路径，源文件大小和纳秒修改时间变化会使其失效。`--refresh` 强制重新列出并一次性替换，`--delete` 只删除当前压缩包的缓存，`--dry-run` 仅报告 `would_create`、`would_refresh`、`would_reuse` 或 `would_delete`（`--delete` 找不到已有索引时报告 `missing`）。`--index-directory` 可用于隔离或迁移缓存。

缓存文件是不可信的优化数据：JSON 损坏、格式不匹配或过期时会被忽略。当前缓存只保存文件信息，不保存解码内容或 TAR 定位点。

## 使用 `--within` 访问嵌套的压缩包

```sh
arcthis tree backup.zip --within project.tar.gz --json
arcthis read backup.zip README.md --within project.tar.gz
arcthis grep bundle.zip TODO --within layer.7z --within source.tar.zst
```

每个 `--within` 都命名当前层的一个压缩包文件。它会被解码成受限制的只读内存数据，再走正常格式识别打开，不创建临时中间文件。最大深度为 8，每层默认上限 256 MiB。v0.5 不支持嵌套解压或转换。详见 [RFC 0001](./docs/RFC-0001-NESTED-ARCHIVES.md)。

## `extract`：安全解压内容

### 解压全部内容

```sh
arcthis extract archive.zip
arcthis extract archive.tar.gz --output ./restored
```

显式 `--output` 是完整解压根目录。不提供时：

- 如果所有文件都位于一个真实顶层目录下，直接保存该目录；
- 否则按完整压缩包后缀生成目录名，例如 `backup.tar.gz` → `backup/`；
- 目标默认必须不存在，除非显式选择冲突处理方式。

```text
archive: project/README.md, project/src/lib.rs
result:  ./project/README.md, ./project/src/lib.rs

archive: README.md, src/lib.rs
input:   bundle.tar.gz
result:  ./bundle/README.md, ./bundle/src/lib.rs
```

### 解压单个普通文件

单个文件解压必须显式给出输出文件：

```sh
arcthis extract archive.zip README.md --output ./README.md
```

命令写入同目录临时文件，完成写入和同步后按选定方式保存。目录和链接条目不支持单文件解压。

### 资源限制

```sh
arcthis extract archive.zip \
  --max-entries 50000 \
  --max-total-size 8589934592 \
  --max-entry-size 2147483648 \
  --max-compression-ratio 1000 \
  --max-entry-duration-seconds 300
```

参数使用原始字节/数量：

| 选项 | 默认值 |
| --- | ---: |
| `--max-entries` | 100,000 |
| `--max-total-size` | 16 GiB |
| `--max-entry-size` | 4 GiB |
| `--max-compression-ratio` | 默认不启用，需显式指定 |
| `--max-entry-duration-seconds` | 默认不启用，需显式指定 |

声明文件信息在写临时文件前检查，实际字节在读取时计数。超限返回 `resource_limit`，不会保存目标。

### 计划、冲突与源文件生命周期

`--dry-run` 会解析真实目标、冲突状态、警告、预计大小和删除意图，但不会写入或删除任何内容：

```sh
arcthis extract archive.tar.zst --dry-run --delete-source --json
```

默认冲突处理方式拒绝已有目标。需要时只能选择一个替代方式：

- `--overwrite`：一次性替换目标；保存失败时恢复原路径；
- `--skip-existing`：报告成功跳过，且绝不删除 source；
- `--rename`：选择第一个可用的同目录编号名称，例如 `bundle.1`。

`--delete-source` 只会在解压完整写入临时文件、验证整个源压缩包并成功保存后执行。即使只解压一个文件，删除 source 前也会验证未选中的文件，因此可能需要额外解码。计划、解码、验证、写入或保存中的任何失败都会保留源压缩包。无论是否请求删除 source，源/目标指向同一位置或存在任意祖先/后代重叠，都会在写入前以 `collision` 拒绝。

### 解压安全

解压先丢弃 `.` 与冗余内部 `/`，再拒绝每个 `..`、绝对路径、drive/UNC、NUL、反斜杠、规范化后的重复／大小写／父子冲突、超长路径、未解码名称及 links/special files。ZIP 重复实例也会拒绝。默认逐文件 fsync；显式 `--no-sync` 可以降低掉电耐久性以换取速度，staging、失败回滚和验证后删除 source 的规则保留。

## `extract-all`：批量处理目录中的压缩包

```sh
arcthis extract-all ./downloads --dry-run --json
arcthis extract-all ./downloads --recursive --workers 4
arcthis extract-all ./downloads --recursive --delete-source
```

发现过程按内容而不是后缀识别支持的压缩包。默认只扫描指定目录；`--recursive` 递归文件系统目录，但不会进入压缩包内继续发现嵌套压缩包。`--workers` 将独立压缩包的并发数限制在 1 到 64。

命令会先为全部压缩包生成计划，并在写入前拒绝批次内相同或嵌套的目标冲突，也拒绝目标覆盖另一个已发现压缩包源。每个压缩包使用与 `extract` 相同的资源限制和冲突处理方式。混合结果返回 `partial_failure`；JSON 按路径稳定排序并报告每个已发现压缩包的结果。

## `pack`：创建并验证压缩包

```sh
arcthis pack ./project --output project.zip
arcthis pack ./project --output project.7z
arcthis pack ./project --output project.tar.zst --json
arcthis pack ./report.txt --output report.txt.xz
```

输出后缀可选择 ZIP、7z、TAR、TAR.GZ/TGZ、TAR.BZ2/TBZ2、TAR.XZ/TXZ、TAR.ZST/TZST，或单个 Gzip/Bzip2/XZ/Zstandard 压缩文件。单个压缩文件的 source 必须是普通文件。目录打包会把源目录本身作为顶层文件条目，保留空目录和普通文件；ZIP 使用 Deflate。

当前 `pack` 只创建未加密输出。传入 `--password-file`、`--volume` 或 `--within` 会返回 `unsupported_operation`，不会静默忽略。

符号链接/特殊文件会被拒绝。默认拒绝目标冲突；`--overwrite`、`--skip-existing`、`--rename` 与解压含义一致。`--dry-run` 返回解析后的计划。处理流程为：

```text
扫描 source -> 写同目录临时文件 -> 收尾 -> 同步 -> 重新打开 -> 验证 -> 保存 -> 可选删除 source
```

启用 `--delete-source` 后，也只有已保存的压缩包能够重新打开并验证成功时才删除 source；此前任何失败都会保留它。

输出路径必须位于目录 source 之外，也不能与文件 source 解析为同一路径，防止压缩包把自身输出打包进去、替换掉或在后续删除 source 时一并删除。

## `convert`：按验证流程转换压缩包格式

```sh
arcthis convert backup.zip --output backup.tar.zst --dry-run --json
arcthis convert backup.zip --output backup.7z --delete-source
arcthis convert data.7z.001 --volume data.7z.002 --output data.tar.zst
```

输出后缀使用与 `pack` 相同的可写格式；RAR 创建明确不支持。v0.4 的转换使用 `staged_materialization`（先写临时文件再保存）策略：先通过统一的底层实现打开 source，强制执行解压路径和资源限制，把通过校验的普通文件写到系统临时目录，再创建临时 target、重新打开验证、按冲突处理方式保存，最后才可选删除单个源压缩包。

```text
打开 -> 校验 -> 把文件写到临时目录 -> 打包/收尾 -> 重新打开/验证 -> 保存 -> 可选删除 source
```

`--dry-run` 会执行 source 列出、路径/资源校验、目标格式校验和冲突处理，然后输出结构化计划；不会创建目标或临时目录。转换保持原压缩包内文件路径，不会把临时目录名带进目标。单个压缩文件目标（`.gz`、`.bz2`、`.xz`、`.zst`）只接受一个根目录下的普通文件，且不能有其他文件。

`convert` 支持 `--overwrite`、`--skip-existing`、`--rename`、解压限制和 `--password-file`。复合后缀重命名会保持格式，例如 `backup.tar.zst` 变为 `backup.1.tar.zst`。嵌套转换和分卷 `--delete-source` 会被拒绝。目标保存之前的任何失败都保留 source。

## `verify`：验证可读压缩包数据

```sh
arcthis verify archive.zip
arcthis verify archive.tar.gz --json
```

ZIP、7z 与 RAR 会通过底层完整性检查逐个读取每个可读文件。TAR 和全部压缩 TAR 变体会解析每个文件头，并通过压缩格式尾部逐个读取文件。单个压缩文件会完整解码。它验证结构和压缩格式完整性，不提供密码学真实性或内容安全保证。

## 机器可读输出

所有成功的结构化文档都以 `"schema_version": "2"` 开始。基于压缩包的结果包含：

```json
{
  "archive": {
    "path": "dataset.zip",
    "path_lossy": false,
    "format": "zip"
  }
}
```

命令专属字段包括 `entries`、`tree`、`entry`、inspect 字段、`find`、`grep`、`hash`、`extraction`、`verification`、`pack`、`index` 与 `convert`。Dry-run 使用 `operation` 和结构化 `plan`，批量执行使用 `result`。`pack`、`index` 与 `convert` 使用各自的操作外层结构，不伪装成普通的文件查询。

启用 `--json` 后，运行时错误作为单个 JSON 文档写入 stderr，stdout 保持为空：

```json
{
  "schema_version": "2",
  "error": {
    "code": "entry_not_found",
    "message": "archive entry not found: missing.txt",
    "details": { "entry": "missing.txt" }
  }
}
```

完整的 JSON 格式见 [docs/CLI.md](./docs/CLI.md)。

## 退出码

| 退出码 | 分类 |
| ---: | --- |
| 0 | 成功，包括 BrokenPipe consumer stop |
| 1 | 通用 I/O error |
| 2 | clap 命令语法/用法错误 |
| 3 | `unsupported_format`, `not_an_archive` |
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

### 单个压缩文件的文件名与预期不同

匹配的压缩后缀会被移除，例如 `report.txt.gz` 变成 `report.txt`。如果文件名与实际压缩格式不匹配，则使用 `.out` 作为内容名，避免悄悄伪造后缀含义。

### ZIP 可以 list，但无法 read/verify

Rust ZIP 底层实现支持依赖构建启用的压缩方法；超出支持集的方法返回 `unsupported_operation`，不会调用外部工具兜底。

### 解压返回 `collision`

可能是目标已存在、压缩包内有重复/大小写冲突路径、文件与父路径冲突，或两个 `extract-all` 计划解析到同一目标。可以保持默认拒绝、换用新的 `--output`，或显式选择一种冲突处理方式。

### 解压拒绝链接

这是保守默认。恢复链接需要额外的目标与顺序安全规则，只有具备回归测试后才会加入。

### RAR 能读取，但文件信息不完整

libarchive 适配层不会暴露所有 RAR 属性。`inspect` 会输出 `rar_metadata_limited`；真正的 read/extract/verify 结果才是行为依据。当前不支持 RAR 创建和 RAR 原生分卷访问。

### 使用 `--volume` 后分卷压缩包仍然失败

确认位置参数是第一个字节片段，并且后续 `--volume` 按完整顺序提供。该能力组合字节流切分，不会重新解释原生分卷协议。

### 跨压缩包递归搜索在哪里？

跨压缩包递归搜索仍是 v0.4 之后的 Roadmap 能力。已知内层压缩包请使用显式 `--within` 链，不要依赖未文档化的路径写法。
