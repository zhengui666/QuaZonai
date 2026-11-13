# QuaZonai 开发者指南

## 项目定位与首要原则

QuaZonai 是基于 **NautilusTrader 与官方 Pi Durable 封装**的单人量化研究工作台。它连接想法、Agent 研究、显式原生策略回测、目标交付和账户观测。项目的价值在业务集成、研究体验与结果表达，不在重新实现底层框架。

**尽可能不造重复轮子：先查 NautilusTrader 和 Pi Durable 的官方能力，优先直接复用，再做最薄的业务适配。** 只有现有公开接口确实不能满足具体需求时才写项目代码；不要因为不了解上游能力而新增替代实现。依赖使用官方包或已固定的官方上游版本，通过公开 API 集成，不复制上游实现、不维护补丁或 fork。

- NautilusTrader 负责市场数据目录、原生指标、回测、撮合、共享账户模拟和原生交易模型。沿用其数据格式、精度和公开接口，不修改、裁剪或替换上游功能与行为，不另写回测引擎、撮合器或指标框架。QuaZonai 尚未接入的能力必须明确标为接线范围，而不是限制 Nautilus 本体。
- Pi Durable 负责 Agent 的 Conversation、Submission、Task、Doc、持久化、恢复、重放、子任务、等待、取消和工具循环。沿用其 Harness、Extension 和 Storage，不另建 Agent 调度器、任务状态机、任务数据库或恢复协议。
- 模型与提供方调用复用官方 pi-ai；并发和上下文能力复用 Chord；列式数据复用 Arrow。旧资金 forward、自研 Clarabel 分配与 WASM 执行链已退役，不得恢复为研究或账户执行入口；历史合同、错误诊断和原始资金记录不因此删除。已存在的库能力不重复实现。
- 新功能先找同类入口、合同与调用链，在原位置扩展。局部需求用局部修改解决，避免平行系统、通用平台和多层转接。

## 当前架构：必须保持

以下描述是当前实现的架构约束。**严禁擅自修改架构设计。需要修改时，必须先向项目所有者说明具体变化、必要性、影响范围与迁移方式，并获得针对该变化的明确审批后才能实施。** 一般功能授权、修复授权、清理授权或合并授权不等于架构变更审批。不得先改架构再补说明，也不得通过改写本文件自行解除约束。

### 分层与目录

| 路径 | 职责与边界 |
| --- | --- |
| `packages/contracts/src/` | TypeScript 公共业务类型、HTTP/CLI/Web DTO 与共享规则；跨入口保持同一合同。 |
| `apps/service/src/` | Fastify HTTP `/api/v3`、实例与模型配置、所有者会话、业务事实、研究准入、科学适配、组合、目标回执和账户观测。业务编排留在这一层。 |
| `apps/research/src/` | 官方 Pi Durable 宿主、研究文档和账本、工具调用、延续与恢复、文献与原生结果。`host.ts` 负责打开官方存储及 Harness。 |
| `apps/research/plugins/` | 通过官方 Extension 接入研究与原生指标工具；使用原宿主，不自建运行时。 |
| `apps/nautilus/src/` | 既有远程 Nautilus HTTP 适配层，连接科学进程、数据目录及显式配置的账户原生宿主；账户与科学执行复用现有容器。 |
| `apps/job/src/` | Rust 原生入口，执行科学计算及官方 LiveNode 账户宿主；订单、成交、持仓与账户恢复沿用 Nautilus 公开接口，业务控制留在 TypeScript。 |
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
                → Rust job / NautilusTrader / Arrow
        → 独立的模拟盘或实盘账户连接：目标回执与观测
```

当前研究入口直接调用显式原生策略回测，不再走自研因子生成、WASM 拟合、独立评分或 PM BUILD/STUDY 准入链。回测结果是历史计算，不是 PASS、不代表样本外有效，也不自动生成交易许可。目标交付和账户观测是独立业务事实，研究完成不等于已交付，收到回执不等于已成交。

- `apps/service/src/service.ts` 是服务装配入口；`facts.ts`、`pm-facts.ts` 等通过官方 Durable Doc 表达业务事实。不要另建事实数据库。
- `apps/research/src/host.ts` 装配官方 Harness 与 SQLite Storage；研究、工具和科学子任务使用原 Conversation/Submission/Task 身份。
- `apps/service/src/science.ts`、`apps/research/src/native-execution.ts` 和 `apps/nautilus/src/` 连接现有科学链路；适配应薄且直接。
- 研究/回测使用 `nautilus.backtest` 连接；模拟盘和实盘使用各自实例连接、账户和凭据。各用途独立配置，不互相静默回退。
- 应用启动时重开原 Durable 存储，由官方运行时恢复原 Task。浏览器断开不构成研究取消，本地进程退出不代表远端任务终止。
- 配置、凭据、研究状态、原生结果和既有数据随安装目录持久化。不得以清空数据或更换对象身份掩盖升级失败。旧科研流程不提供兼容执行；已有账户事实、资金记录和未决订单仍须保留并核实，不得据此恢复已删除的科研算法。
- QZ 本身不持有券商账户、不直接发送金融订单；通过配置的下游接口交付目标并读取账户事实。
- 资金退出复用上述账户下游及现有容器，由官方 LiveNode、Strategy、RiskEngine、ExecutionEngine、Cache 与 Hyperliquid 适配器执行。QZ 通过现有 Pi Durable 保存退出意图、预留和命令；外部划转和提现由用户完成并以平台流水对账。资金合同校验在共享 TypeScript 层执行，不依赖科学 Worker。

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

## 原生研究与历史扫描记录的实际范围

- `native_strategy_configure` 在同一 Pi Conversation 保存官方策略的明确请求；`native_strategy_backtest` 通过原 Task、远端执行回执和已绑定 catalog 运行，不另建策略状态机。
- 当前单标的回测已接官方 EMA_CROSS（QuoteTick）、ORDER_BOOK_IMBALANCE（L2）、GRID_MARKET_MAKER（QuoteTick）及 HURST_VPIN_DIRECTIONAL（官方 BAR/QuoteTick/TradeTick 订阅）。Grid 的策略专属字段逐一映射官方 GridMarketMakerConfig，instrument_id 来自原请求；可选 base 原样由官方 StrategyConfig 解析。省略 trade_size/expire_time_secs 保持官方 None 语义，其余数值/开关显式提供，不由胶水补交易规则。尚未接任意 Python 策略、多标的或全部原生策略。
- Hurst/VPIN 的所有策略专属配置按公开 HurstVpinDirectionalConfig 字段映射，窗口校验与指标/交易算法由上游执行。官方 INTERNAL BAR 由 DataEngine 按真实 TradeTick 聚合；EXTERNAL BAR 使用完整 BarType 标识，不能以 instrument ID 替代。同标的不同 BarType 可并列读取。外部非时间 BAR 若只用作策略输入，应显式设置官方 venue.bar_execution=false 并提供真实 Quote/Trade 执行数据；省略保持官方 true 及其原生拒绝行为，不静默改变撮合语义。
- 回放使用原始事件与可用时间窗口、官方账户/撮合/费用及原生统计；不把缺失值补零，不把回测当独立样本外验证。账户运行仍仅允许原有 EMA_CROSS，Grid、Hurst/VPIN 和订单簿不因回测注册而开放账户执行。
- 自研参数网格枚举、批量择优与扫描执行已退役，Web/API/CLI 不再创建、重试或取消后重提扫描。历史扫描保留原始请求、Task、结果及回执，展示只读状态；必要的停止操作只沿原身份处理未决子任务，不创建新回测。
- 已退役的研究评分、review allocation、ensemble fit/apply/freeze、自研因子工具和旧 PM 科研创建入口不提供兼容执行，不为旧记录重装旧插件。Rust job 已不依赖 WASM 解释器或 Clarabel 求解器；历史 Wire 类型及拒绝旧请求的测试仍可保留相关标识，不等于支持执行。真实资金/账户与原始数据代码须按调用图保留，不能按关键词盲删。
- 原生兼容方向是使用官方配置、策略、指标、数据与报告接口，而不是再造策略 DSL。当前 Rust `add_strategy<T>` 需要编译时具体类型；尚无任意 Python `ImportableStrategyConfig` 动态执行接线，原生策略入口也不支持用户源码即席运行。已接入的策略不是 Nautilus 全部能力，扩展应接官方公开接口，不修改上游。

## 账户资金退出运行配置

既有远程 Nautilus 入口通过 `QZ_NAUTILUS_ACCOUNTS` 显式配置账户数组；每项包含 `id`、`clientId`、`accountId`、`mode` 和原生现货 `instruments`。Paper 另外要求 `startingBalance` 与显式 `paperFeeRate`；LIVE 使用 `accountAddress` 指定 master 地址，`network` 选择 mainnet/testnet。签名密钥通过账户宿主读取的 `HYPERLIQUID_PK` / `HYPERLIQUID_TESTNET_PK` 注入，不写入源码或业务请求。

应用账户连接 URL 指向既有 Nautilus 服务的 `/accounts/<id>/`，凭据沿用该服务配置。`QZ_ACCOUNT_BINARY` 可指定原生宿主，默认使用科学可执行文件同目录的 `account-runtime`。账户业务记录与官方缓存保存在既有 Nautilus 数据目录下 `accounts/<id>/`；保留该目录以恢复原始账户和未决订单。原生宿主使用 `cargo build --locked -p job --features account-runtime --bin account-runtime` 构建，并由现有镜像构建脚本打包。

新模拟请求的原生模型版本须声明 `0.69.0`，旧 `0.68.0` 模型请求不会被静默按新版执行；账户观测仅接受当前 `0.69.0`；旧版本不支持。账户缓存版本与上游 revision 必须和当前固定版本完全一致，否则明确拒绝，不自动迁移、重标版本、删除或清空原文件。

Nautilus 固定到官方 `634e09456c949f488b2bfcf6b6897b33cbf218d7`（0.69.0）。账户 `executionProductTypes` 可显式设置为 `["SPOT"]`，复用上游订单、私有报告和恢复的产品范围限制；省略时仍包含所有产品，不改变旧账户恢复可见性。缩小范围前必须处理范围外未结订单与本地未决提交，范围外订单不会自动撤销；余额及定时全账户撤单仍是账户级，产品范围不是账户隔离。保留原账户目录，不通过清缓存完成升级。提交恢复显式采用官方 `RetainUnresolved`：未知提交结果保持未决，不把查询缺失推断成拒绝或完成；上游恢复预算耗尽后不再自动逐单重查，未确认的撤单/改单可恢复之前的已接受状态，须继续核实真实状态。

资金流程使用账户页的“退出资金”或同一 HTTP API 的 CLI `capital context/status/preview/command/get`。Paper 的现金与可提取证据始终标记为模拟；实盘现货余额、提现路径可提取额和用户外部已完成提取分别核实。

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

## 原生条件订单预览

既有账户适配器提供 `POST /accounts/<id>/orders/preview`（相同 client/account 身份合同；请求类型见 `packages/contracts/src/native-order-preview.ts`）。它直接用 Nautilus `OrderFactory` 构造 STOP_LOSS (`StopMarket`)、TAKE_PROFIT (`MarketIfTouched`) 或 BRACKET，再调用官方 Hyperliquid 校验；价格和数量必须符合已配置合约的原生精度与步长。Bracket 使用 IOC Limit 入场、OTO 关联和 OUO 减仓退出腿，不另写触发或订单组引擎。

此入口仅预览：独立临时 OrderFactory 不消耗账户策略订单 ID、不写 Cache、不提交订单，也不启用自动保护。当前 Hyperliquid 官方适配器在 entry 成交后激活暂存子单，不得解释为交易所原子 OCO；Sandbox 模拟也不是实盘支持证明。预览不检查余额或市场接受性，不保证止损成交价。既有目标和资金退出默认仍按原 IOC 路径执行，不自动附加保护单。

当前新 paper bracket 返回 `account_native_paper_bracket_recovery_unverified`，live bracket 返回 `account_native_live_brackets_not_supported`；capabilities 均标为不支持。未完成父成交/子腿缩量、出口成交/关联撤单等中间恢复窗口的完整验证，不得将预览、协议 ACK、Sandbox 或稳定态重启测试宣传为实盘保护单支持。普通无 bracket 的 IOC 路径不变。这是 QZ 接线的真实边界，不修改或限制 Nautilus 本体。

现有协议中的 `nativeBrackets` 字段及 `observation cancel-protection` / `POST /api/v3/observations/:id/cancel-protection` 不代表新 bracket 可提交。原保护订单的查询与取消沿用真实账户绑定和官方订单状态；不得伪造恢复、自动放行或清空原缓存。

## 原生绩效与可选价格基准

账户收益报告额外注册 Nautilus Calmar、95% VaR、95% Expected Shortfall 与 Ulcer Index；既有普通账户沿用 252 天年化，现货日报沿用 365 天。不可计算项保持空值及原因，不补零。Observed Spot 的 hold/rebalance 比较另外输出以真实 hold 账户为基准的原生相对统计。

既有策略参数/科学请求的 `simulationSettings`（原生请求中的 `settings`）可配置 `price_benchmark`，值为一个完整的 `NativeBarSelectionV1`：`schema_version`、单元素 `bar_types`、十进制字符串 `event_start_ns` / `event_end_ns` / `decision_cutoff_ns`，以及可省略的 `maximum_rows`。不配置则不计算价格基准；这不是对旧版本科研请求的兼容承诺。

基准读取本次任务已授权的同一 catalog，不下载或生成市场价格。选择窗口须包含首个收益日起点的闭合价格和各日终点的闭合价格；窗口可以比策略交易窗口早一根 bar。仅支持已知右开时间语义的 INTERNAL 时间 bar，报价币种必须等于报告币种，且各 UTC 日界必须有真实闭合价格和完整账户收益；不匹配时报告 `BENCHMARK_*` 原因，不转换币种、不前填、不自动换用其他标的。输出明确标记为价格收益基准，排除费用及资金费率，采用零无风险利率，不等价于实际买入持有账户。统计直接调用 Nautilus 原生 Alpha、Beta、Information Ratio、Tracking Error、Treynor 和上下行捕获率。

价格基准的实际读取截止取配置截止与当前 simulation.selection.decision_cutoff_ns 的较早者，避免同一策略参数跨验证/留出窗口复用时读取未来才到达的历史 bar。比较必须覆盖当前请求内全部完整 UTC 日，缺首日、尾日、中间日、重复日或未知账户收益均不可用；部分首尾日不参与比较，不能用原生快照环形缓冲区的残留尾段冒充完整窗口。

## 双路研究评级的当前接线

`native_strategy_logic_review` 在已配置策略执行前创建官方 Pi Durable 子 Task。子 Task 用官方 pi-ai 的独立上下文审阅声明的经济机制、前视风险、自由度、参数依据和执行假设；对上游策略可显式传入需审阅的 `strategies` 与预声明完整请求列表，省略时只审阅已注册策略，不读取主对话或原生回测结果，也不声称检查过未提供的源码。结果绑定原始策略请求、配置 Task ID、模型与评级规则版本；模型缺失或响应无效按真实失败处理。逻辑星级仅为研究判断，不是收益概率。

`native_strategy_assessment_report` 创建另一个官方子 Task，读取同一配置的真实逻辑审阅 Task 及原生执行 Task。此入口读取显式回测及原生敏感性事实；历史 WF 仅作为原始历史。未运行时数据状态为 `NOT_RUN`，缺少事前方法或必要结果时综合星级为 `null`，不以模型建议覆盖缺失事实；完整局部方法的评级范围见下文。历史原生回测及探索参数扫描不得冒称独立验证。旧报告仍通过原 Task ID 留存，改参后的综合报告不复用不匹配的逻辑评级。

这是完整自动双路流程的首个接线切片，不是已完成的数据验证产品；现有普通研究回测仍保持原收尾方式。审阅工具可在同一 Conversation 的后续 Submission 调用，不增设交易门禁、审批或资金权限。

自动 walk-forward 分折与训练择优执行已退役，不提供新调用。历史 WF Task、原请求和原始报告仍可只读，并沿原回执恢复或取消未决执行；历史大型表继续用有界流式解析，不能因入口退役退回全量读取。

新研究由 Pi 在见到结果前声明完整显式原生请求及训练/验证用途，逐次配置并调用同一原生回测。每次执行使用各自明确窗口、账户与上游配置；QZ 不生成折、不选胜出候选、不重新计算统计。原始事件窗口、可用时间边界、费用与原生报告保持原样。

事前逻辑审阅可在 validationMethod.requests 中声明多个完整请求。后续每个回测按原始请求、catalog、target 和事前 Task 时序关联原预审，不因逐次配置的 Task ID 改变就丢失已声明覆盖，也不把未声明请求或旧 WF 形状纳入覆盖。综合报告保留本会话全部真实尝试与失败；跨会话试验完整性未经核查。

事前逻辑工具的 `validationMethod` 记录显式完整请求、用途及邻域/费用假设；这些声明不是 QZ 分折或择优参数。敏感性工具仅按明确场景调用原生回测，原始结果、失败与每次调用均保留原 Task，取消和恢复复用原回执，不把同窗口敏感性冒充独立样本外。

综合审阅读取真实 Task，核对事前候选/方法与实际执行，并呈现原生统计、显式窗口与敏感性、交易笔数及已记录试验。可用证据齐备后由独立 Pi 子 Agent 判断是否足以给出 0–5 星，逻辑与数据分开、综合不取平均；材料不足或 Agent 认为不充分时保持 `null`。有效评级明确标记 `RATED_WITH_DISCLOSED_LIMITATIONS`，只评价本会话已记录方法，不是概率、全面防过拟合认证或交易门禁。CPCV、跨会话试验历史和独立源数据审计未执行时如实披露，不机械禁止有限范围研究判断。官方统计保留原值，IEEE754 十六进制只作展示解码，缺失/非有限数不补零。

审阅绑定同时比较官方归一化请求、实际 catalog 和原生执行 target。省略与官方等义的 null 不构成新策略，不能绕过事前结果检查；相同请求在不同 catalog/target 的结果不拼接为同一研究证据。这里记录的是执行来源，不是数据内容独立审计、不可变快照或证明。报告保留原生 `run.outcome`、notes 与数据行数；官方 `stopped` 是真实停止结果，不冒充完整验证。样本范围按本次原始窗口与实际数据记录说明，不把派生缓存的更长日期范围当原始行情覆盖；USDT/USDC 不由胶水默认等值。

完整原生研究使用同一 Pi 工具循环：先 `native_strategy_brief` 读取已注册的原始请求与来源，再由 Agent 在未见结果时声明候选/方法并逻辑审阅，随后执行预声明显式窗口的原生历史回测、邻域/成本与综合报告。`native_strategy_backtest` 默认保存真实历史结果但不强制结束整个研究；单次独立回测可显式 `finishResearch: true` 收尾。此连接不另建调度器，不运行新策略DSL、不推断币种换算，也不下实盘订单。没有已注册的原生请求、catalog、target 或必需账户/费用假设时仍须取得真实输入，不能冒充已自动完成数据登记。

费用/邻域证据须对应实际执行的显式策略配置，不得以策略 A 的结果冒称覆盖策略 B。原始费用、变更场景与 Task ID 原样保留，覆盖不足如实披露；已运行过的配置不能重新登记后冒充事前审阅。

最大化训练 Sharpe 只用于真实训练候选比较，不证明样本外或年化目标；报告保留官方收益口径、每折可用时间窗口、数据行数、成交数及选择历史，让 Agent 评价短样本与选择偏差，不把高值当稳定年化绩效。

研究详情的 `assessments` 直接读取原 Pi 审阅 Task，返回真实状态、原始报告和 `CURRENT`/`HISTORICAL` 配置标识；比对原生请求、catalog/target 与配置 Task，不从 Agent 文本重算星级。现有 CLI `research get` 原样返回同一字段，Web 显示三路星级、未评级、限制和历史。证据链接 `/api/v3/research/:id/evidence/:taskId` 只读本研究原生回测/WF及审阅 Task，沿用 owner 认证；不暴露私有工具任务或跨研究记录，不建立第二报告存储。

历史回测已保存后，后续验证仍由官方活跃 Task/Submission 决定是否可停止，不能因 `COMPUTED` 隐藏停止入口。用户停止，或历史结果保存后模型调用失败（`model_error`），均可继续同一研究，历史结果移入原 `previousNativeResults`，原 Task 不重写。新提交前核查本会话全部原生回测/WF/敏感性 Task 及原远端回执，未决只读原回执，不重新 POST、不把取消/未知变成成功；新提交获得新的停止任务，避免重复使用已完成的停止命令。

## 显式固定规格研究情景

原生策略请求默认按窗口起点读取当时已进入 catalog 的 instrument 定义。可显式使用 `instrument_definition.mode=STATIC_SCENARIO` 并指定目录中原始定义的精确 `ts_init_ns` 与 `ts_event_ns`，将该版本作为整个窗口的固定模拟规格交给官方 `BacktestEngine.add_instrument`。此路径不改原定义时间、不回填历史、不改行情数据、不重写上游，也不在情景中混入其他规格更新。

每个显式窗口的原生回测结果均携带实际使用的定义时间和 `HYPOTHETICAL_STATIC_RULES_NOT_HISTORICALLY_VERIFIED` 口径。时间外测试只表示该假设下按时间分离的结果，不验证历史规格，也不构成实盘证据。费用、币种和资金费模型仍须分别明确；无隐式 USDT/USDC 转换。默认严格路径的 `CATALOG_DEFINITION_AVAILABLE_AT_WINDOW_START` 仅描述目录可用时间，不冒称独立历史来源审计。

原生请求的 `instrument_definition` 缺省为严格目录时点定义；只有显式 `STATIC_SCENARIO` 并给出真实目录定义的精确 `ts_init_ns`/`ts_event_ns` 才表示“假设这一规格整个窗口不变”。不改目录、行情或定义时间，不将未来定义偷偷重标历史。每个原生结果必须保存定义依据，严格模式仅证明目录定义在窗口开始已可用，不是独立源准确性审计。

STATIC_SCENARIO 原样贯穿配置、各显式回测与成本/邻域候选；原生指标不变。可作有限条件研究评级，状态为 `RATED_CONDITIONAL_STATIC_SCENARIO`、结论为条件情景，始终披露历史规格未核实，不能解释为真实历史可交易性、实盘证据或自动交易许可。报告/API/Web和历史扫描保留定义原时间与scope，不能混用严格历史与情景依据。

历史扫描只读展示原生 Task 及原结果状态：stopped 仍为真实停止，不重标完整验证，不再计算排名或生成热图。恢复、查看与停止均不得重新运行旧候选；原始指标、时间与数据口径保持。

原生策略结果的 `equity_performance` 只读官方 PortfolioAnalyzer 的 portfolio_returns（原始权益快照），直接调用官方 CAGR/Sharpe 的252与365两种频率，完全独立于可能回退到持仓收益的 canonical 默认统计。官方权益不可用时保留原因，不以持仓收益替代或隐式转换币种。原始快照UTC日期数、收益日数、未定价快照和时间范围同时展示；官方跨日延续不是每天实际观测，短样本CAGR只是外推，不是已实现年收益。WF可显式选择 `EQUITY_PERFORMANCE` 与频率，扫描以独立权益指标ID保持同名canonical值分离；无需新增交易门禁。
盘口快照不等于逐事件增量数据；报告必须披露已知档位、采样频率、遗漏空簿/异常档位及carry/checkpoint语义，不把快照撮合结果宣称真实高频可成交性。

原生策略回放在既有 Rust job 内使用官方 `BacktestNode(chunk_size=4096)`，不新增服务或回放引擎。catalog 适配只提供精确选定的原始初始定义、严格模式后续定义及原窗口数据；静态情景不混入定义更新。数据 `start_time` 用于可用时间过滤，node 的 `start` 只映射显式 `available_start_ns`；缺省仍由官方引擎从首个完整回放事件推导。canonical 的固定 `run_config_id` 为 `native-strategy`，原始账户/权益/订单/时间字段不改写。

按公开 catalog 文件时间区间将相交、同端点及连锁重叠文件归为一组，耗尽并释放当前组后再开下一组；组内使用官方 `optimize_file_loading=false` 的逐文件合并。不得用目录优化替代：已验证其在跨文件同 `ts_init` 的 CLEAR/ADD/F_LAST 帧中可能交错行。F_LAST 聚合、跨 selector 时序、同时间块、撮合与统计全部由官方 node/engine 处理。可用内存仍取决于最大重叠文件组、同时间数据量及订单历史，不保证固定 RSS。

原生策略在启动 node 前复用既有有界文件读取完整计数并校验帧尾，`data_rows_by_type` 仅表示预运行校验扫描当时观察到的原始行数，不是回放消费数或一致目录快照。报告现有 notes 同时披露实际reader读取行数（含预取）及目录一致性未验证；Parquet仅支持Latest，read-only不阻止其他导入者，组发现后添加的文件可能包含或不包含，替换文件即使行数不变也可能改变内容。不同fold/candidate不保证同一目录内容，不以相同行数冒充一致输入；不新增冻结、哈希或交易门禁。测试中的启动即停得到选中9行、预取6行、回放0帧，三者口径不同。具体 native_strategy 集成测试与相同 canonical 对照覆盖真实上游策略执行、费用、资金率同时间排序、显式/缺省起点及静态/历史定义；临时合成数据不充当历史交易或盈利证据。

本次顺序独立进程压力测试使用临时合成648份原生Parquet、2653560行delta及884520完整帧，官方OBI dry-run：原预载路径峰值648732 KiB、回放35.91秒；延迟组官方node含选中行预计数峰值160028 KiB、69.59秒。完整canonical结果仅显式run标签不同，资金与时间字段未删改。此样本内峰值约降75%、时间约增94%，不外推真实27天数据或实盘效果。相关定向验证使用jobs=1、debug=0；因共享构建空间不足，最终定向重试关闭incremental缓存，不扩展为额外验证流程。

本轮科学减法已删除自有 BAR 容量估算、账户 sizing、目标混合、资金曲线规划、旧目标回放、组合数值优化和库存估值执行入口。历史合同和原始资金记录保留，不代表旧算法可执行。界面只展示上游统计与原始权益，不重算年化、收益或回撤；原始回撤负值保留，越接近 0 损失越小，API 数值排序不变，界面首次按降序显示。
## 用户 Rust 原生策略的静态接线

用户策略作为独立 Rust crate 实现官方 Nautilus `Strategy`，不属于 QZ 的科学计算实现。请求可使用 `strategy: { kind: "REGISTERED", name, config }`；名称和配置由用户 crate 的注册函数解释，QZ 不维护策略名称白名单、不实现策略 DSL、不推断参数或重写算法。默认发行 `job` 未注册用户策略时明确拒绝，不回退到四个示例。

现有 job 库公开 `native_strategy::registry::StrategyRegistration`：函数接收官方 `BacktestEngine`、请求的 `InstrumentId`、名称及原配置，构造用户策略并调用官方 `engine.add_strategy(...)`。用户 crate 的二进制入口调用 `job::cli::main_with_registration(Some(register))`，复用相同 CLI、日志隔离、单次回测 与结果传输；每个回测均创建独立原生实例。用户 crate 依赖此仓库 `apps/job` 及与 Cargo.lock 固定 revision 完全相同的 Nautilus crate，使用统一 precision feature；新策略类型需要编译部署，已编入策略的配置变更不要求重新编译。此机制不是 `.so` 动态加载，也不增加 Python 环境。

部署沿用现远程 native 服务的 `QZ_JOB_BINARY` 和 `QZ_TICK_JOB_BINARY`。原生策略 走后者，未配置则使用前者；必须将相应路径指向新构建的用户 runner。既有 `runtimes/native/build-native-image.mjs --job-binary /path/to/user-runner --output-dir /outside/new-directory` 支持打包实际二进制并生成其原生 manifest；若部署配置了 `QZ_NATIVE_MANIFEST`，必须一起安装对应新产物，旧 manifest 与新二进制不匹配会拒绝启动。保留 catalog、任务回执、账户缓存与登录数据，不删除存储完成升级。

此接缝放开的是策略 Rust 类型，不声称现有数据/账户接线支持 Nautilus 所有用法：请求仍为既有单 `instrument_id`、selectors 和 venue 合同；多标的、其他市场适配与实盘生命周期需另按原生能力接线。不得把构造 API 完成等同于某个用户策略已编译、部署或回测成功。

用户 crate 若需多标的、多 venue 或完整引擎配置，可实现 `native_configured::ConfiguredRunFactory`，并调用 `cli::main_with_native_hooks(single_registration, Some(factory))`。工厂接收实际绑定的 catalog 根目录、用户注册名与原始配置，返回 `ConfiguredNativeRun { config: BacktestRunConfig, install }`；`config` 由用户代码调用官方 typed builders 构造，`install` 在新建的官方 engine 上安装一个或多个原生 Strategy。工厂必须对未知注册名返回错误，默认 job 未注册工厂明确失败。typed工厂可先安装官方Actor/Controller，再由上游生命周期动态创建策略，QZ不在运行前强制策略数量；输出原生结果，不将空结果或无交易推断为策略验证成功。单策略REGISTERED入口仍约定立即安装Strategy，不能将该窄契约当作完整配置限制。

对应 `native-configured-backtest --catalog ...` 输入为 `{ schema_version: 1, name, config }`，仍在现 job 进程/远端 worker 内执行。QZ 不复制多 venue/data/engine 的字段 schema，也不覆盖用户选择的 `DataEngineConfig`、`emit_quotes_from_book`、官方流式块大小或撮合参数。运行前检查工厂返回配置中每个 catalog 的真实路径仍在本次允许根目录下，当前仅接本地 file 协议；这项路径检查不是对任意用户 Rust 代码的沙箱隔离。工厂不得另写数据源或算法来绕过官方 API。

返回 `native_results` 为官方 `BacktestResult` 原序列化值，不重新年化、打分、择优或聚合；当官方配置 `dispose_on_completion=false` 时附原 `canonical_result`，启用官方释放则明确 `OMITTED_BY_DISPOSE_ON_COMPLETION_CONFIG`，不修改该选择。官方 `raise_exception=false` 可能产生空 results；空数组不是已完成完整窗口或策略有效性证明。此入口不自动生成现单标的 `equity_performance` 包装，不把两种报告形状混用。现Pi任务与远端回执仍拥有执行状态，不能把重新构建node称为从中途checkpoint恢复。

多标的/多 venue 配置接口与实际策略、数据源覆盖须分别验收；它不是LiveNode账户策略注册接口，不解除现账户策略生命周期限制，也不启动模拟盘/实盘。用户策略的官方save/load、订单归属和账户恢复仍需另行接线与验证。
