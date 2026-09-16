# QuaZonai

证据优先的自托管量化研究工作台。通过 Rust 原生计算、Codex 会话与独立评估，把研究想法变成可追溯结论和 target-only 组合包；不持有券商凭据，也不发送真实交易指令。

**当前仍未完成生产验收。** [PR #63](https://github.com/zhengui666/QuaZonai/pull/63) 已合并，完整 Demo、真实账号/数据链路及旧数据恢复等缺项统一记录在[实现与验收证据](docs/architecture/issue-62-execution.md#acceptance)。通过 CI 或打开合成预览不能替代这些验收。

## 从哪里开始

| 目的 | 入口 |
|---|---|
| 体验界面 | 下文的合成预览，不需要账号或数据库 |
| 配置真实服务 | [运行与部署](OPERATIONS.md)，包含认证、数据、Runtime、恢复和切换边界 |
| 使用命令/API | [CLI](CLI.md)；命令帮助和 OpenAPI 从实际 Rust 实现产生 |
| 修改项目 | [治理](AGENTS.md)、[完整设计](DESIGN.md)、下文的验证入口 |
| 检查支持范围 | [原生兼容矩阵](docs/architecture/compatibility-matrix.md)、[证据索引](docs/architecture/issue-62-execution.md) |

## 合成界面预览

需要 Node.js ≥22.12、npm 和 make。在仓库根目录运行：

```sh
make demo-preview
```

浏览器打开 [本机预览](http://127.0.0.1:4179)，按 Ctrl+C 停止。预览复用正式 React/Ant Design 界面与响应合同，所有数据均明确标为合成数据。它支持项目创建/编辑、Brief 草稿及研究/组合/交付历史查看；完整研究、Brief 冻结、交付领取等仍被拒绝，不是完整 T02 Demo。

数据只保存在进程内存中，重启清空；当前最多保留 256 次项目创建/编辑回执。预览不连接真实 API、数据库、账号或下游，请勿输入真实凭据。PWA 测试服务器也不是部署入口。

## 开发环境

后端：Linux x86_64、C 工具链、rustup 和 [rust-toolchain.toml](rust-toolchain.toml) 固定的 Rust 1.98.1。前端使用 Node.js ≥22.12，CI 使用 Node.js 24。依赖通过 Cargo.lock 和 npm lockfile 安装；第一方后端不需要 Python。Nautilus 所属发布族仍为 RC，支持范围见兼容矩阵。

```sh
rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy
rustup run 1.98.1 rustc -Vv
npm ci --prefix apps/web --ignore-scripts --no-audit --no-fund
npm ci --prefix runtimes/codex --ignore-scripts --no-audit --no-fund
```

`make` 使用 rustup 明确选择固定编译器，避免宿主发行版的 cargo 绕过工具链文件。文档链接检查还需要 [lychee 0.24.2](https://github.com/lycheeverse/lychee/releases/tag/lychee-v0.24.2)，可用 `cargo install --locked lychee --version 0.24.2` 安装；CI 使用同一版本。

## 验证入口

| 命令 | 实际范围 |
|---|---|
| `make check-docs` | 本地 Markdown 文件/标题链接及全部 server 子命令帮助；不访问真实账号或执行文档中的任意 Shell |
| `make check-unit` | 格式、全部目标/原生 feature 的 Clippy，以及不需要 PostgreSQL 的 workspace 测试子集 |
| `make check` | 上述检查和 Store/Server 测试；必须显式提供可丢弃 PostgreSQL18 + PGMQ1.10.0 实例及原生 Codex 测试程序 |
| `make check-store` / `make check-http` | 分别运行真实数据库 Store 或 HTTP/Worker 测试，前提见 [CLI 开发测试](CLI.md#开发测试) |
| `make check-web` | 生成客户端一致性、类型检查、单元测试和正式静态构建；先完成前端锁定安装 |
| `make native OUTPUT=/tmp/quazonai-native-example` | 新目录中的真实 Rust Clarabel/Nautilus/Arrow 合成探针；FIXTURE 不能交付 |

完整验证不能用 `check-unit` 替代，也不能使用生产数据库。测试依赖和真实浏览器入口详见 [CLI](CLI.md#开发测试)；真实 OCI 生命周期与隔离入口见 [Runtime](runtimes/native/README.md)。合成模型响应、合成行情、真实账号与授权市场数据分别记录，不合并推断生产通过。

前端开发和浏览器检查：

```sh
npm --prefix apps/web run dev
# 在另一个终端执行；先安装 Playwright Chromium。
npm --prefix apps/web run test:e2e
```

开发 API 代理只接受回环 HTTP 地址；实际认证服务的 `PUBLIC_URL` 必须与浏览器 Origin 一致。真实浏览器验收使用 `npm --prefix apps/web run test:e2e:native`，需要独立测试数据库和已构建 server，前提见 [CLI](CLI.md#开发测试)。

## 架构与维护

- `apps/server`：HTTP、Worker、CLI/MCP、身份与领域编排；`crates/contracts/domain/store/integrations`：共享合同、业务约束、事务和原生适配。
- `apps/runtime`：固定原生任务网关；`apps/job`：受限科学计算；[原生镜像说明](runtimes/native/README.md)负责装配和运行。
- `apps/web`：React、TypeScript、官方 Ant Design 与 PWA；生成类型/验证器来自 Rust OpenAPI，不手改平行协议。

[DESIGN](DESIGN.md) 是产品事实源；操作步骤留在 [OPERATIONS](OPERATIONS.md)，命令留在 [CLI](CLI.md)，[项目 Skill](skills/quazonai/SKILL.md)只做导航。历史实现日志留在 Git，不向入口文档追加第二套状态机。变更需更新受影响的原文并通过对应检查。

[CI](https://github.com/zhengui666/QuaZonai/actions/workflows/ci.yml)、[Web](https://github.com/zhengui666/QuaZonai/actions/workflows/web.yml)、[Runtime](https://github.com/zhengui666/QuaZonai/actions/workflows/native-runtime.yml) 和 [CodeQL](https://github.com/zhengui666/QuaZonai/actions/workflows/codeql.yml) 展示实际运行；每个提交重新验证，绿色记录不等于生产发布。

## 真实界面

以下为一次性数据库中的 Chromium → Rust API → PostgreSQL 验收截图，包含真实 TOTP 初始化后的项目写入与响应丢失重试；截图不包含认证秘密，不是市场研究或完整 T42 证据。

![研究工作台](docs/images/native-projects-1440.png)

[平板](docs/images/native-projects-768.png) · [手机](docs/images/native-projects-390.png)。`test:e2e:native` 只有验收和资源清理成功后才导出这些视口的公开截图。

## License

原创代码保持 [AGPL-3.0-only](LICENSE)。第三方许可和版权说明保留在 [NOTICE](NOTICE) 与 [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES.md)。清理源码不删除用户数据、备份或许可证。
