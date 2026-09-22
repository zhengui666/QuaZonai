# QuaZonai

**个人、本机、自托管的量化研究工作台。** Rust 负责科学计算，Codex 组织研究，React / Ant Design 提供 Web / PWA。输出可追溯的 Alpha、组合回测与 target-only 目标包；不持有券商凭据、不发送真实订单。

[部署](docs/user-guide.md) · [运行手册](OPERATIONS.md) · [Agent 操作](docs/agent-operations.md) · [CLI](CLI.md) · [架构](docs/architecture.md)

[![CI](https://github.com/zhengui666/QuaZonai/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/zhengui666/QuaZonai/actions/workflows/ci.yml)

> 当前是开发版本，完整生产验收尚未完成。[验收证据](docs/architecture/issue-62-execution.md#acceptance)区分真实执行、合成测试与未验收项；专用账号实测已由所有者豁免，并不等于执行通过。

## 本机使用

网页直接进入工作台，不需要账号、验证码或设备信任。API 与网页仅监听 loopback，不提供公网免登录模式。数据库可由同一所有者账号迁移和运行；独立低权限角色是可选部署方式，不是启动条件。

Codex 自动沿用同一 OS 用户的 `PATH`、`HOME` / `CODEX_HOME` 和原生配置。先在该用户终端执行 `codex login`，设置页只选择模型与推理强度；启用“本机默认”即不覆盖原生设置。右上角切换浅色／深色主题。

正式部署需要 Linux x86_64、Rust 1.98.1、PostgreSQL / PGMQ；前端使用 Node.js ≥22.12。按[个人部署指南](docs/user-guide.md)构建、初始化独立状态目录、显式迁移并启动 API / Worker。已有数据先备份，不通过删除状态目录解决启动问题。

<a id="quickstart"></a>
## 无凭据界面预览

```sh
git clone https://github.com/zhengui666/QuaZonai.git
cd QuaZonai
make demo-preview
```

打开 <http://127.0.0.1:4179>。预览标注 **SYNTHETIC / FIXTURE**，使用内存数据；重启清空，不执行真实模型、科学计算或交付。`Ctrl+C` 停止。正式部署不要使用 `vite preview` 或演示数据代替服务。

## 结构与文档

Web / CLI / MCP → Rust API → PostgreSQL / PGMQ → Worker → Codex / 独立 Runtime。科学任务复用 NautilusTrader、Wasmi、Clarabel、Arrow 与 ndarray；QZ 保留研究规则和证据关联，不重写撮合、优化器或 Agent 工具循环。

| 需要 | 入口 |
| --- | --- |
| 部署、配置、备份与恢复 | [个人部署](docs/user-guide.md)、[OPERATIONS](OPERATIONS.md) |
| 服务 Agent 与命令接口 | [Agent 操作](docs/agent-operations.md)、[CLI](CLI.md)、`server --help` |
| 字段、状态机与模块边界 | [DESIGN](DESIGN.md)、[架构](docs/architecture.md) |
| 开发与验证 | [CONTRIBUTING](CONTRIBUTING.md)、[AGENTS](AGENTS.md) |

## 开发

按[贡献指南](CONTRIBUTING.md#verify-the-change)选择与改动相关的检查。CI 保留真实数据库、原生科学计算、接口生成与浏览器回归；不再生成每次提交的双份 SBOM 或运行专属 CodeQL 扫描。编译复用的手动微基准：

```sh
cargo run --locked --release -p job --example benchmark_signals
```

基准同时核对结果与 fuel；计时不设置通过阈值，也不代表完整研究任务的提速倍数。

## License

原创代码：[AGPL-3.0-only](LICENSE)。第三方许可与版权：[NOTICE](NOTICE)、[THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES.md)。
