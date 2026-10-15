# 0.6.0 发布验收

更新：2026-10-16（Asia/Shanghai）。维护：殷浚航。

0.6.0 已发布到 crates.io、GitHub Releases、Homebrew 和 npm；pnpm 使用同一 npm 包。所有安装方式默认包含 MCP。JSON 已升级为 schema 2，迁移见 [ADR 0002](./ADR-0002-EXPLORATION-CONTRACT.md)，28 项探索结果见[逐项验收](./EXPLORATION-ITERATION-PLAN.md)。

## 发布来源与门禁

- 正式源码与签名 Tag：[v0.6.0](https://github.com/mkynyd/arcthis/releases/tag/v0.6.0)，指向 `3dfe33c56945fd6e3693cbacebbd07e283e474ea`。
- 功能与候选：[PR #7](https://github.com/mkynyd/arcthis/pull/7)；正式版文档和安装验证：[PR #8](https://github.com/mkynyd/arcthis/pull/8)。
- 正式发布前 main 的 [CI](https://github.com/mkynyd/arcthis/actions/runs/38795623793) 成功，工作树干净且与 origin/main 一致；目标版本、Secrets、源码包、publish dry-run 和 dist 计划检查通过后才上传和打 Tag。
- [正式 Release workflow](https://github.com/mkynyd/arcthis/actions/runs/38796318692) 的三平台构建、安装器、npm、Homebrew 和公告 Job 全部成功；[Tag CI](https://github.com/mkynyd/arcthis/actions/runs/38796318552) 成功。
- 从 crates.io 下载的源码包 SHA-256 为 `9cbea47ceff4073da6d06c1463f1b9973838a5c111275d0c70e7118dec4438eb`，与 registry checksum 一致；包内 VCS 信息指向上述干净源码 commit。

## 平台与安装渠道

[正式下载包 smoke](https://github.com/mkynyd/arcthis/actions/runs/38797443292) 在下列四个原生 runner 上全部通过。验证的是实际下载产物，不是重新构建的替代二进制。

| 产物 | 运行环境 | 验收 |
| --- | --- | --- |
| aarch64-apple-darwin | macos-14 | SHA-256、系统动态库、版本、MCP、归档往返；干净 Homebrew 安装 |
| x86_64-apple-darwin | macos-15-intel | SHA-256、系统动态库、版本、MCP、归档往返 |
| x86_64-unknown-linux-gnu | Ubuntu 22.04 | SHA-256、ELF 直接与系统传递依赖、版本、MCP、归档往返 |
| 同一 Linux 产物 | Ubuntu 24.04 | 同上 |

每个平台完成 ZIP/TAR.GZ/7z 的 pack/list/read/grep/verify，以及生成的 npm 包、Homebrew Formula 的版本和 Release URL 检查。Linux 直接依赖保持受限，系统 libxml2 的 ICU/C++ 传递依赖通过实际 ELF 闭包核验；Ubuntu 22.04 与 24.04 分别使用系统 ICU 70 与 74。

| 安装渠道 | 实际结果 |
| --- | --- |
| Cargo | 从 registry 下载，隔离目录安装；版本、schema 2、pack/list/read/verify、MCP 握手/九工具 schema/实际调用/EOF 通过 |
| npm | 官方 registry 0.6.0，隔离安装；同上 |
| pnpm 11.21 | 新隔离目录安装，按需批准 arcthis 脚本并设置首日发布年龄选项；同上 |
| Homebrew | 干净 macOS arm64 runner 实际安装；版本、默认 MCP 和归档读写/校验通过 |
| GitHub Release | 三个产物在四个 runner 上通过上述独立检查 |

npm 发布先返回 accepted/processing，registry 可见后才完成安装验收，没有重复上传。依赖代理的本机网络仅为安装进程传入已有代理；pnpm 只批准 arcthis。对应客户端说明已同步到双语安装指南和网站。

### 二进制 SHA-256

| 产物 | SHA-256 |
| --- | --- |
| arcthis-aarch64-apple-darwin.tar.xz | `88b52969fae600da9f99df92bd668298c88ba2010f48a62e96353dc7aad0a5a4` |
| arcthis-x86_64-apple-darwin.tar.xz | `7025c6fb31d89b6b92b115f584d6f324f1608eb22c7734e676fea5e1314f15d7` |
| arcthis-x86_64-unknown-linux-gnu.tar.xz | `c06ff32880f9e78b499231f6428f8affad115081ec6c0d402292c02e79fac694` |

## 核心回归

`cargo fmt --check`、严格 all-target/all-feature Clippy、104 项 all-feature tests、84 项 no-default tests、locked release build、源码包验证均通过。独立 JSON Schema validator 验证 15 个 MCP outputSchema 与 19 份实际结果，另有 11 种可创建格式往返和真实 RAR CLI 回归。

同机同 fixture 的 release 三次中位数：20,001 文件 ZIP grep 为 0.302400 秒；1000 空文件抽取各测一次，默认逐文件 fsync 为 4.970414 秒，显式 no-sync 为 0.129756 秒。默认耐久性策略保留。umask 022/077/002 的 pack 输出分别为 644/600/664。

## 网站 QA

生产地址：[中文文档](https://arcthis.mkynstudio.top/docs.html)、[英文文档](https://arcthis.mkynstudio.top/en/docs.html)、[下载](https://arcthis.mkynstudio.top/download.html)。资产版本 `20261016040610`。

Browser 插件未提供，按 frontend-testing-debugging / Playwright 技能使用 headed Chromium。六页分别在 1440×1000 与 390×844 验证；流程是页面载入 → 中英文切换 → 主题切换/复制 → 核对实际状态和剪贴板。

| 检查 | 结果 |
| --- | --- |
| 页面身份、标题、0.6.0/schema 2 | 通过 |
| 非空内容、无错误覆盖层 | 通过 |
| console/page error | 0 |
| 中英文双向切换、主题状态改变 | 通过 |
| 复制按钮回执与实际剪贴板值 | 通过 |
| 页面宽度与锚点 | 12 组通过，无页面横向溢出；代码/表格内部滚动保留 |
| 截图 | 桌面/手机实际渲染已检查，截图保存在仓库外 |
| 部署一致性 | 六页和四资源均 HTTP 200，SHA-256 与源码逐文件相同，部署文件 www:www |

部署前将原六页和 assets 备份到服务器 `/tmp/arcthis-site-before-0.6.0-3dfe33c.tar.gz`，SHA-256 为 `49158691df4d737c63ed2201ab28d2c1aa293125a7b6509d552b5716028f7a11`。仅部署站点文件。

验证范围为上述平台和 Chromium 视口；Windows、Linux arm64、其他浏览器与发行版未在本轮建立独立发布验收。后续发布继续遵循 [RELEASING](./RELEASING.md)。本机详细日志保存在被 Git 忽略的 `archive_test/release-0.6/`，不进入源码包。
