# 产品、领域与架构

<a id="product"></a>
## 产品边界

QuaZonai 面向单人本机研究。项目冻结研究问题、数据、验证政策与预算；研究员提出实验，独立评估形成 Alpha 资格，组合研究生成候选与目标包。用户通过 Web/PWA、CLI 或 Mission MCP 查看和推进同一套持久记录。

QZ 只交付目标，不连接券商账户或执行真实订单。任务执行状态、证据质量与研究决策分别记录；`SUCCEEDED` 不等于 `VALID` 或 `PASS`。失败、取消、淘汰和无效试验保留在原账本。

<a id="modules"></a>
## 模块与依赖

| 模块 | 责任 | 第一方生产/构建依赖 |
| --- | --- | --- |
| [contracts](../crates/contracts/src) | Wire 类型、严格序列化、公开 schema | 无 |
| [domain](../crates/domain/src) | 预算、准入、资格与状态转换的纯规则 | contracts |
| [integrations](../crates/integrations/src) | 原生 Codex、MCP、外部协议适配 | contracts |
| [store](../crates/store/src) | SQLx 事务、并发控制、幂等回执、PGMQ | contracts、domain |
| [server](../apps/server/src) | HTTP/CLI/MCP、API 与 Worker 编排 | contracts、domain、store、integrations |
| [runtime](../apps/runtime/src) | 科学任务网关、OCI 生命周期、持久 journal | contracts、domain、integrations |
| [job](../apps/job/src) | 类型化科学计算与原生结果文件 | contracts、domain |
| [web](../apps/web/src) | React、Ant Design、生成客户端、PWA | 公开 API 合同 |

依赖方向由 [architecture 测试](../crates/contracts/tests/architecture.rs)检查。测试辅助跨层复用不改变生产依赖；领域层不执行 SQL、HTTP 或容器操作。

复用边界：Codex 管理 Thread/Turn/Item、模型目录、登录和工具循环；NautilusTrader 管理原生目录、撮合、共享资金模拟与权益；Clarabel 负责求解；Wasmi 执行有界模型；Arrow 保存列式结果；PostgreSQL/PGMQ 负责事务与消息。QZ 只补充研究规则和跨组件关联。部署 Python 与原生历史数据适配保留在现有边界内，不扩展成第二套领域服务。

<a id="contracts"></a>
## 合同与身份

字段、枚举和输入限制以 [Rust DTO](../crates/contracts/src)、[API OpenAPI](../contracts/generated/api-v2.openapi.json)、[Runtime OpenAPI](../contracts/generated/runtime-v1.openapi.json)及[领域 schema](../contracts/generated/domain-v1.openapi.json)为准。修改在源定义完成并生成派生文件；客户端不能自行补全未知字段或把不兼容响应解释为有效证据。

实体 ID、版本 ID、版本序号和修订号不可互换。Decimal、超出 JavaScript 安全整数范围的计数与纳秒值按原合同传递，不能先转 `Number` 再序列化。正式指标拒绝 NaN/Infinity；缺失值不变成零。

写命令绑定原请求、幂等键、作用域和预期修订。相同请求重放原回执；改正文、换键或自动修改修订号不是重试。超时可能发生在提交之后，先用原身份对账。冻结对象追加新版本，撤销追加记录，不覆盖已发生的执行或交付。

<a id="cycle-startup"></a>
## 周期、预算与执行

`POST /api/v2/projects/{id}/cycles` 的调用链：

| 阶段 | 实现与边界 |
| --- | --- |
| HTTP | [cycles.rs](../apps/server/src/cycles.rs) 解析 `CycleStartV1`、权限与幂等键，调用 Store；不执行 SQL |
| 事务准入 | [Store::start_cycle](../crates/store/src/cycles.rs) 重放旧回执，或锁定项目、检查修订、冻结 Brief、数据与 Runtime |
| 纯规则 | [domain/admission.rs](../crates/domain/src/admission.rs) 计算预算准入；Store 读取并锁定事实、应用结果 |
| 原子发布 | [lifecycle.rs](../crates/store/src/lifecycle.rs) 同事务预留预算、写 QUEUED Run/事件、PGMQ 消息及关联 |
| 领取 | [queue.rs](../crates/store/src/lifecycle/queue.rs) 领取已提交任务；HTTP 202 仅表示排队，不表示完成 |

回归入口：[HTTP 周期](../apps/server/tests/cycles_http.rs)、[Store 周期](../crates/store/tests/cycles.rs)、[原子准入](../crates/store/tests/atomic_cycle_admission.rs)。

PGMQ 是至少一次投递，不提供外部副作用的 exactly-once。Run、Attempt、预算预约、远端身份、事件与结果采纳共同约束重复执行。重启、换 UUID 或重建项目不能清零原试验与数据暴露记录。领取后的输入和期限不能被新配置替换。

取消先记录意图，再核对实际远端停止及晚到创建/启动结果；断线、404、本地进程退出或停止观察均不证明远端已终止。原远端结果未知时保持对账状态，不另建一份计算。SSE 使用原游标恢复；游标失效须披露事件缺口并读取当前快照。

<a id="data"></a>
## 数据与研究隔离

Brief 冻结假设、经济含义、Universe、数据授权、预测期限、基准、费用/容量、验证分区、选择/停止规则及预算。政策变更创建新版本，不能事后改阈值使既有结果通过。

原始目录/snapshot、Instrument 定义、Universe 历史成员、交易日历、质量报告和许可必须可追溯。`event_at`、`available_at`、`decision_at` 分别表示事件、当时可得与决策时间；PIT 要求 `available_at <= decision_at`。抓取时间、当前成分或重述后的数据不能冒充历史可得信息。

Discovery、Validation、Sealed、Forward 按冻结用途隔离。原始值、样本、指标、图表和摘要都可能暴露数据；暴露沿根血缘继承，在评估器获得读取能力前预约。崩溃或取消不退还已发生的读取机会。研究员不能通过日志/预览读取 Sealed，独立评估器不能改写研究工作区。披露后的数据不再作为未接触的独立样本。

时间验证使用支持对应标签区间的原生 purge/embargo。变量标签区间没有受支持实现时报告不支持，不用固定行数近似。选择统计需要完整可比试验集合；CPCV 不等于 CSCV/PBO。未接通并验证的 DSR/PBO 等必需指标返回不确定结论，不生成常量或伪指标。

撤销许可或资格后，历史记录仍可审计，但不能据此批准新研究或新交付。测试来源不能改标 REAL；缺数据、协议能力或合法来源时保留具体失败。

<a id="codex"></a>
## Codex 与 Mission

研究员和独立审阅员共用原生账户，但使用独立会话、工作区和角色设置。模型与推理强度来自实际 Codex 目录与能力；完整消费模型分页，不维护固定模型清单。“本机默认”沿用原生配置，不配置第三方 provider URL/API key。

容器部署的 API 探测与 Worker Mission 均调用独立 Codex 镜像中的 App Server；连接失败不回退宿主 Codex。ChatGPT 设备码授权、令牌保存/刷新由原生 Codex 承担。浏览器可启动、观察、取消登录和确认退出；刷新页面不重新泄露设备码。QZ 不读取或复制 `auth.json`。

Mission 固定原输入、角色模型配置、预算、工具绑定和原生执行身份。工具调用结果回到原 Thread；不能通过另起 Thread 或静态报告代替科学反馈。MCP 使用官方 SDK，工具权限绑定项目/Mission/Attempt，不开放通用 Shell、任意 SQL 或 Docker 控制。

每个 App Server 会话独立容器。CPU、内存、进程数及墙钟受原生限制；Worker 退出不重置任务期限。取消核对实际 container ID。只记录公开调用和可观察结果，不索取或保存隐藏推理。

<a id="runtime"></a>
## 科学 Runtime 与产物

Runtime 接收已登记的镜像、不可变输入引用和固定任务，不拥有预算、数据许可或交付资格。配置与类型见 [runtime/config.rs](../apps/runtime/src/config.rs) 和 [Runtime 合同](../crates/contracts/src/runtime_jobs.rs)。`images` 按 `job_kind/image_ref` 绑定不可变镜像身份；`catalogs` 按 `root/metadata_file` 登记原目录；资源和存储上限由宿主配置决定，不从任务正文接收任意挂载或命令。

Gateway journal 保存提交身份、launch、取消与结果；重启恢复原任务，不把新镜像/路径套进旧身份。任务只挂载自己冻结的输入，编译不获得 Dataset。输出经完整类型、请求绑定、大小和数值检查后才可采纳，文件存在或进程 exit 0 不等于正式成功。

Artifact 绑定内容、类型、来源和原生产者；正式文件不可覆盖。MODEL、PARAMETERS、REPORT 和数据授权有各自语义，不能用一份参数副本伪造真实费用/容量来源。授权已撤销或报告不完整时拒绝新采纳。

原生计算与 OCI 回归位于 [job/tests](../apps/job/tests)、[runtime/tests](../apps/runtime/tests)及[Native Runtime 工作流](../.github/workflows/native-runtime.yml)。源码变更后必须使用重新构建的镜像验证，`--version` 或 doctor 不代替执行、取消与恢复。

<a id="portfolio"></a>
## Alpha、组合与历史

Alpha 输出 score、expected_return、uncertainty，不输出订单。Score 未经允许训练段校准不能冒充收益或仓位。实验、校准、评估与资格绑定原版本；至少两个合格 Alpha 的真实预测经资产、单位、共同期限、币种和覆盖率对齐后进入组合。

组合 Build/Study 冻结原 MODEL、校准、费用、日历、当前权重及原始目录选择。当前权重必须有可核验来源，不能由资产清单猜测。求解复用原生估计器和 Clarabel；方差与 CVaR 的单位、周期、模型和风险预算按冻结合同解释。无解、零风险或无效对偶见证不返回备用权重。

Nautilus 在一个共享资金账户内执行目标序列，保留原生成交、费用和权益。不能加总单 Alpha 回测曲线作为组合净值，也不能重复扣费。历史 BAR 参与率只是冻结条件下的观测，不冒充盘口深度或未来成交保证。

[portfolio_history.rs](../crates/contracts/src/portfolio_history.rs) 是 Arrow 历史读写合同；Study 输出绑定原 manifest、请求、报告和完整行列元数据。历史价值曲线读取同一已验证组合序列；缺口不插入零值，不重算另一份权威净值。模拟成功本身不创建 PORTFOLIO 资格或交付授权。

<a id="polymarket"></a>
## Polymarket

历史准备复用 Nautilus 原生 Polymarket 适配器与 Parquet catalog。网络、分页或结算失败不得生成替代目录；原始来源、资产映射、历史可得时间和结算证据随 snapshot 冻结。不得把今日 Gamma 状态或交易末价当历史结算。

二元资产的币种、费用、交易截止、结算来源及目标期限保持一致。已结算持仓按冻结结算证据兑现，未来期限不越过允许交易区间。研究抵押币与 Codex 成本预算的法币单位分开；不把旧 USD 配置或旧探测当作新能力。准备与回归入口位于 [job 源码](../apps/job/src)和[Polymarket history 工作流](../.github/workflows/polymarket-history.yml)。

<a id="handoff"></a>
## 目标交付

研究 Release 冻结原 Candidate、评估、目标包与来源。批准、Offer、Claim、ACK 和撤销是不同记录：批准不等于已领取，ACK 不等于 QZ 执行了交易。创建、领取时重验当前来源、资格、下游配置、期限及拒绝/撤销历史。

同一外部 Claim/ACK 身份重放原结果，不刷新期限；改编号或正文不能覆盖终态。撤销与 supersede 只影响尚未领取的 Offer，已领取事实不变，也不代表下游撤单。自动化政策同样冻结版本与预算；异常不能绕过交付条件。

<a id="web"></a>
## Web 与 PWA

界面以项目、实验、Alpha、组合、交付、运行为主对象；版本和证据放详情。使用官方 Ant Design 表单、反馈与主题，图表复用 ECharts。移动端保留同等操作能力，长表格和详情不能因视口收窄丢失操作。

浏览器访问本机入口，无 TOTP 绑定流程。机器/Mission 身份不复用浏览器身份。表单沿用服务端的字段、联合约束和错误；HTTP 202、过期缓存和未知结果不能展示为成功。PWA 检测新前端版本后提示用户更新，不在编辑中静默替换页面。

<a id="container-release"></a>
## 镜像与持久状态

Web/API/Caddy 同镜像，PostgreSQL/PGMQ 独立 Compose 服务，Worker 从同镜像提取 server 并由宿主 systemd user manager 运行。应用镜像同时提供匹配的 Runtime 网关二进制；科学 job 与 Codex 由 CI 制作独立 GHCR 镜像。安装器按 manifest 拉取已有镜像。原生资源、数据目录、journal 与登录目录分别保持原身份。

| 文件 | 关键字段与不变量 |
| --- | --- |
| `release.json` | `schema_version=2`、完整 `version` tag、40 位 `revision`、固定 GHCR digest 的 `image/database_image/runtime_image/codex_image`、精确 `codex_version`；不包含私有配置 |
| `installation.json` | 保存原 `root/uid/gid/home/codex_home/path/unit_directory`、端口、密码、Compose `project`、`bundle`、本安装 `codex_runtime_image` 启动别名及可选 `runtime_targets/downstream_targets`；首次建库前持久化身份，重试不重生 |
| `.env` | 独立 Codex 的精确 `CODEX_VERSION`；应用升级保留，不作为 Shell 执行 |
| `pending.json` | 原 `previous/target/backup/phase`；恢复同一操作，不清空或重建安装 |
| `current` | 仅在目标 HTTP 与 Worker 启动检查成功后选择新版本 |

升级在下载/ABI 检查后确认静止点；关闭 API 后、停止 Worker 前再查 Run，防止竞态准入。先持久化 `preparing`，静止备份成功并落盘后写 `migrating`，才执行迁移。迁移前失败可恢复旧处理器；迁移后不启动旧程序访问新 schema。

Worker 配置原子写入并同步后，先保存 `starting` 再启动候选。`starting` 重试恢复同一容器和 Worker，不重复迁移、不停止可能已拥有 Run 的候选。激活依次同步 `current`、启用 Worker 恢复、开启应用自动重启、清除 pending。缺少 phase 的旧记录按可能已迁移处理。

Codex 更新先拉取已发布镜像并执行真实版本与 sandbox 检查，再持部署锁确认 Run/会话静止，切换镜像与 `.env`。启动器与更新器使用同一锁；运行会话不长期持锁。认证目录保持原位置，失败或中断按原目标对账，不回退宿主可执行文件。

实现：[deploy/docker](../deploy/docker)。发布和诊断：[operations](operations.md)。安装、更新与备份：[部署手册](../deploy/docker/README.md)。
