# Arcthis 发布流程

本文记录公开版本的固定发布步骤。正式发布不可覆盖；任何一项未满足都应停止，不应通过 `--allow-dirty`、跳过验证或手工修改生成产物绕过。

## 发布渠道

- crates.io：`cargo install arcthis --locked`
- GitHub Release：macOS arm64、macOS x86_64、Linux x86_64 压缩包、SHA-256 与安装脚本
- Homebrew：`brew install mkynyd/tap/arcthis`
- npm：`npm install -g arcthis`
- pnpm：`pnpm add -g arcthis`

所有公开二进制都必须包含默认启用的 MCP 命令。Windows 与 Linux arm64 在有独立测试前不进入发布列表。

## 一次性准备

1. `mkynyd/homebrew-tap` 必须存在并保持公开。
2. 在 `mkynyd/arcthis` 配置 `HOMEBREW_TAP_TOKEN`：使用只允许写入 `mkynyd/homebrew-tap` contents 的细粒度 GitHub token，不复用个人通用 OAuth token。
3. 在 npm 创建 `arcthis` 公共包的发布凭据，并在仓库配置 `NPM_TOKEN`。首次发布完成后优先迁移到 npm Trusted Publishing；迁移完成前定期轮换 token。
4. 首次 crates.io 发布需要人工登录并使用只允许发布 `arcthis` 的 token。首次发布完成后，在 crates.io 为本仓库发布 workflow 配置 Trusted Publishing。
5. 不把 token 写入仓库、日志、命令历史、release notes 或构建产物。

## 每次发布前

```sh
git status --short --branch
git fetch origin
test "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)"

cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --release --locked --all-features

cargo package --list
cargo publish --dry-run --locked
dist generate --check
dist plan
```

同时确认：

- `Cargo.toml`、`Cargo.lock`、`CHANGELOG.md`、网站与文档版本一致；
- GitHub 的 `CI` 和 `Release` pull request 检查全部通过；
- 发布计划只包含已支持的三个 target；
- macOS release 二进制的 `otool -L` 只列出系统库；Linux x86_64 的 `ldd` 只列出 glibc 基础库与已确认的 `libnettle.so.8`、`liblzma.so.5`、`liblz4.so.1`、`libz.so.1`、`libxml2.so.2`，并在受支持发行版上实际运行；
- crates.io 与 npm 上的目标版本尚不存在；
- GitHub Release、Tag 和 Homebrew Formula 中不存在同版本产物。

## 候选版本验证

正式版之前先把 `Cargo.toml` 与 `Cargo.lock` 中的项目版本改为 `0.5.0-rc.1` 之类的候选版本，提交并等待 main 的 CI 全绿，再创建完全一致的 `v0.5.0-rc.1` Tag。cargo-dist 不接受 Tag 与包版本不一致。默认配置不会把候选版本发布到 npm 或覆盖 Homebrew Formula。

候选产物必须实际完成：

- 下载并核对 SHA-256；
- 在对应平台执行 `arcthis --version` 和 `arcthis mcp --help`；
- 完成 pack、list、read、verify smoke test；
- 检查 macOS `otool -L` 与 Linux `ldd`；
- 检查生成的 npm 包与 Homebrew Formula 指向同一 GitHub Release。

候选验证结束后，把项目版本改回正式的 `0.5.0`，再进入正式发布步骤；不得移动、删除或复用已经推送的候选 Tag。

## 正式发布

1. 把 `CHANGELOG.md` 的 `Unreleased` 改为发布日期，提交并等待 main 的 CI 全绿。
2. 在干净且与 `origin/main` 一致的 main 上再次运行全部发布前命令。
3. 首次发布人工执行 `cargo publish --locked`，确认 crates.io 页面与 `cargo install arcthis --locked` 可用。
4. 创建签名 Tag `v0.5.0` 并只推送该 Tag；cargo-dist workflow 随后创建 GitHub Release，并发布 npm 包与 Homebrew Formula。
5. 等待 Release workflow 全部成功，不以 GitHub Release 已出现代替 npm/Homebrew Job 成功。
6. 在干净环境分别执行 Cargo、Homebrew、npm、pnpm 安装，并重复版本、MCP 和归档 smoke test。
7. 更新中英文 README 与网站，把“尚未发布”替换为已经真实验证的安装命令，再部署网站。

如果任何渠道失败，保留已发布事实并记录，不删除或重写同版本。修复后发布新的补丁版本；crates.io 只能在确有必要时 yank，不能覆盖。
