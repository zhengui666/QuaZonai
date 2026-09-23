# QuaZonai

**面向个人、本机、自托管使用的生产级量化研究系统。** Rust 负责科学计算，Codex 组织研究，React / Ant Design 提供 Web / PWA。系统保存可追溯的 Alpha、组合回测与 target-only 目标包；不持有券商凭据、不发送真实订单。

[部署与运行](OPERATIONS.md) · [安装 Agent Skill](#agent-skill) · [CLI](CLI.md) · [架构与合同](DESIGN.md)

[![CI](https://github.com/zhengui666/QuaZonai/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/zhengui666/QuaZonai/actions/workflows/ci.yml)

<a id="quickstart"></a>
## 安装与启动

准备 Linux x86_64、Rust 1.98.1、Node.js ≥22.12、PostgreSQL 18 / PGMQ 1.10.0 和 Caddy。API、Worker 与原生 Codex 使用同一操作系统用户。

```sh
git clone https://github.com/zhengui666/QuaZonai.git
cd QuaZonai
```

按[安装步骤](OPERATIONS.md#install)构建正式静态资源和 Rust 二进制，初始化状态目录、显式迁移数据库，并启动 API、Worker 与网关。默认访问 `http://localhost:8081`；页面直接进入工作台，不需要账号、验证码或设备信任。API 和网页只监听 loopback。

先在运行服务的用户终端执行 `codex login`。系统自动发现该用户的 `PATH`、`HOME` / `CODEX_HOME` 与原生配置；设置页只选择模型和推理强度，“本机默认”沿用原生设置。右上角切换浅色／深色主题。

研究前登记真实数据及许可、探测计算 Runtime，并冻结输入和预算。数据、模型、数据库或计算端不可用时显示实际错误，不生成替代结果。升级与恢复保留原数据库、状态目录、密钥和任务身份。

<a id="agent-skill"></a>
## 安装 Agent Skill

使用 [skills.sh 的官方 CLI](https://skills.sh/docs/cli)，准备 Git 和 Node.js ≥22.20（推荐 Node.js 24），在需要使用 Skill 的项目目录执行：

```sh
npx skills add zhengui666/QuaZonai --skill quazonai
```

按提示选择 Agent，默认安装到当前项目。全局安装到 Codex 和 Claude Code：

```sh
npx skills add zhengui666/QuaZonai --skill quazonai --agent codex claude-code --global --yes
```

仅使用一个 Agent 时只保留对应名称。查看或更新全局安装：

```sh
npx skills list --global
npx skills update quazonai --global
```

安装器从仓库发现 `skills/quazonai/SKILL.md`，安装整个目录及 `references/`；无需手动克隆源码、复制文件或另行发布 npm 包。此 Skill 用于操作已有服务，不安装或启动 QuaZonai，也不创建凭据或 MCP 连接。宿主仍须提供匹配的 `server` CLI 和已有机器连接，或已绑定的 Mission MCP；连接要求见[包内说明](skills/quazonai/references/connection.md)。

## 结构

Web / CLI / MCP → Rust API → PostgreSQL / PGMQ → Worker → Codex / 独立 Runtime。

科学任务复用 NautilusTrader、Wasmi、Clarabel、Arrow 与 ndarray。QZ 负责研究规则、任务编排和结果关联，不重写撮合、优化器或 Agent 工具循环。

| 需要 | 入口 |
| --- | --- |
| 安装、配置、备份和恢复 | [OPERATIONS](OPERATIONS.md) |
| 服务 Agent 与命令接口 | [操作 Skill](skills/quazonai/SKILL.md)、[CLI](CLI.md)、`server --help` |
| 字段、状态机与模块边界 | [DESIGN](DESIGN.md) |
| 开发和验证 | [CONTRIBUTING](CONTRIBUTING.md)、[AGENTS](AGENTS.md) |

## 开发

按[贡献指南](CONTRIBUTING.md#verify-the-change)选择对应检查。浏览器回归使用真实 Rust API、PostgreSQL、Worker 和 Caddy；科学回归使用原生引擎。编译复用的手动微基准：

```sh
cargo run --locked --release -p job --example benchmark_signals
```

基准同时核对结果和 fuel；计时不设置通过阈值。

## License

原创代码：[AGPL-3.0-only](LICENSE)。第三方许可与版权：[NOTICE](NOTICE)、[THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES.md)。
