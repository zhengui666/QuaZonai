# QuaZonai

**把量化研究想法变成可追溯的证据与目标组合。**

QuaZonai 是面向独立研究者的单用户、自托管研究工作台。Rust 原生组件负责科学计算，Codex 组织有界研究，独立评估约束结论，最终向下游交付 target-only 组合包。QZ 不持有券商凭据，不发送真实交易指令。

[个人部署](docs/user-guide.md) · [使用与恢复](OPERATIONS.md) · [界面预览](#quickstart) · [架构](docs/architecture.md) · [参与贡献](CONTRIBUTING.md)

[![CI](https://github.com/zhengui666/QuaZonai/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/zhengui666/QuaZonai/actions/workflows/ci.yml)
[![Web console](https://github.com/zhengui666/QuaZonai/actions/workflows/web.yml/badge.svg?branch=main)](https://github.com/zhengui666/QuaZonai/actions/workflows/web.yml)
[![Native Runtime](https://github.com/zhengui666/QuaZonai/actions/workflows/native-runtime.yml/badge.svg?branch=main)](https://github.com/zhengui666/QuaZonai/actions/workflows/native-runtime.yml)

> **当前仍为开发版本，完整生产验收未完成。** 真实服务部署与无凭据界面预览是两条不同路径。服务启动、部署模板和 CI 通过不证明真实账号、授权数据、完整研究交付或恢复已验收；实际状态见[验收证据](docs/architecture/issue-62-execution.md#acceptance)。
>
> 专用账号实测已按[所有者验收修订](DESIGN.md#acceptance-scope)完成豁免（未执行），其余验收使用 GitHub Actions 继续；这不证明真实账号登录或推理已运行。实际模型研究仍需配置可用的原生 Profile/认证，不能以豁免将空配置或 Demo 视为就绪。
>
> **English:** A personal, self-hosted quantitative research workbench for traceable evidence and target-only portfolio delivery. Dedicated-account acceptance is complete by owner waiver, not execution; other production acceptance remains incomplete. See the [personal-hosting guide](docs/user-guide.md) or try the explicitly synthetic preview below.

![真实 API 与数据库验收中的研究工作台](docs/images/native-projects-1440.png)

截图来自一次性数据库中的 Chromium → Rust API → PostgreSQL 验收，包含真实认证后的项目操作；不代表完整市场研究验收。[平板](docs/images/native-projects-768.png) · [手机](docs/images/native-projects-390.png)

## 为个人研究而设计

研究结果应该能回答“用了什么、试过什么、为什么相信它”，而不只是一次聊天或一张收益曲线。QuaZonai 将 Brief、输入、预算、运行、产物和评估关联到持久记录，失败与无效结论也被保留。科学能力复用 Codex App Server、NautilusTrader、Clarabel、Arrow 和 PostgreSQL/PGMQ；QZ 只负责自身的规则、权限和证据关联。

从研究结果到目标交付，资格、组合评估、人工授权与下游领取有各自的边界。页面显示的历史成功不等于当前资格；目标包交付也不是实际成交。已实现入口和未验收场景见[覆盖表](docs/architecture/issue-62-execution.md#executable-coverage-map)。


## 本机工作台

打开本机网页直接进入研究工作台，不使用账号、动态验证码或设备信任。
API 和网页代理只监听 loopback；不要把它们暴露到局域网或公网。

Codex 从服务进程的 `PATH` 自动发现，沿用同一操作系统用户的 `HOME` /
`CODEX_HOME` 和原生配置。先在该用户的终端运行 `codex login`。
“设置 → Codex”只配置模型与推理强度；启用“本机默认”时不覆盖原生设置，
关闭时可选原生目录提供的模型与强度。研究员和独立审阅员自动建立，无需注册 Provider 或目录。

右上角的主题按钮切换浅色／深色。首次跟随系统，手动选择后保存在当前浏览器。

## 个人部署：真实服务与持久数据

[个人部署指南](docs/user-guide.md)给出 Linux 上的正式构建、独立状态目录、API/Worker 服务托管、同源 HTTPS 网关、升级与恢复路径。网页由 Caddy 提供静态文件，真实请求进入 Rust API；API 与 Worker 使用同一份持久数据和配置。它不是 `vite preview`，也不会在服务启动时自动创建密钥或执行数据库迁移。

运行前需要准备 PostgreSQL/PGMQ、独立数据库身份和匹配的本机 Origin。本机 Codex、授权数据与独立 Runtime 是实际研究的额外前提；空配置不会被伪装成就绪。先在可丢弃环境完成[验收与恢复检查](docs/user-guide.md#recovery)，再决定是否用于自己的正式工作。完整配置和操作仍以 [OPERATIONS](OPERATIONS.md) 为准。

<a id="quickstart"></a>
## 无凭据界面预览

需要 Git、Node.js ≥22.12、npm 和 make；首次安装需要访问 npm registry。在仓库内，一条 `make demo-preview` 命令启动合成预览：

```sh
git clone https://github.com/zhengui666/QuaZonai.git
cd QuaZonai
make demo-preview
```

打开 [http://127.0.0.1:4179](http://127.0.0.1:4179)，页面始终标注 **SYNTHETIC / FIXTURE**。从预置的“双 Alpha 研究示例”体验以下流程：

1. 打开“查看冻结版本”→“以此创建新版本”，修改假设并保存草稿。保留原数据引用，以及 DISCOVERY、VALIDATION、SEALED 三个数据绑定。空绑定草稿会被拒绝；仍有绑定但缺少某个用途的草稿可以保存，冻结时必须补齐并与输入集完全匹配。
2. 在新草稿所在行选择“冻结执行上下文”，选择 `SYNTHETIC · Demo Runtime` 和对应的三个输入集。确认后，再通过“修改项目状态”明确启用项目；冻结不会自动启用。
3. 从刚冻结的新版本启动 Cycle，分别选择 `SYNTHETIC · Demo Researcher` 与 `SYNTHETIC · Demo Reviewer`。这是内存中的交互演示，不执行模型或实验，不生成合格候选；可查看本次关联的运行记录。
4. 查看原历史周期的试验选择，以及 Alpha、组合候选和 Release 的独立历史样例。在目标包详情下载原始 DEMO Package，检查其中的限制说明。它们不是本次新 Cycle 计算出的结果，不能用于审批、登记生产 Offer 或领取。

原 Brief v1 保留其历史冻结上下文，指向已停用、版本已变化的 Runtime；新配置不会原地替换该上下文。要体验新的冻结/启动路径，应按上面步骤创建新版本。新建空项目只能演示元数据编辑，不会自动继承示例数据、研究或交付记录。启用项目需要冻结 Brief，归档不可退出。

演示按项目和 UTC 自然日检查所选冻结 Brief 的每日周期额度；预置预算为每日 2 次。达到额度后，新请求显示预算不足；更换请求键、暂停后启用或创建相同预算的新 Brief 不会清零当日次数。相同已成功请求的重试只返回原回执，不重复创建周期。项目被其他页面修改时，应按修订冲突提示重载最新版本，而不是连续提交旧请求。

按 **Ctrl+C** 停止。数据只在进程内存中，重启清空；项目和 Brief 的创建/编辑回执合计最多 256 条。预览复用正式 React/Ant Design 界面和响应合同，但没有真实 API、数据库、账号、科学计算或下游连接；Runtime 的配置显示不代表已有原生探测能力。请勿输入真实凭据，不要把预览进程接到个人部署的网关后面。浏览器测试入口与 T02 覆盖范围见[验收证据](docs/architecture/issue-62-execution.md#synthetic-demo-evidence)，不能用它代替完整生产验收。

## 系统结构

```mermaid
flowchart LR
  Browser[Web / PWA] --> Gateway[Caddy: HTTPS / 静态文件]
  Gateway --> Server[Rust API]
  CLI[CLI / MCP] --> Server
  Server --> DB[(PostgreSQL / PGMQ)]
  Worker[Worker] --> DB
  Server --> Artifacts[持久研究产物]
  Worker --> Artifacts
  Worker <-->|原生会话| Codex[Codex App Server]
  Worker <-->|有界科学任务| Runtime[独立 Runtime / OCI]
  Runtime --> Job[Rust Nautilus / Clarabel / Arrow]
  Server <-->|目标包 / Claim / ACK| Downstream[独立下游系统]
```

后端构建基线为 Linux x86_64、Rust 1.98.1；前端为 React/TypeScript/官方 Ant Design。Rust crate 不等于独立微服务；网关不管理业务会话，systemd 不替代研究队列或 Runtime 恢复日志。组件职责和真实调用链见[架构导览](docs/architecture.md)，依赖与平台范围见[兼容矩阵](docs/architecture/compatibility-matrix.md)。

## 文档导航

| 目标 | 入口 |
| --- | --- |
| 安装与托管自己的实例、处理启动与页面故障 | [个人部署指南](docs/user-guide.md) |
| 配置账号、数据与 Runtime，操作研究、交付、升级与恢复 | [运行手册](OPERATIONS.md) |
| 使用 CLI、HTTP、MCP 与错误重试 | [CLI 与协议](CLI.md)及实际 `server ... --help` |
| 理解模块职责、接口边界与请求调用链 | [架构导览](docs/architecture.md) |
| 查阅字段、状态机与完整验收合同 | [DESIGN](DESIGN.md) |
| 查看覆盖、限制和剩余工作 | [实现证据](docs/architecture/issue-62-execution.md) |
| 开发、测试与贡献 | [CONTRIBUTING](CONTRIBUTING.md)、[AGENTS](AGENTS.md)、[OpenSDLC](.opensdlc/project.md) |

OpenAPI、前端类型与响应验证器由 Rust 合同及生成器产生，不维护另一份手写协议。产品合同集中在 DESIGN；运行手册与源码导航不另建业务事实。

## 开发与验证

按[贡献指南](CONTRIBUTING.md#set-up-a-checkout)准备工具链与依赖，然后选择与改动有关的检查：

```sh
make check-docs          # 文档链接、标题锚点与原生 CLI help
make check-architecture  # Rust workspace 直接依赖边界
make check-unit          # 格式、Clippy、无需数据库的测试子集
make check-web           # 生成客户端、类型、单元测试与正式构建
CADDY_BIN=/path/to/caddy node --test deploy/proxy.test.mjs
```

完整数据库/HTTP 检查使用 `make check`，需要可丢弃的 PostgreSQL18 + PGMQ1.10.0 和原生测试前提；单元测试不能替代它。三视口/PWA、真实 API 浏览器与 OCI 检查见[检查选择表](CONTRIBUTING.md#verify-the-change)。网关测试使用真实 Caddy 和合成 HTTP 对端，只验证网关行为，不证明真实研究或生产部署。

欢迎通过 [Issues](https://github.com/zhengui666/QuaZonai/issues) 提交可复现问题，通过 [PR](https://github.com/zhengui666/QuaZonai/pulls) 提交聚焦的实现与测试。按 [PR 模板](.github/PULL_REQUEST_TEMPLATE.md)说明实际验证与限制；维护者是 [@zhengui666](https://github.com/zhengui666)。

## License

原创代码保持 [AGPL-3.0-only](LICENSE)。第三方许可与版权说明见 [NOTICE](NOTICE) 和 [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES.md)。
