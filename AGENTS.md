# QuaZonai Agent 治理

本文件是开发 Agent 的最小治理入口。它定义执行顺序、事实源指针、架构边界和完成门槛；不复制完整 API、数据表、产品背景或 Skill 内容。

## 1. 事实源与读取顺序

1. `DESIGN.md` 是 QuaZonai 唯一完整的产品与架构事实源。
2. `OPERATIONS.md` 是用户运行视图，不得改写 `DESIGN.md` 的状态、风险或交易语义。
3. `CLI.md` 是本地 CLI、MCP Gateway 和外部 Agent Skill 的实现展开，不得引入新的产品事实；冲突时先更新 `DESIGN.md`。
4. `skills/quazonai/SKILL.md` 是外部运行 Agent 的工作流，不是 QF 内置 Agent runtime，也不是权限或风险事实源。
5. `README.md` 是运行入口和当前状态摘要，不是第二份设计文档。
6. 代码、配置、测试和运行结果是实现证据；它们不能静默改写文档事实。
7. 法律文件按各自约束处理。

开始任何跨层工作前，先读取 `DESIGN.md` 中与任务相关的章节。若目标行为、状态、字段、接口、插件能力、MCP Tool、OAuth scope、风险规则或验收标准没有明确记录，先补文档再写代码。

### 何时必须读取设计全文

- 修改产品流程、Research、Approval 或 Deployment 生命周期；
- 修改 Nautilus ownership、插件安装/激活/停用/卸载、runtime bundle、Strategy 合同或数据导入；
- 修改本地 CLI、MCP Gateway、OAuth、Tool/Resource、Tasks、Artifact upload 或 Skill；
- 修改 Polymarket、风险、Recovery、Heartbeat、Stop/Restart 或真实资金边界；
- 修改数据库、持久卷、运行拓扑、依赖、API 资源或错误语义；
- 进行仓库级删除、重构、发布或 live 验收。

## 2. 不可突破的边界

### 2.1 交易与产品边界

- NautilusTrader 是唯一交易内核；QF 是控制平面和产品层。
- 订单、成交、仓位、NAV、撮合、费用、市场数据和交易场所同步不在 QF 重建第二套事实账本。
- 券商执行连接只使用官方 Nautilus adapter；没有官方 adapter 的执行场所不支持。
- `parquet_l2` 只负责固定 L2 Parquet 到 Nautilus Catalog 的导入，不是自定义券商协议客户端。
- QF 无前端、无业务用户/workspace/RBAC、无内置 LLM/Agent runtime/model provider、无 Paper scheduler。
- Plugin 和 Strategy 都是可信代码；独立进程是生命周期和故障边界，不得描述为恶意代码安全沙箱。
- 跨 instrument 的中央逐单风险必须沿设计指定的 Rust execution seam 实现；不使用 C++，不复制 Polymarket 协议客户端。
- 不新增应用级 SHA、hash、checksum、digest 或 fingerprint 字段、文件、状态判断或完整性流程。
- 不预埋公共插件市场、HA、Redis、密钥轮换、旧业务迁移框架或其他未批准机制。

### 2.2 运行时插件边界

- 数据源和执行连接必须引用具体 `plugin_release_id`；不得只依赖可漂移的 `plugin_id` 或全局 site-packages。
- 第三方插件只在独立 validator 或 runner 子进程中发现和加载；API、finite worker、live supervisor 和 MCP Gateway 长进程不得导入插件代码。
- 插件使用操作者上传的 wheel 集和隔离、不可变 runtime bundle；不接受 sdist、editable install、任意 Git/URL 安装或运行时源码编译。
- 动态插拔通过 release 状态机、drain、子进程退出和新 bundle 完成；不得使用 `importlib.reload()`、修改 `sys.modules` 或在运行中的 TradingNode 内原地替换 adapter。
- 插件升级必须 side-by-side；已有 Run、Data Source、Execution Connection 和 Deployment 不得被静默切换版本。
- 普通停用只阻止新绑定并允许已有引用 drain；强制卸载必须先取消未启动作业、标准 Stop 受影响 Deployment、等待 Runner 退出，再删除 artifacts/bundles。
- `READY` bundle 不得原地修改；Python/QF/Nautilus 版本变化使旧 bundle `STALE`，不得用于新的 Run 或 generation。

### 2.3 CLI 与远程 MCP 边界

- 本地人类只通过官方 `quazonai` CLI 或 loopback Core API 操作。
- 远程 AI Agent 只通过官方 MCP Streamable HTTP Gateway 和 `skills/quazonai/SKILL.md` 操作。
- **不得实现 SSH transport、普通 Shell、forced command、端口转发或自定义 JSONL 隧道。**
- Core API 继续只通过宿主 `127.0.0.1:8000` 和内部网络可达；不得为 Agent 公开 `/api/v1` 或增加 `--allow-remote-api`。
- MCP Gateway 是独立 resource server，只公开 `/mcp`、OAuth protected-resource metadata 和受限 Artifact upload endpoint。
- Gateway 不访问 PostgreSQL、Docker socket、Plugin/Catalog/Report/Wallet Volume。
- Gateway 不提供通用 HTTP proxy，也不把 MCP access token 传给 Core API、Polymarket 或其他下游。
- MCP HTTP 授权使用 OAuth 2.1、RFC 9728 discovery、精确 resource/audience 和 scopes；这不是 QF 业务用户模型。
- Gateway 必须校验 token signature、issuer、expiration、audience/resource、client identity 和 scopes。
- Gateway 必须校验 Host 和存在的 Origin；CORS 不允许 `*`。
- Tool list 按 scope 过滤；客户端提示和 Tool annotations 不能代替服务器权限校验。
- Human CLI 和 MCP Tool 必须映射相同领域行为；不得建立 Agent 专用自动化后门。
- Skill 只能使用当前 MCP `tools/list`、Resources 和 Tool Result；不得根据历史聊天假定命令存在。
- Skill、MCP Tool 或 Elicitation 不得请求、读取、显示或转发 OAuth token、API Key、Private Key、Wallet Secret、Password 或支付凭据。
- Credential Secret 写入/读取、Approval approve/reject、Force Plugin Remove、Live Canary、Master Key 和数据库破坏操作永久 human-only，不能通过 scope 解锁。
- `deployment.stop` 只有用户明确要求或设计规定的紧急风险策略触发时才可由 Agent 调用；必须明确 Stop 不强平已有仓位。
- MCP Tasks 只能映射 QF 已持久化的 job/run/deployment；业务事实不得只存在 MCP session 或 task store。
- Artifact 上传使用 HTTPS 两阶段、精确 size、offset 和生命周期；禁止把大型文件 Base64 放入 MCP JSON，也禁止服务器抓取任意远程 URL。

## 3. 文档优先

涉及以下任一变化时，先更新 `DESIGN.md`，再同步 `OPERATIONS.md`、`CLI.md`、README 和 Skill：

- 产品能力、Research 状态、Approval 条件或失败语义；
- Plugin package contract、capability、release/bundle 状态或动态插拔语义；
- CLI command、MCP Tool/Resource、OAuth scope、Artifact、Task 或 human-only gate；
- Strategy 合同或数据格式；
- API、错误码、数据库模型或持久化 ownership；
- 风险上限、Reservation、Reconciliation、Heartbeat、Recovery 或真实资金边界；
- 运行拓扑、依赖、部署入口和验收标准。

顺序固定为：

```text
发现缺口或冲突
→ 更新 DESIGN.md
→ 同步 OPERATIONS.md / CLI.md
→ 检查 README.md / AGENTS.md / Skill 指针
→ 实现
→ 运行最窄有效验证
→ 独立复核
→ 汇总证据
```

如果实现与设计冲突，停止相关代码工作并报告冲突。不得用代码事实反向覆盖文档事实，也不得让 README、OPERATIONS、CLI、AGENTS 或 Skill 变成竞争事实源。

## 4. Ponytail 实现纪律

每次编码、重构、依赖选择和 Review 都使用 Ponytail full：

1. 先判断需求是否必须存在；推测性需求删除。
2. 查找现有实现；能复用就不新增。
3. 优先 Python 标准库、平台能力、官方 MCP SDK 和已批准依赖。
4. 做最小局部改动；删除优先于添加，少文件、少状态、少抽象。
5. 一个实现不建立接口、工厂或配置层来假装可扩展。
6. 插件框架只实现已批准的 runtime release/bundle seam，不扩张为应用市场。
7. CLI 是薄客户端；不复制 API 领域逻辑。
8. MCP Gateway 只实现标准 MCP、OAuth resource server 和 Artifact bridge，不扩张为通用 API Gateway。
9. Skill 只编排 MCP Tool，不把模型推理变成新的业务状态机。
10. 能下沉到 Nautilus、Python packaging、uv、MCP SDK、OAuth 标准或 PostgreSQL 原语的行为，不在 QF 重写。
11. 真实简化上限用一行 `ponytail:` 注释说明升级条件；普通代码不添加口号式注释。

Ponytail 不得删掉真实边界的校验、错误处理、Secret 保护、数据一致性、插件 drain、OAuth audience、scope、idempotency、precondition、human handoff 或用户明确要求。

## 5. 分层与 ownership

修改前先画出调用和数据流，确认真正拥有者：

- API：请求校验、控制面状态、统一错误、SSE、最终幂等和领域校验；
- QF orchestration：Research、Strategy Version、Experiment、Run、Approval、Deployment、Universe 和控制事件；
- CLI：本地 human command、输出、watch 和 Secret-safe 输入；
- MCP Gateway：MCP protocol、OAuth token validation、scope-filtered tools、Resources、Tasks、Artifact bridge 和 audit；
- Skill：外部 Agent 工作流、读取优先、Tool 选择和 human handoff；
- Plugin Manager：release、artifact、activation、drain、remove、descriptor snapshot 和 bundle 编排；
- Plugin Validator/Builder：短进程安装 wheel、加载 entry point、生成 schema、构建 bundle；
- Runners/Workers：有限作业和独立 Nautilus 进程生命周期；
- Nautilus：Catalog、回测、风险、订单、成交、仓位和 Reconciliation；
- PostgreSQL：QF 控制面、插件、job/event、幂等、Optuna 和风险最小投影；
- 持久卷：Plugin Release/Bundle、Artifact staging、Catalog、Report 和 Import staging。

任何同时修改两个 ownership 层的工作，都必须在设计中说明边界和失败路径。

## 6. 关键路径纪律

### 6.1 MCP 与外部 Agent

- 实现 MCP `2025-11-25` Streamable HTTP；不实现旧 HTTP+SSE 作为主路径。
- 使用官方 MCP SDK，并固定经过验证的稳定版本。
- `tools/list`、input/output schema、structuredContent 和 Tool annotations 必须有合同测试。
- OAuth resource metadata、401 challenge、PKCE、Client Credentials、scope challenge 和 audience validation 必须用真实 HTTP 测试。
- Access token 每次请求校验；MCP session ID 不得被当作认证。
- Mutation Tool 必须带 idempotency key；同 key 不得用于不同 normalized request。
- 更新类 Tool 必须携带 current state/revision/version/generation 等前置条件；冲突后重新读取，不盲重试。
- 高影响 Tool 必须先获得绑定 principal/target/version 的 impact token。
- Human-only Tool 不得出现在 tools/list；直接调用也必须 hard-deny 且无副作用。
- Long-running Tool 无 Tasks 客户端也必须可用；支持 Tasks 时必须绑定 OAuth principal。
- Gateway 断线、重启或 task 过期不得改变底层 QF Run/Deployment 事实。
- Form Elicitation 不得请求敏感信息；Approval 和 Secret 返回 human handoff。
- Artifact 上传按 offset 流式处理，不全量入内存；中断和过期清理 staging；不生成应用级 hash。

### 6.2 运行时插件

- 只接受 PRIMARY/DEPENDENCY wheels 和唯一 `quazonai.plugins` entry point。
- API、worker、supervisor、Gateway 主进程不得导入第三方插件。
- 安装只使用离线、本地 wheels 和预编译 binary；禁止远程解析、sdist build、editable install 和 Python 自动下载。
- Descriptor import 不得启动网络、后台线程或交易节点。
- Release、Artifact、Bundle 使用数据库 UUID 和显式关系，不建立内容 hash。
- 新 release 激活时旧 release 进入 `DRAINING`；已有资源固定旧 release。
- 强制 Remove 必须由独立 job 执行标准 drain/Stop。

### 6.3 Strategy、回测和优化

- 只接受设计规定的单个 `.py` 合同和大小上限。
- 回测只走 `BacktestNode + ParquetDataCatalog`。
- Run 和全部 optimization trials 固定同一 runtime bundle。
- 优化保持 100 trials、最多 4 个独立 BacktestNode 进程和 Optuna 独立 schema。
- Pareto 自动选择使用固定确定性算法；不得改成人工挑选。
- Holdout 只运行一次；失败 candidate 不得被同一 Holdout 上的下一个 candidate 替换。

### 6.4 Parquet 和外部交易

- L2 Importer 从固定 bundle 启动，分批读取并严格校验 decimal、snapshot 和事件连续性。
- 导入失败不注册部分 Catalog，终态清理 staging。
- Polymarket 订单语义遵循官方 adapter；不增加自定义 stop/trailing/OCO。
- Polymarket Plugin 或 Nautilus 升级必须重新验证 factory、Risk Decorator、Recovery 和 Canary。
- 真实资金路径保留 production read-only preflight、最小金额 canary 和独立复核。

### 6.5 风险和恢复

- 25 pUSD 是具体 InstrumentId 的 Nautilus 原生逐单限制；100 pUSD 是 QF funder gross reservation。
- Reservation 在官方 client submit 前完成；无法证明为减仓的订单按全部 debit 计入。
- DB、projection、owner、heartbeat、runtime bundle 或 reconciliation 不完整时，增加风险 fail-closed；cancel 仍可用。
- Recovery generation 无 Strategy、无 heartbeat；Armed generation 再次对账并激活风险后才加载 Strategy。
- Plugin switch 和 Universe 变化通过受控 Restart；新 instrument/bundle 未生成精确风险映射前不得交易。
- `DRAINING` release 可服务原 Deployment 自动 Recovery；`REMOVED` release 不得被同 plugin_id 其他版本自动替代。

## 7. Agent 编排

QuaZonai 开发使用主 Agent + 执行子 Agent。

主 Agent：

- 读取事实源并界定工作包；
- 拆分文档、实现、测试和复核依赖；
- 指定输入、输出、边界和验收条件；
- 汇总结构化报告，处理冲突和返工；
- 只依据独立证据收口。

执行子 Agent 负责工作包内文档、代码、测试、静态检查和运行验证。关键改动的实现与独立验收不能由同一 Agent 承担。

MCP 关键工作包至少拆为：

- Protocol/Tool/Resource schema；
- OAuth discovery、token verification 和 scope；
- shared command mapping、idempotency 和 precondition；
- Tasks/notification；
- Artifact upload；
- Skill E2E；
- 独立 live boundary review。

子 Agent 报告必须包含：

- 修改文件和行为摘要；
- `DESIGN.md` 一致性；
- API/schema/依赖/持久化影响；
- 执行的检查、结果和失败原因；
- MCP/OAuth/Tool/Artifact 证据；
- 未验证项、风险和阻塞；
- 是否满足完成标准。

## 8. 验证纪律

每次检查前写清楚：

1. 它要发现的具体失败；
2. 失败后下一步如何改变。

验证顺序：

1. 文档和路径自洽；
2. 受影响模块最窄单元/行为检查；
3. 跨边界集成；
4. 只有边界扩大或设计明确要求时运行更宽检查。

以下不能只靠 mock：

- OAuth discovery、PKCE、audience 和 scope；
- Origin/Host validation；
- MCP Streamable HTTP 和 reconnect；
- Tool list 过滤和 human-only hard deny；
- Idempotency、precondition、task isolation；
- Artifact 大文件流式上传和 resume；
- Plugin wheel install、entry point、native extension 和 bundle isolation；
- PostgreSQL locks；
- Nautilus factory、Recovery、Heartbeat、unknown submit 和真实订单状态。

Review 只报告可达真实缺陷，说明路径、影响和最小修复；不要用未来可能性制造工作。

## 9. 文档任务完成标准

- `DESIGN.md` 仍是唯一产品/架构事实源；
- `OPERATIONS.md` 只描述用户运行视图；
- `CLI.md` 只展开 CLI/MCP/Skill；
- `README.md` 只保留入口、状态、Quick Start 和文档链接；
- `AGENTS.md` 只保留治理规则；
- Skill 不复制完整 API 和风险事实；
- 文档不再出现 SSH 作为远程 Agent 传输；
- MCP OAuth 明确是 transport authorization，不是业务用户域；
- 文档没有承诺未实现的 ready 状态；
- 路径、端口、scope、Tool、Plugin 状态和术语一致；
- 文档检查和 `git diff --check` 通过；
- 独立 Documentation/Architecture Reviewer 无阻断意见。

## 10. 代码任务完成标准

交付前必须确认：

- 先更新受影响事实源；
- 变更位于正确 ownership point，未创建平行交易内核；
- MCP 没有退化为 raw Core API、通用 proxy、SSH 或 Shell；
- OAuth token audience、scope、no-passthrough、Origin/Host、rate limit 有验证；
- Human-only Tool 不可被配置解锁；
- Idempotency、precondition、impact token、Tasks 和 Artifact lifecycle 有测试；
- 插件动态插拔未退化为全局 site-packages、长进程 reload 或静默版本漂移；
- 没有新增前端、业务 auth、多租户、应用级 hash、公共插件市场或未批准抽象；
- 外部系统部分区分“已验证”“未验证”和“被阻塞”；
- 实现报告和独立复核报告均已提交。

未满足任一项时，状态只能是未完成或部分完成，不得标记为 release-ready/live-ready。