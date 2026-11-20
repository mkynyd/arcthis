# Arcthis 0.7.0 发布验证

正式版于 2026-11-20 发布，使用 JSON schema 2。Cargo、Homebrew、npm、pnpm 和 Shell 安装脚本均已安装验证，默认包含 MCP。[GitHub Release](https://github.com/mkynyd/arcthis/releases/tag/v0.7.0) · [crates.io](https://crates.io/crates/arcthis/0.7.0) · [npm](https://www.npmjs.com/package/arcthis/v/0.7.0)

## 源码与构建

| 阶段 | 源码及校验 |
| --- | --- |
| 候选版 | 签名 Tag `v0.7.0-rc.1` 指向 `81ec1fbc228bf611f5659ee68c607cef482ab7c4)。[PR #9](https://github.com/mkynyd/arcthis/pull/9) · [构建](https://github.com/mkynyd/arcthis/actions/runs/44656825166) · [下载包验收](https://github.com/mkynyd/arcthis/actions/runs/44658943419) |
| 正式版 | 签名 Tag `v0.7.0` 指向 `5c27847e31009de375e9344cfbab9a8ad1d8c2af`，与 crates.io 源码包的提交一致。[PR #10](https://github.com/mkynyd/arcthis/pull/10) · [main CI](https://github.com/mkynyd/arcthis/actions/runs/44660114806) · [Tag CI](https://github.com/mkynyd/arcthis/actions/runs/44660915591) · [发布构建](https://github.com/mkynyd/arcthis/actions/runs/44660915593) |

正式源码包 SHA-256 为 `fcdd24f85ed2338394eafa75595a2920778eda34e43ea1db6e9d38c2e7a71651`，与 crates.io 索引匹配。包内 VCS 信息指向干净源码。

## 下载包与安装

| 平台 | 真实验证环境 | 压缩包 SHA-256 |
| --- | --- | --- |
| Apple Silicon | macOS 15 arm64 | `a33a8672fc19eaa2730f90baff49e47f4450ab6952b68152d5658a53e911f042` |
| Intel Mac | macOS 15 x86_64 | `9f640a8a54f145644d5bf1d1e7e8c7898d61ad92a7229128fa334960ebbdb6be` |
| Linux x86_64 | Ubuntu 22.04 与 24.04 | `6d04913b5bb27e645f4fe08aa31f65fa035534d521b78da911c5e9671b429fd0` |

四个原生环境均验证下载物校验和、动态依赖、默认 MCP 和 ZIP/TAR.GZ/7z 往返。Apple Silicon 的验证还实际执行了全新 Homebrew 安装。本机也验证了正式 Apple Silicon 资产的 GitHub attestation。[正式下载包验收](https://github.com/mkynyd/arcthis/actions/runs/44661887483)

通过 crates.io 和 Homebrew 升级本机两个入口，均确认运行 0.7.0。npm、pnpm 与 Shell 安装脚本在独立目录验收。每个入口都执行批量改名与部分失败、归档读写、grep/BLAKE3、补全以及 MCP 初始化和工具调用。本机直连 GitHub 资产超时后，通过向安装进程传入现有系统代理重试成功。pnpm 12.21.0 验证使用 `--allow-build=arcthis` 和 `--config.minimumReleaseAge=0`。

## 复测修复

- 批量解压按发现顺序保留目标，`--rename` 跳过磁盘已有和批内预留路径，worker 使用计划目标。`--continue-on-error` 保留首个可用计划，逐包报告后续冲突和坏包。
- 命令帮助显示适用选项，pack 明确提示先将 stdin 存为文件。MCP 启动时提示先创建输出目录。索引删除文案去掉计数，读取时可用 `archive.index_status` 查看旧缓存失效状态。
- grep 增加 `-i`、`-E`、`--exit-code`，默认保持字面搜索和无匹配退出 0。另增 BLAKE3、Shell 补全、可选的 AppleDouble 文件名过滤。schema 2 保留既有字段，并将机器错误的类别摘要与具体诊断分别放入 `error.message` 和 `error.details.message`。

本地 fmt、严格 Clippy、108 项 all-features 测试、锁定 release 构建和发布源码包验证通过。候选版另通过 88 项 no-default-features 测试。根帮助、16 个子命令帮助与双语网站命令索引已核对。[CLI 契约](./CLI.md) · [Schema 2 迁移](./ADR-0002-EXPLORATION-CONTRACT.md)

## 官网

2026-11-20 部署资源版本 `20261120103843`。中英文六个页面与四个共享资源公网返回 HTTP 200，内容哈希逐文件匹配工作树。1440px 与 390px 视口的 12 组 Chromium 检查覆盖命令内容、版本、语言切换、批量命令入口、安装命令复制、下载链接、横向溢出与控制台错误。[中文文档](https://arcthis.mkynstudio.top/docs.html) · [English docs](https://arcthis.mkynstudio.top/en/docs.html)
