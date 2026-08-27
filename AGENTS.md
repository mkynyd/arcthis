# arcthis Agent 协作约束

## 项目定位

`arcthis` 是面向 AI Agent 与人类的统一压缩文件访问层，英文定位为“An agent-native CLI for accessing and manipulating compressed files.”。项目的核心不是复制一个 zip/unzip 工具，而是让 archive 像可访问的文件树一样被发现、查询、流式读取、验证和按需物化。

## 长期原则

- Access before extraction.
- Stream before materialize.
- Structured by default for agents.
- Safe before convenient.
- Unified semantics over format quirks.
- Compose instead of reimplement.
- Library first, CLI first-class.
- Do not extract what you do not need.

## 目录与模块约定

- `src/lib.rs` 只暴露可复用的 library interface，不应依赖 CLI 表现层。
- `src/archive/` 管理 archive 抽象、格式检测与 backend adapter。上层命令不得重复 ZIP/TAR-specific 分支。
- `src/cli.rs` 和 `src/output.rs` 只负责 CLI 语法、输出和进程退出语义。
- `src/security.rs` 是 entry path 和 extraction 安全规则的单一真相源。
- `tests/` 使用动态 fixture 和 CLI 集成测试验证公开行为。
- `docs/` 保存产品、架构、CLI 契约与安全设计；未实现能力必须标记为 planned。

## 工程质量

- 使用当前 stable Rust，优先同步 streaming I/O；没有经证实的需求不引入 async runtime。
- 优先小 interface、深 module 和真实 backend seam；不为只有一个实现的假设变化创建 trait hierarchy。
- 生产路径不得用 `unwrap()`/`expect()` 处理不可信 archive，测试代码除外。
- 禁止自行实现压缩算法；具体 crate/native backend 必须隔离在 backend 层。
- 保持依赖克制，新增主要依赖时在代码或文档中能说明用途。
- 完成任务前必须运行 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all-features` 和与改动相称的真实 CLI smoke test。

## CLI 与 JSON 兼容性

- 命令语法优先保持 `arcthis <command> <archive> [entry] [options]`，禁止加入格式专属一级命令。
- stdout 仅输出结果数据，stderr 仅输出诊断、警告和进度。`read` 的 stdout 是原始 entry bytes。
- 非 TTY、`--json`、`NO_COLOR` 或 `--no-color` 模式不得输出 ANSI decoration。BrokenPipe 必须按 Unix 惯例正常退出。
- JSON 是公共 interface。已发布 schema 的字段名、类型、含义和 `schema_version` 不得静默改变；破坏性改动必须写 RFC/ADR 并升级 schema version。
- Machine mode 错误写入 stderr，使用稳定 error code 和合理的 process exit code，不暴露 crate 或 Rust backtrace。

## 安全约束

- extraction 必须经过统一 path sanitizer，拒绝 `..`、绝对路径、Windows drive/UNC prefix、NUL 和可能越界的 link。
- 默认不恢复 archive symlink/hardlink/special file。任何放宽都必须显式选项、安全设计和回归测试。
- 写入优先 staging + commit；失败不得留下伪装成成功的最终产物。
- `--delete-source` 只能在 perform → finalize → verify → commit 完成后执行；任何错误、中断或验证失败都必须保留 source。
- 资源限制必须在真正写入/解码路径执行，不能只依赖 `inspect` 警告。

## 文档同步规则

完成任何影响项目行为、结构、CLI、功能、依赖、安全策略、构建、测试或使用方式的代码任务后，必须主动检查并同步项目说明文档。固定收尾流程为：

1. 对照真实代码、`arcthis --help`、subcommand help 和测试检查文档。
2. 更新受影响的 `AGENTS.md`、`README.md`/`README.zh-CN.md`、`INDEX.md`、`START.md`/`START.zh-CN.md`、`docs/` 和 `ROADMAP.md`。
3. 规划中的功能明确标记 planned/roadmap，不得写成已实现。
4. 向根目录 `log.md` 追加简体中文记录，不覆盖历史；`log.md` 必须保持在 `.gitignore` 中。

## 禁止事项

- 不得将默认完整解压作为 `read`/`stat`/`inspect` 的隐藏实现。
- 不得为 ZIP、TAR 等复制上层 command 逻辑。
- 不得在没有安全契约和测试的情况下引入 overwrite、source deletion、symlink restoration 或 nested recursion。
- 不得把终端文本包装为 JSON；必须输出真实结构化 schema。
- 不得将 target、cache、临时产物或 `log.md` 加入版本库。
