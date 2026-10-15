# 探索报告迭代验收

更新：2026-10-16。依据《arcthis 0.5.1 黑盒探索报告》（2026-10-12），对照当前代码、真实 CLI 和 MCP stdio 测试逐项处理。外部报告是问题线索；其修法建议不直接作为执行指令。

当前源码为 **0.6.0-rc.1，JSON schema 2，尚未发布**。公开安装渠道仍是 0.5.1/schema 1。迁移决策见 [ADR 0002](./ADR-0002-EXPLORATION-CONTRACT.md)。源码迭代已完成；用户随后授权维护网站、提交并发布，候选验证与正式发布继续遵循 RELEASING.md。

## 逐项结果

| 报告 # | 结果 | 实现／验收 |
| --- | --- | --- |
| 1 grep 平方开销 | 已修复 | 一份元数据快照和 backend 批量 visitor；ZIP 复用中央目录，TAR/压缩 TAR、7z、RAR 复用一次内容遍历。保留过滤、顺序、二进制／大小跳过、匹配／结果／实际扫描字节预算。7z 按真实实例身份处理空文件和重名，不依赖 native 的全局停止行为。 |
| 2 solid 7z 压缩比 | 已修复 | 共享块成员 `compressed_size: null`；单文件可用大小保留数值，空文件真实零与未知分开。比例按 block packed/unpacked 总量检查，规划前执行；随机 solid 不误拒，高压缩 block 仍拒绝，实际写入大小上限继续独立生效。 |
| 3 TAR `./` | 已修复 | 统一 sanitizer 丢弃 `.` 与冗余内部 `/`，拒绝每个 `..`、绝对路径、drive/UNC、NUL 和反斜杠。root 目录标记不计入子项，规范化后查重复／大小写／父子冲突；extract、convert、extract-all 回归通过。 |
| 4 小文件抽取开销 | 已处理 | 默认保留逐文件 fsync；显式 `--no-sync` 适用于 extract、extract-all、convert。仍使用 staging、失败回滚、验证后提交和提交后删除 source；代价是降低掉电时的耐久性。 |
| 5 ZIP 静默去重 | 已修复 | 枚举原始中央目录，保留全部实例及 `archive_index`。为重复成员构造有界单条目目录视图，现有 ZIP 库继续负责解密／解码／CRC。命名访问拒绝歧义；stat/read/hash/extract 支持 `--entry-index`。普通、AES、Deflate 和超过 4 GiB 偏移的稀疏 ZIP64 fixture 通过。 |
| 6 batch 错误隔离 | 已修复 | `extract-all --continue-on-error` 将 discovery/planning 错误保留为逐档结果；dry-run 也返回错误清单，不写入。源和目标重叠的计划被隔离，独立好档继续；排序确定，汇总正确，部分失败 exit 12。默认仍先完成全部规划。 |
| 7 单流命名 | 已修复 | Gzip 优先 FNAME，保留原始名字字节及可用 mtime；pack 写入 FNAME/mtime。无 FNAME 的 split source 去掉常见 `.aa`／数字／`.partN` 卷后缀，再按压缩后缀推导，无法恢复原名时使用 `.out`。stat/read/hash 可省略 entry，前提是只有一个普通文件或显式指定 index。 |
| 8 `%XX` 歧义 | 已修复 | `path_raw_hex` 保留 adapter 暴露的名字字节，index 为实例身份；同显示名的命名访问明确 collision，显式 index 能区分。ZIP/TAR 的原始字节名与字面 `%XX` 名经显式解码后，可分别抽取出正确内容；统一 writer 以 index 接收计划，回归先失败后通过。 |
| 9 非 UTF-8 物化 | 已处理 | 默认仍拒绝未解码名字；显式 `--encoding utf8|cp437|gbk|shift-jis` 严格解码，不使用有损替换，解码后的名称继续经过 sanitizer 和碰撞检查。GBK 中文真实抽取和读取通过。 |
| 10 index 无消费／清理 | 推论纠正并补齐 | 原实现已经消费缓存，撤销 source 权限不能证明相反结论。新增 `archive.index_used`、`index --list`、`index --prune`、年龄阈值和 dry-run；只删除索引子目录中符合哈希命名的 stale 文件。增加 Unix ctime/dev/inode 指纹；同长度／恢复 mtime 的替换不再复用旧名字。文档读取上限在实际读取时执行。 |
| 11 read 部分输出 | 已提供选项 | 默认继续 raw streaming，必须检查退出码。`--verify-first` 先完整解码并校验到匿名临时文件，再输出；默认最多 4 GiB，可显式降低，受服务限额约束。CRC、密码或存储失败时 stdout 无内容。 |
| 12 read + JSON | 已修复分类 | 返回 `invalid_argument` / exit 2，先于 archive/password I/O；read 的 stdout 契约保持原始字节。 |
| 13 时间戳 | 已统一语义 | `modified_time` 仅为带偏移的绝对 RFC3339 或 null。ZIP DOS 无时区时间放入 `modified_time_local`，不凭空附加 Z。 |
| 14 null / 0 | 已修复 | 不可用／共享块压缩大小、未知时间／模式／CRC 使用 null；真实的零字节文件和可用的零 CRC 不被伪造为未知。 |
| 15 目录元数据 | 已修复 | 目录 executable 和 CRC 为 null，不再把可遍历目录当可执行文件。 |
| 16 CLI/MCP 契约 | 已统一 | 只读 JSON 共用 application result 序列化结构，包含版本与 archive reference；inspect 扁平化、tree 不多输出 entries。错误共用 typed envelope，MCP 同时给 text/structuredContent，参数错误不透出 serde 原文。outputSchema 接受成功或结构化错误并保留 `$defs` 引用。JSON 对象键顺序不作为接口语义。 |
| 17 错误分类 | 已修复 | 非法 glob 为 invalid_argument/2；抽取 CRC 与 verify 对齐 verification_failed/11；within 非归档为 not_an_archive/3；目录 source 为明确参数错误；不支持的输出格式文案与输入区分。既有密码相关 exit 10 保持粗分类，精确分支使用 JSON error.code。 |
| 18 plan / result 路径 | 已统一 | filesystem reference 与生命周期 source/destination 使用规范化绝对路径；stdin／nested 保留显式虚拟 reference。相对路径 dry-run/execute 对账回归通过。 |
| 19 no-color 空开关 | 无需行为变更 | 当前纯文本满足无 ANSI 的禁用契约；保留兼容开关，不为该观察新增颜色或删除参数。 |
| 20 Debug / 单复数 | 已修复 | status、index action、path encoding 用显式文本；成功结果按 entry/entries 计数，不再输出 Some(Completed)、WouldCreate。 |
| 21 计数 | 已修复 | index delete 读取可用缓存计数；root 目录和自动剥离目录不再导致计划数与实际数不同。 |
| 22 pack 权限 | 已修复 | output 使用 0666 & umask，临时文件和最终产物一致；与同进程普通新建文件的权限对照回归通过。 |
| 23 stdin | 已实现 | inspect/list/tree/stat/read/find/grep/hash/verify 接受 `-`，压缩源以有界内存支持重开／seek，默认 64 MiB，`--max-stdin-bytes` 可调整，`--stdin-name` 提供名字／空 TAR 后缀。不能与 volume 组合，不进入破坏性命令。 |
| 24 MCP EOF | 已修复 | 握手前干净 EOF 和正常会话 EOF 均 exit 0；非法初始协议消息仍是错误。无需从“客户端未回收进程”推导服务端泄漏，也未增加无证据需求的 idle timeout。 |
| 25 MCP 死参数 | 推论纠正并收敛 | output-root/source-deletion 原本用于六个受控写工具，继续保留并验证门控；`mcp --json` 明确参数错误。只读工具可按请求传 password_file，必须位于授权输入根内且不超过 4096 字节，不接受明文密码值。 |
| 26 inspect capabilities | 已实现 | 人类输出打印 solid、比例和 can_create/extract/verify/seek、streaming_read。 |
| 27 删除回执 | 已实现 | extract、pack、convert 与 batch 项成功删除 source 时打印 source deleted；skip 不打印删除。 |
| 28 错误对象文案 | 已修复 | 输入／输出格式、directory source 和 within 非归档分别描述实际对象，保留结构化错误细节。 |

附加可用性建议 `pack --strip-root` 已实现。`--skip-unsafe`、有损名称物化和 link restoration 没有加入；安全拒绝由逐档继续模式隔离。MCP 加密密码文件目前用于九个只读工具，受控写工具的请求不接受该字段。SFX 输入仍不属于已支持的格式检测入口；backend 的 ZIP64 directory view 不等于新增 SFX CLI 能力。

## 验证与性能

本次回归使用动态 fixture 和独立 CLI／JSON-RPC 子进程。源码同时验证默认 MCP 构建和 `--no-default-features`；没有运行正式发布或宣称三平台 release 已独立验证。

第一轮 ZIP release 基线：200/400/800 文件三次中位数为 0.121310/0.445761/1.700703 秒，400→800 为 3.82 倍。第一轮优化后 20,001 文件为 0.229570 秒。完整迭代后，以相同 ZIP fixture 的三次中位数复测如下；报告对 19 分钟的外推不作为本次基线。

| release 场景 | 实测秒数 |
| --- | ---: |
| ZIP grep 400 / 800 / 1600 / 3200 文件 | 0.010605 / 0.016678 / 0.028932 / 0.055327 |
| ZIP grep 20,001 文件 | 0.307632 |
| TAR.GZ grep 400 / 800 / 1600 / 3200 文件 | 0.005661 / 0.007389 / 0.011233 / 0.018143 |
| solid 7z grep 1000 文件 | 0.013642 |
| 抽取 1000 空文件：默认 fsync / 显式 no-sync（各一次） | 5.062707 / 0.118510 |

最终通过 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all-features` **104 项**、`cargo test --no-default-features` **84 项**和 release build。真实 release smoke 用独立 JSON Schema validator 校验 **15 个 MCP output schema 和 19 份实际成功／错误结果**，完成 **11 种可创建格式**的 pack→grep 往返，并核对帮助、stdout/stderr、退出码与 ASCII；RAR 另由真实 CLI fixture 覆盖 list/read/grep/extract/verify。

证据在本机 `archive_test/report-all/`（`tests-final.log`、`tests-no-default.log`、`clippy-final.log`、`release-build.log`、`acceptance-smoke.json`、`performance.json`、`identity-red.log`／`identity-green.log`）。HANDOFF.md、log.md 和本机 fixture／证据都被 Git 忽略；原始外部报告不复制进仓库。核心迭代验收与后续发布分开记录；候选三平台验证和渠道发布状态以发布记录为准。
