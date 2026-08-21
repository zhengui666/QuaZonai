# QuaZonai 用户运行操作模型

> 本文件是 [`DESIGN.md`](DESIGN.md) 的**用户运行视图**，用于回答系统实际运行时“谁在什么节点做什么”。
>
> 本文件不定义新的产品状态、API、交易语义、风险限额或权限模型；如与 `DESIGN.md` 冲突，以 `DESIGN.md` 为准。
>
> 当前 V1 是 API-only、单机、单操作员系统，没有用户账号、RBAC 或多方会签。下文的“角色”是同一操作者在不同节点承担的**责任帽子**，不是系统内已经实现的权限组。

---

## 1. 运行角色

### 1.1 工作台管理员

负责让工作台具备可用能力，而不是进行策略研究。

主要操作：

- 检查系统 health/readiness；
- 上传、验证、激活、停用、升级和卸载插件 release；
- 预热或检查 runtime bundle；
- 处理 `FAILED`、`STALE`、`PLUGIN_IN_USE` 等插件状态；
- 决定是否执行强制插件卸载；
- 检查 PostgreSQL、worker、supervisor 和持久卷是否可用。

### 1.2 数据与连接管理员

负责把外部数据、凭据和交易连接变成可被研究或实盘引用的资源。

主要操作：

- 创建和更新 credential set；
- 创建 data source；
- 创建 execution connection；
- 运行真实配置 preflight；
- 上传历史数据并发起导入；
- 检查 Catalog dataset 的时间范围、instrument 和导入状态；
- 在插件升级后显式验证或重新创建 credential、data source 和 execution connection。

### 1.3 量化研究员 / 策略作者

负责研究判断、策略内容和实验设计，不负责手工挑选系统已经确定的 Pareto candidate。

主要操作：

- 创建 Research；
- 完成七个研究章节；
- 上传新的 Strategy version 和默认配置；
- 选择 dataset、训练区间、Holdout 区间和随机种子；
- 发起 Experiment；
- 阅读 optimization、Holdout 和 Nautilus reports；
- 根据失败结果修改研究章节或 Strategy version；
- 把满足研究要求的结果提交审批。

### 1.4 资金与风险审批人

负责所有会改变真实资金暴露边界的人工决策。

主要操作：

- 审阅 Holdout 结果、研究风险章节、插件版本、runtime bundle 和交易连接；
- 批准或拒绝首次 Deployment 启动；
- 批准或拒绝扩大 universe 的 revision；
- 批准人工 Restart；
- 批准执行插件切换后的重新启动；
- 在真实资金 canary 前确认钱包、资金和限额；
- 不参与系统的逐单自动风险裁决。

### 1.5 实盘运维员

负责运行中的 Deployment、告警和异常处置。

主要操作：

- 观察 desired/observed state、generation、heartbeat、risk account 和事件流；
- 在需要时执行 Stop；
- 对人工 Restart 发起审批；
- 处理 `RECOVERY_BLOCKED`、bundle 不可用、credential 失效和 venue 异常；
- 配合插件 drain、switch 或 force remove；
- 处理已结算但尚未 redemption 的 instrument；
- 必要时在 QZ 外核对 Polymarket 钱包和交易场所状态。

### 1.6 系统自动化

系统自动化不是人工角色，负责确定性的执行和 fail-closed 收口：

- 插件安装、validation 和 bundle 构建；
- Parquet 导入；
- Backtest、100-trial optimization、Pareto candidate 选择和单次 Holdout；
- Run 状态推进和报告生成；
- Deployment recovery/armed generation；
- heartbeat；
- 25 pUSD 原生逐单限制和 100 pUSD funder reservation；
- 进程崩溃后的自动 recovery；
- 动态 universe 的发现、排序和 pending revision 生成；
- 事件记录和 SSE 推送。

---

## 2. 用户可见的工作台节点

即使 V1 通过 API/CLI 使用，产品仍应按以下逻辑工作台组织。未来若增加 UI，这些节点可以直接映射为页面或主导航，不应把内部进程、数据库表或 Nautilus 实现细节暴露成用户工作流。

| 工作台节点 | 主要角色 | 用户在这里做什么 | 系统自动完成什么 |
|---|---|---|---|
| **System Status** | 工作台管理员、实盘运维员 | 查看 API、DB、worker、supervisor、master key、plugin manager 状态 | 汇总 readiness、heartbeat 和故障原因 |
| **Plugin Center** | 工作台管理员 | 上传 wheel、激活、停用、升级、卸载、查看影响范围 | 安装、validation、bundle 构建、drain 引用分析 |
| **Credentials & Connections** | 数据与连接管理员 | 填写 secret、配置 data source/execution connection、运行 preflight | schema 校验、secret 加密、兼容性检查 |
| **Data Catalog** | 数据与连接管理员、量化研究员 | 上传 Parquet、发起导入、选择 dataset | 分批校验、写 Nautilus Catalog、失败清理 |
| **Research Workspace** | 量化研究员 | 创建 Research、填写七章节、查看 revision | 校验章节完整性和状态转移 |
| **Strategies** | 策略作者 | 上传 `.py`、创建版本、填写默认配置 | 独立进程校验 Strategy 合同 |
| **Experiments & Runs** | 量化研究员 | 选择数据与区间、发起实验、查看进度与报告 | Backtest、optimization、Pareto 选择、Holdout |
| **Review & Approvals** | 资金与风险审批人 | 审阅不可变审批快照，批准或拒绝 | 校验审批前置条件，事务写入 desired state/event |
| **Deployments** | 实盘运维员、审批人 | 创建、Stop、请求 Restart、查看 generation | recovery、reconciliation、armed、TradingNode 生命周期 |
| **Risk & Exposure** | 资金与风险审批人、实盘运维员 | 查看限额、reservation 和阻断原因 | 每笔订单自动批准或拒绝，异常时 fail-closed |
| **Universe** | 量化研究员、审批人、实盘运维员 | 查看 active/pending roster，审批扩张 | 定时发现、排序、截断、收窄时撤单与重启 |
| **Events & Recovery** | 实盘运维员 | 查看需要人工处理的事件并执行修复 | 事件补发、自动重试、保持 `RECOVERY_BLOCKED` |

---

## 3. 正常主流程中的操作节点

### N0：系统就绪检查

**角色：工作台管理员。**

用户操作：

1. 启动 QuaZonai；
2. 查看 `/api/v1/system/health`；
3. 确认 database、master key、finite worker、live supervisor 和 plugin manager ready。

系统自动：

- 检查数据库连接和 schema；
- 检查 master key 是否可用；
- 汇总组件 heartbeat；
- 不因某个未激活或失败插件使整个控制面不可用。

人工阻断条件：

- 数据库或 master key 不可用；
- 旧 schema；
- worker/supervisor 无法工作。

### N1：安装并激活插件

**角色：工作台管理员。**

用户操作：

1. 上传 PRIMARY wheel 和必要依赖 wheels；
2. 查看 install job；
3. 审阅 descriptor、capability、版本约束和配置 schema；
4. 对 `STAGED` release 执行 activate。

系统自动：

- 离线安装；
- 加载唯一 entry point；
- 校验 descriptor 和依赖；
- 持久化 schema/capability snapshot；
- 按资源组合构建 immutable runtime bundle；
- 激活新版本时使旧默认版本进入 `DRAINING`。

必须人工确认：

- release 激活；
- force remove；
- 影响 running deployment 的插件切换。

### N2：配置凭据

**角色：数据与连接管理员；涉及真实资金时由资金所有者提供凭据。**

用户操作：

1. 选择具体 plugin release；
2. 按该 release 的 secret schema 填写字段；
3. 创建 credential set；
4. 只检查字段是否已配置，不尝试回读 secret。

系统自动：

- schema 校验；
- AES-GCM 加密；
- 将 credential 固定到 `plugin_release_id`；
- 永不返回 plaintext、ciphertext 或 nonce。

插件升级后不能自动迁移 credential；用户必须显式验证或新建。

### N3：创建数据源

**角色：数据与连接管理员。**

用户操作：

1. 选择具有 `HISTORICAL_IMPORT` 或 `LIVE_DATA` capability 的 release；
2. 选择 credential set；
3. 填写 public config；
4. 创建 data source；
5. 对真实数据连接运行 preflight。

系统自动：

- 校验 capability；
- 校验 public/secret config；
- 固定具体 release；
- 构建或复用 runtime bundle。

### N4：创建执行连接并完成外部钱包准备

**角色：数据与连接管理员 + 资金与风险审批人。**

用户在 QZ 内：

1. 选择具有 `EXECUTION` capability 的 release；
2. 选择 credential set；
3. 填写 execution config；
4. 创建 execution connection；
5. 运行 production read-only preflight。

用户在 QZ 外：

- 创建和管理 Polymarket wallet/funder；
- 准备 CLOB credentials；
- 完成 funding 和 allowance；
- 保管 signer private key；
- 后续执行 redemption。

系统自动：

- 只允许官方 Nautilus execution adapter；
- 校验 data/execution `compatibility_key`；
- 验证 QZ/Python/Nautilus 版本约束；
- 不替用户划转资金或配置 allowance。

### N5：导入历史数据

**角色：数据与连接管理员发起，量化研究员验收可用性。**

用户操作：

1. 选择 data source；
2. 上传固定 schema 的 L2 Parquet；
3. 填写 instrument identity、时间范围和 source label；
4. 发起 import；
5. 查看成功 dataset 或失败原因。

系统自动：

- 固定 `parquet_l2` release 和 runtime bundle；
- 分批读取；
- 校验 decimal、snapshot 和 event 连续性；
- 成功后原子注册 Catalog dataset；
- 失败时清理 staging，不注册部分 dataset。

### N6：创建 Research

**角色：量化研究员。**

用户操作：

1. 创建 Research；
2. 完成以下七个章节：
   - `HYPOTHESIS`
   - `MARKET_CONTEXT`
   - `DATA`
   - `METHOD`
   - `RESULTS`
   - `RISKS`
   - `CONCLUSION`
3. 每次修改产生新 revision；
4. 准备激活 Research。

系统自动：

- 检查章节是否完整；
- 检查是否存在有效 Strategy version；
- 只允许定义的状态转移。

### N7：上传 Strategy version

**角色：策略作者。**

用户操作：

1. 上传恰好一个 `.py` 文件；
2. 提交默认 Strategy config；
3. 查看 contract validation 结果；
4. 验证失败时创建修正后的新 version。

系统自动：

- 校验文件和配置大小；
- 在独立进程导入模块；
- 检查 `Config`、`Strategy`、`suggest`、`OBJECTIVE_DIRECTIONS` 和 `objectives`；
- 不安装源码声明的动态依赖；
- 不把 Strategy 当作安全沙箱。

### N8：配置并启动 Experiment

**角色：量化研究员。**

用户操作：

1. 选择 Strategy version；
2. 选择 Catalog dataset；
3. 设置训练时间范围；
4. 设置独立且不重叠的 Holdout 时间范围；
5. 确认 objective directions 和 seed；
6. 发起 Experiment。

系统自动：

- 固定 runtime bundle；
- 校验时间范围和资源引用；
- 创建 optimization Run；
- 不要求用户逐个创建 trial。

### N9：Optimization、Pareto 选择和 Holdout

**角色：系统自动化；量化研究员只观察。**

系统自动：

1. 执行恰好 100 个 Optuna trials；
2. 最多并发 4 个独立 BacktestNode 进程；
3. 确定性选择 Pareto 折中解；
4. 对唯一 candidate 执行一次 Holdout；
5. 生成摘要和 Nautilus report 引用；
6. 成功后把 Research 推进到 `REVIEW`。

用户不应：

- 手工挑选另一个 Pareto candidate 替换自动结果；
- 在 Holdout 失败后自动尝试下一个 candidate；
- 修改已经运行中的 Run 所固定的 Strategy、dataset 或 bundle。

### N10：审阅研究结果

**角色：量化研究员 + 资金与风险审批人。**

量化研究员操作：

- 阅读训练、optimization 和 Holdout 结果；
- 检查报告、时间范围和候选参数；
- 补充 `RESULTS`、`RISKS` 和 `CONCLUSION`；
- 决定提交审批还是回到研究阶段。

资金与风险审批人操作：

- 独立检查 Holdout；
- 检查 Strategy version、dataset、插件 releases、runtime bundle；
- 检查执行连接、universe predicate、25/100 pUSD 限额；
- 检查真实资金前置条件。

### N11：审批 Deployment

**角色：资金与风险审批人。**

用户操作：

- `APPROVE`：允许创建 desired state 和启动事件；
- `REJECT`：填写拒绝理由，Research 回到 `ACTIVE`。

审批界面或 API 响应必须展示不可变快照：

- Research revision；
- Strategy version 和参数；
- dataset 和训练/Holdout 区间；
- Holdout 指标；
- data/execution plugin release；
- runtime bundle；
- execution connection 和 funder；
- universe predicate/cap；
- 25 pUSD/100 pUSD 风险边界；
- 当前已知限制。

禁止“一键研究并直接实盘”绕过审批。

### N12：Deployment 启动

**角色：系统自动化；实盘运维员观察。**

系统先运行 recovery generation：

1. 取得 owner lock；
2. 校验固定 bundle；
3. 启动无 Strategy、无 heartbeat 节点；
4. 获取订单和仓位状态；
5. 撤销无法归属的残留挂单；
6. 完成风险 reconciliation。

随后运行 armed generation：

1. 生成精确 instrument 风险 map；
2. 再次 reconciliation；
3. 激活中央 reservation；
4. 启用 heartbeat；
5. 检查 Strategy readiness；
6. 最后加载 Strategy 并进入 `TRADING`。

任何一步失败都不允许增加风险。

### N13：实盘监控

**角色：实盘运维员；资金与风险审批人只在风险决策时介入。**

用户应观察：

- desired/observed state；
- runner generation；
- heartbeat；
- active universe 和 recovery roster；
- risk account、reservation 和阻断事件；
- 插件 release/bundle 状态；
- venue/reconciliation 异常；
- `resolved_unredeemed` instrument。

系统自动：

- 每笔订单执行 Nautilus 25 pUSD 限制；
- 执行 funder 100 pUSD gross reservation；
- 数据库、heartbeat、projection 或 reconciliation 不完整时 fail-closed；
- cancel 始终保持可用；
- 进程崩溃后自动进入 recovery，而不是直接恢复交易。

### N14：Universe 变化

**角色：系统自动化发现；审批人和实盘运维员处理需要人工决策的 revision。**

系统自动：

- 每 60 秒发现市场；
- 按 liquidity 和 `InstrumentId` 稳定排序；
- 生成 pending roster；
- 已获批 predicate 内的新市场可以进入待激活 revision；
- 收窄条件时撤单并受控 restart。

必须人工审批：

- 放宽 predicate；
- 降低流动性或到期门槛；
- 提高 instrument cap；
- 其他扩大可交易范围的变化。

### N15：人工 Stop

**角色：实盘运维员。**

用户操作：

- 发起 Stop；
- 观察从 `STOPPING` 到 `STOPPED`；
- 在 QZ 外核对仍持有的仓位或待结算资产。

系统自动：

- 禁止新增订单；
- 撤销挂单并等待确认；
- 停止 Strategy、TradingNode 和 heartbeat；
- **不强平现有仓位**。

### N16：人工 Restart

**角色：实盘运维员发起，资金与风险审批人批准。**

用户操作：

1. 修复停止原因；
2. 确认插件/bundle、credential、connection 和风险状态；
3. 发起 Restart；
4. 审批人审阅新启动快照并批准。

系统自动：

- 创建新的 `DEPLOYMENT_START` approval；
- 批准后创建新 generation；
- 从 recovery 开始，不直接恢复 Strategy。

### N17：插件升级或切换

**角色：工作台管理员 + 数据与连接管理员 + 实盘运维员 + 资金与风险审批人。**

正常升级：

1. 管理员上传并验证新 release；
2. 管理员激活新 release，旧 release 进入 `DRAINING`；
3. 连接管理员创建或迁移 data source、execution connection 和 credential；
4. 系统构建新 bundle并完成真实配置 preflight；
5. 实盘运维员发起 plugin switch/restart；
6. 审批人批准；
7. 新 runner 使用新 bundle 从 recovery 开始；
8. 旧 release 无引用后进入 `INACTIVE`。

禁止：

- 修改运行中 Deployment 的 `plugin_release_id`；
- 在旧 TradingNode 内 reload；
- 自动把旧资源漂移到新版本。

### N18：市场结算和 Redemption

**角色：实盘运维员 / 钱包操作者。**

系统自动：

- 识别市场结算状态；
- 将 instrument 标记为 `resolved_unredeemed`；
- 阻止不再允许的新交易。

用户在 QZ 外：

- 使用官方 Polymarket 流程 redemption；
- 核对钱包到账；
- 必要时记录外部处理结果。

QZ 不代替钱包 redemption，也不建立资金划转账本。

---

## 4. 必须人工确认的硬节点

以下操作不得被普通后台自动化静默完成：

| 硬节点 | 必须由谁确认 | 原因 |
|---|---|---|
| 激活 plugin release | 工作台管理员 | 改变新资源可用的运行代码 |
| 写入真实 credential | 数据与连接管理员/资金所有者 | 涉及敏感连接能力 |
| 提交新的 Strategy version | 策略作者 | 改变研究和交易逻辑 |
| 发起 Experiment | 量化研究员 | 固定数据、时间和策略边界 |
| 首次实盘启动 | 资金与风险审批人 | 开始真实资金暴露 |
| Universe expansion | 资金与风险审批人 | 扩大可交易范围 |
| 人工 Restart | 资金与风险审批人 | 重新允许真实交易 |
| Execution plugin switch | 资金与风险审批人 | 改变真实执行代码和连接 |
| 人工 Stop | 实盘运维员 | 改变运行状态且不会强平 |
| Force plugin remove | 工作台管理员 + 实盘运维员 | 可能使 Deployment 无法恢复 |
| 真实资金 canary | 资金所有者/审批人 | 使用生产环境和真实资金 |
| Redemption | 钱包操作者 | QZ 外的资产处置 |

---

## 5. 不应要求用户手工操作的节点

以下行为属于系统内部机制，不应让普通用户逐步点击或填写：

- 选择 job lease owner；
- 创建 validation venv；
- 运行 `uv pip check`；
- 决定 bundle 文件路径；
- 启动每个 Optuna trial；
- 手工挑选自动 Pareto candidate；
- 手工创建 BacktestNode/TradingNode；
- 维护 advisory lock；
- 计算逐单 reservation；
- 在 recovery 和 armed generation 之间手工推进状态；
- 手工发送 heartbeat；
- 手工清理失败 staging；
- 手工把同一个插件 reload 到运行进程。

用户只应看到：当前状态、系统正在做什么、阻断原因、允许的下一步、影响范围和需要人工确认的风险。

---

## 6. 异常处理节点

| 异常状态 | 首要处理角色 | 用户操作 | 系统行为 |
|---|---|---|---|
| `PLUGIN_INSTALL_FAILED` / `PLUGIN_VALIDATION_FAILED` | 工作台管理员 | 查看失败阶段和 stderr；修复 wheel/依赖后重试或上传新版本 | 隔离失败 release，清理临时环境 |
| `PLUGIN_BUNDLE_BUILD_FAILED` | 工作台管理员 | 检查 release 组合和 core 约束；重新构建 | 不影响其他 ready bundle |
| `STALE` bundle | 工作台管理员 | 从原 releases 重建 bundle | 禁止新 Run/generation 使用旧 bundle |
| `CREDENTIAL_INVALID` | 数据与连接管理员 | 更新或重建 credential set | 禁止依赖连接启动 |
| Import `FAILED` | 数据与连接管理员 | 根据 schema/semantic 错误修正文件并新建 Run | 不注册部分 dataset，清理 staging |
| Optimization `FAILED` | 量化研究员 | 查看基础设施或策略错误，修正后新建 Experiment/Run | 不伪造 100-trial 成功 |
| Holdout 失败 | 量化研究员 | 修改研究或 Strategy 后重新开始研究流程 | 不自动换 candidate |
| Approval `REJECTED` | 量化研究员 | 根据理由修改章节或 Strategy version | Research 回到 `ACTIVE` |
| `RISK_LIMIT_EXCEEDED` | 策略作者/风险审批人 | 判断策略订单是否合理；修改 Strategy 或等待暴露下降 | 自动拒单，不需要人工改单放行 |
| `RISK_UNAVAILABLE` | 实盘运维员 | 修复 DB、heartbeat、owner 或 reconciliation | 增加风险 fail-closed，cancel 可用 |
| `RECOVERY_BLOCKED` | 实盘运维员 | 查看精确阻断原因，修复 bundle/credential/venue/投影 | 持续退避重试，不加载 Strategy |
| `PLUGIN_IN_USE` | 工作台管理员 | 先迁移/停止引用，或明确执行 force remove | 普通删除不破坏活动资源 |
| `BLOCKED_PLUGIN_REMOVED` | 工作台管理员 + 实盘运维员 | 安装新兼容 release 或重新绑定连接，并重新审批 | 不自动漂移到其他版本 |
| `resolved_unredeemed` | 钱包操作者 | 在 QZ 外 redemption | 保留控制状态，不代替钱包操作 |

---

## 7. 操作提醒和通知原则

系统不应把所有事件都推给用户。只有以下情况应进入“需要操作”队列：

- 插件安装、validation 或 bundle 构建失败；
- credential/preflight 失效；
- import、optimization 或 Holdout 失败；
- 存在待审批的 Deployment start 或 universe expansion；
- Deployment 进入 `RECOVERY_BLOCKED` 或 `FAILED`；
- risk/heartbeat/reconciliation 不可用；
- 插件进入 `DRAINING` 且仍有长期引用；
- bundle 变成 `STALE` 且将影响新的 Run/recovery；
- market `resolved_unredeemed`；
- Stop 长时间不能完成撤单确认。

纯进度事件应保留在事件流中，但不要求用户处理。

---

## 8. 产品交互约束

无论最终使用 CLI、API 客户端还是未来 UI，每个可操作资源都应返回或展示：

- 当前状态；
- 允许的下一步动作；
- 禁止动作及明确原因；
- 当前固定的 `plugin_release_id` 和 `runtime_bundle_id`；
- 是否涉及真实资金；
- 是否需要新审批；
- 对 running Run/Deployment 的影响范围；
- 最近失败阶段和可执行修复；
- 相关事件和报告入口。

危险动作必须拆分：

- `deactivate` 与 `force remove` 分开；
- `stop` 与“强平”分开，V1 不提供强平操作；
- `restart request` 与 `approval` 分开；
- `plugin activate` 与 `running deployment switch` 分开；
- Research 通过与真实资金启动分开。

---

## 9. 最小人工参与模型

在单操作者 V1 中，最小可运行流程可以压缩为五次责任切换：

```text
工作台管理员
  安装并激活插件
        ↓
数据与连接管理员
  配置数据、credential、execution 和钱包前置条件
        ↓
量化研究员 / 策略作者
  完成 Research、Strategy、Experiment 和结果复核
        ↓
资金与风险审批人
  审批真实资金启动及后续扩张/重启
        ↓
实盘运维员
  监控、Stop、异常恢复、插件迁移和结算处置
```

同一个人可以承担全部角色，但系统应通过独立的操作步骤、审批快照和显式确认，避免把研究动作、代码变更和真实资金启动合并成一次不可审计的点击。
