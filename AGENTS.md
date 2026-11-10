# QuaZonai 开发者指南

## 项目定位与首要原则

QuaZonai 是基于 **NautilusTrader 与官方 Pi Durable 封装**的单人量化研究工作台。它连接想法、Agent 研究、原生科学计算、策略与组合分析、目标交付和账户观测。项目的价值在业务集成、研究体验与结果表达，不在重新实现底层框架。

**尽可能不造重复轮子：先查 NautilusTrader 和 Pi Durable 的官方能力，优先直接复用，再做最薄的业务适配。** 只有现有公开接口确实不能满足具体需求时才写项目代码；不要因为不了解上游能力而新增替代实现。依赖使用官方包或已固定的官方上游版本，通过公开 API 集成，不复制上游实现、不维护补丁或 fork。

- NautilusTrader 负责市场数据目录、原生指标、回测、撮合、共享账户模拟和原生交易模型。沿用其数据格式、精度和已有适配器，不另写回测引擎、撮合器或指标框架。
- Pi Durable 负责 Agent 的 Conversation、Submission、Task、Doc、持久化、恢复、重放、子任务、等待、取消和工具循环。沿用其 Harness、Extension 和 Storage，不另建 Agent 调度器、任务状态机、任务数据库或恢复协议。
- 模型与提供方调用复用官方 pi-ai；并发和上下文能力复用 Chord；约束优化复用 Clarabel；列式数据复用 Arrow。已存在的库能力也不重复实现。
- 新功能先找同类入口、合同与调用链，在原位置扩展。局部需求用局部修改解决，避免平行系统、通用平台和多层转接。

## 当前架构：必须保持

以下描述是当前实现的架构约束。**严禁擅自修改架构设计。需要修改时，必须先向项目所有者说明具体变化、必要性、影响范围与迁移方式，并获得针对该变化的明确审批后才能实施。** 一般功能授权、修复授权、清理授权或合并授权不等于架构变更审批。不得先改架构再补说明，也不得通过改写本文件自行解除约束。

### 分层与目录

| 路径 | 职责与边界 |
| --- | --- |
| `packages/contracts/src/` | TypeScript 公共业务类型、HTTP/CLI/Web DTO 与共享规则；跨入口保持同一合同。 |
| `apps/service/src/` | Fastify HTTP `/api/v3`、实例与模型配置、所有者会话、业务事实、研究准入、科学适配、组合、目标回执和账户观测。业务编排留在这一层。 |
| `apps/research/src/` | 官方 Pi Durable 宿主、研究文档和账本、工具调用、延续与恢复、文献和研究评价。`host.ts` 负责打开官方存储及 Harness。 |
| `apps/research/plugins/` | 通过官方 Extension 接入研究与评价工具；使用原宿主，不自建运行时。 |
| `apps/nautilus/src/` | 独立部署的远程 Nautilus HTTP 适配层，连接科学进程与数据目录；不把科学引擎嵌入业务服务。 |
| `apps/job/src/` | Rust 原生科学入口，执行 Nautilus 数据处理、回测、组合及必要数值计算。 |
| `crates/contracts/src/` | 原生科学 Wire 类型、Arrow 类型与可生成的科学合同。 |
| `crates/domain/src/` | 科学计算所需的纯规则、数值与结果校验，不承担 HTTP 服务和 Agent 调度。 |
| `apps/cli/src/` | 同一 HTTP API 的客户端、命令和合同读取；不得直接打开或修改服务存储。 |
| `apps/web/src/` | React、TypeScript、官方 Ant Design 与 ECharts。负责交互和展示，不运行后台研究调度。 |
| `runtimes/data/` | Python 数据获取及来源适配，向原生科学入口提供数据，不重写 Nautilus 引擎。 |
| `contracts/generated/` | 从 Rust 科学合同生成、供程序实际读取的机器合同；它不是手写开发文档，不手工修改。 |
| `deploy/`、`runtimes/native/` | 安装、容器、CLI 分发和原生镜像构建。 |
| `.github/` | 保留的 CI、构建和发布配置。 |

Rust workspace 只保留 `apps/job`、`crates/contracts` 和 `crates/domain`。非科学控制面使用 TypeScript；不得重新引入 Rust Server、Store、Runtime 网关或 Rust CLI 作为另一套控制面。

### 调用与数据流

```text
Web / CLI
    → TypeScript Service / api/v3
        → Research Host / 官方 Pi Durable / pi-ai
            → 远程 Nautilus HTTP 适配
                → Rust job / NautilusTrader / Clarabel / Arrow
        → 独立的模拟盘或实盘账户连接：目标回执与观测
```

研究结果可以供原子因子、策略、组合和 PM 业务消费。组合与模拟是可选消费者，不是开始因子研究的前置步骤。目标交付和账户观测是独立业务事实，研究完成不等于已交付，收到回执不等于已成交。

- `apps/service/src/service.ts` 是服务装配入口；`facts.ts`、`pm-facts.ts` 等通过官方 Durable Doc 表达业务事实。不要另建事实数据库。
- `apps/research/src/host.ts` 装配官方 Harness 与 SQLite Storage；研究、工具和科学子任务使用原 Conversation/Submission/Task 身份。
- `apps/service/src/science.ts`、`apps/research/src/native-execution.ts` 和 `apps/nautilus/src/` 连接现有科学链路；适配应薄且直接。
- 研究/回测使用 `nautilus.backtest` 连接；模拟盘和实盘使用各自实例连接、账户和凭据。各用途独立配置，不互相静默回退。
- 应用启动时重开原 Durable 存储，由官方运行时恢复原 Task。浏览器断开不构成研究取消，本地进程退出不代表远端任务终止。
- 配置、凭据、研究状态、原生结果和既有数据随安装目录持久化。日常改动不得清空数据或改变已存在对象的身份来绕过兼容问题。
- QZ 本身不持有券商账户、不直接发送金融订单；通过配置的下游接口交付目标并读取账户事实。

### 合同与数值

TypeScript 的公共合同放在 `packages/contracts/src/`，原生科学合同放在 `crates/contracts/src/`。前端和 CLI 消费同一服务与合同，不维护另一个业务实现。生成合同从源定义生成，不在生成结果上手改。

保留真实数值含义：金额、十进制和纳秒不要先转成不精确的 JavaScript Number；缺失值不要补成零，失败不要包装成成功。只保留实际业务需要的校验，不为假设性风险增加防御层。

## 开发中严格禁止的做法

以下是明确禁止项，不是可以默认新增的工程惯例：

- **过度设计**：没有当前需求的抽象、通用框架、插件平台、重复封装、多层转发，以及为未来可能性预留整套系统。
- **过度安全、过度防御**：与单人实际使用无关的安全层、权限体系、重复验证、兜底链、隐式降级和防御性协议。实际错误直接、准确地处理，不堆叠机制。
- **SHA256**：禁止新增或推广 SHA256/sha256sum 校验、内容指纹、哈希清单、证明链和依赖这类摘要的开发流程。依赖工具原有锁文件与上游发布元数据不手工重写。
- **冒烟**：禁止新增、运行或要求 smoke/冒烟脚本与流程；验证要针对具体改动和真实行为。
- **夹具**：禁止新增独立 fixture/夹具体系、虚构数据交付、样板环境和用固定假结果代替真实结果的验收。
- **浏览器测试**：禁止新增、运行或要求 Playwright、浏览器端到端、截图比较等浏览器测试流程。
- **门禁**：禁止增加自设审批链、质量门禁、安全门禁、形式化评审签字，以及无实际收益的检查步骤。保留必要 CI，不擅自扩充流程。
- **冻结、基线**：禁止新增冻结清单、冻结快照、基线归档、基线证明、对照证据包等工程机制。
- 禁止恢复 `.opensdlc`、SKILL、任务文档、报告目录及各种额外非代码材料；开发说明统一维护在根目录本文件。必要构建/运行配置、CI 和程序使用的生成合同保留。

这些规则约束后续开发与执行。仓库现有业务可能使用同名字段或历史实现；不要仅为清除关键词而更改科学语义、持久化格式或既有运行架构。确需改变架构时仍必须先获得明确审批。

## 如何开展修改

1. 从用户要完成的具体行为出发，找到现有入口、合同和调用链。
2. 先查 NautilusTrader 与 Pi Durable 的官方接口和已安装版本，再决定项目需要多少适配代码。
3. 在当前分层内做最小修改，复用已有业务规则与组件，不新建平行路径。
4. 根据影响运行必要的类型检查、编译或相关单元测试。不要运行上面禁止的流程，不以数量代替验证价值。
5. 交付时说明行为变化、实际验证结果与未完成事项；用户要求合并到 dev 时完成远程合并并读回提交状态。

## 常用入口与命令

Node 版本由应用 `package.json` 的 engines 与部署配置约定；Rust 版本由 `rust-toolchain.toml` 固定，依赖由对应锁文件管理。

| 目的 | 命令 |
| --- | --- |
| 安装某个 TypeScript 应用依赖 | `npm ci --prefix apps/<应用名>` |
| 检查服务、研究宿主或 CLI 类型 | `npm --prefix apps/service run typecheck`，或替换为 `research` / `cli` |
| 检查 Web 类型 | `npm --prefix apps/web run typecheck` |
| 构建 Web | `npm --prefix apps/web run build` |
| 运行某个应用的单元测试 | `npm --prefix apps/<应用名> test`，只选择与改动有关的测试 |
| 检查 Rust 格式 | `cargo fmt --all -- --check` |
| 检查科学纯规则 | `cargo test --locked -p contracts -p domain --lib --tests` |
| 生成科学合同 | `make generate-native-schema` |
| 启动服务 | `npm --prefix apps/service start` |
| 启动远程 Nautilus 适配 | `npm --prefix apps/nautilus start` |
| 使用开发 CLI | `npm --prefix apps/cli start -- --help` |

科学计算需要配置真实远程实例和数据路径；不得以虚构结果冒充原生运行完成。只运行当前需求必要的检查，不盲目执行 Makefile 中所有历史目标。
