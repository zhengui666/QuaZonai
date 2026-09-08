# CLI 命令

完整产品合同在 DESIGN。当前已实现原生验证、逐轮 Store、浏览器认证、Project/机器身份和不可变研究准备 HTTP 控制面；研究/组合/交付命令仍待实现，不提供绕过 API 的手工 SQL 业务路径。

## 认证服务与本机管理

以下入口复用 Clap；`cargo run --locked -p server -- --help` 展示实际命令。

```sh
# 目录必须不存在；生成私有 master.key、原生 session key 和加密 secrets 目录。
cargo run --locked -p server -- init-state --state-dir ./var

# DATABASE_URL 此时是独立的新库迁移身份。原生 PostgreSQL 管理预先创建
# quazonai_app 登录角色；本命令只授予应用所需 DML，不创建或输出数据库密码。
cargo run --locked -p server -- migrate --application-role quazonai_app

# 将 DATABASE_URL 切换为非 owner、非 superuser 的应用身份。
# 此本机命令显示一次15分钟有效的初始化 capability；没有远程发证接口。
cargo run --locked -p server -- bootstrap

# PUBLIC_URL 必须是实际同源 HTTPS 入口。API 不在启动时执行 DDL。
cargo run --locked -p server -- serve --state-dir ./var \
  --bind 127.0.0.1:8080 --public-url https://research.example
```

`DATABASE_URL` 支持环境变量；不要把真实密码写到命令行、Git 或日志。默认启动拒绝具有 schema CREATE、表 TRUNCATE 或超级用户权限的应用角色。master key 必须独立于数据库和加密对象备份。

本地开发可显式使用 `--development-http --public-url http://127.0.0.1:8080`，同时监听地址必须为 loopback。此选项只调整本地传输和 cookie 的 Secure 属性，不跳过初始化、TOTP、会话撤销、Origin 或数据库角色校验。

本机维护：`cargo run --locked -p server -- prune-unpublished-verifiers --state-dir ./var` 在数据库发布锁下，只回收无任何历史凭据引用、原生用途认证为 MACHINE_VERIFIER 的孤儿。数据库错误时不删除；不提供远程/Agent删除密钥接口。详见 OPERATIONS。

## 已实现的控制面 HTTP 合同

`server openapi` 包含实际 Project 与机器身份路由，不是手写路径清单或待实现占位。项目命令的 HTTP/CLI/MCP 统一以服务端事务为准，不提供 SQL 业务后门。控制面专用远程 CLI 与 MCP 仍在同一 PR 中接通，不能把本机 `server` 管理命令视作已实现全部研究命令。

真实浏览器：原生 TOTP 登录后使用同源私有 cookie，写操作携带 Origin、Idempotency-Key 和 DTO 的 expected_revision。机器：只使用独立 Bearer token，不复制浏览器 cookie；`GET /api/v2/auth/machine` 显示自身公开归属/权限/到期，`GET /api/v2/projects` 只返回授权项目。项目和凭据管理要求 Operator 浏览器的最近认证，或专属 CLI 身份提交原生 TOTP 后获得一次性精确命令 grant；Agent、自动化和下游不能取得该人工授权。

## 已实现的研究准备 HTTP 合同

`GET/POST /api/v2/input-sets`、`GET /api/v2/input-sets/{id}` 与
`GET/POST /api/v2/evaluation-policies`、`GET /api/v2/evaluation-policies/{id}`
均已接通 Rust 业务事务。集合 GET 必须给 `project_id`，`limit` 为1–100，
后续页使用响应中的 UUID `next_cursor`。完整字段由 `server openapi` 生成；
这些新增路径尚没有专属远程 CLI 子命令，不把本机管理入口当作研究客户端。

输入创建提交目的、微秒精度的 `decision_cutoff` 和1–256个已登记原生对象的
类型化引用；id、连续 ordinal、冻结时间由服务端生成。结果只含元数据，
不会返回 Sealed 原始字节、宿主路径或原生存储位置。数据源停用、许可过期或
撤销、跨项目产物和分区不匹配会拒绝新登记；不要手工写 SQL 创建引用来绕过。
目前数据源/数据版本和执行假设的可信登记入口仍须在后续工作包接通。

评估政策创建需要同项目已冻结 comparison 输入、执行假设和完整 selection、
split、required 指标等意图。policy 版本和 experiment_family/root_lineage
由服务端同事务分配，客户端不能挑选新谱系来清除暴露。WALK_FORWARD 使用
VALIDATION comparison 且不得包含 sealed_revision；SEALED selection 使用
包含精确 sealed_revision 的 SEALED comparison。策略、输入和成员创建后不能
原地追加或改写；相同幂等请求只返回首次冻结的元数据。

写操作仍要求近期 Operator 浏览器认证，或 CLI 的一次性 TOTP grant：
`INPUT_SET_CREATE` / `EVALUATION_POLICY_CREATE` 的 target 为 null，授权绑定
完整非秘密请求。RESEARCH_READ 的机器只能读精确授权项目，不因此得到发布
或验证权限。输入/政策 POST 与完整人工授权请求上限64KiB，超过直接拒绝；
其他原有路径仍保留其上限。422 的 `field_errors` 指明安全字段路径和原因，
不包含输入数据、密钥、存储路径或 SQL。

保存 FIXTURE/UNVERIFIED 输入及未核验方法的政策，仅表示如实保存研究准备；
Brief 冻结、任务准入与独立评估必须另行核验实际原生能力、当前许可和证据资格。
这个 API 不执行模型、切分、估计或回测，不能用登记成功替代生产可交付结论。

## 已实现的 Brief 草稿 HTTP 作者流程

`GET/POST /api/v2/projects/{id}/briefs` 与 `GET/PATCH /api/v2/briefs/{id}`
使用严格 BriefCreate/BriefUpdate/BriefView。完整请求由原生 `server openapi` 导出。
创建只传研究内容、已登记的数据绑定和可选 supersedes_id，服务端分配 DRAFT/版本/身份；
更新必须携带 expected_revision，并完整替换内容及绑定。相同幂等键返回原响应，冲突409，
失败不提交半套成员。FROZEN 不可编辑，只能新建版本；归档项目不能新增或编辑。

`BRIEF_CREATE` 人工 CLI grant 的 request 为
`{schema_version:1,project_id,request:BriefCreate}`，target_id=null；项目绑定不可替换。
`BRIEF_UPDATE` 的 request 为完整 BriefUpdate、target_id为精确Brief。
这些命令仍只允许近期 Operator 浏览器或经真实 TOTP 的单次人类 CLI 授权；
RESEARCH_READ 仅可读自身项目的内容/元数据，不取得 Sealed 原始数据。
草稿保存验证范围、预算、引用、角色及币种，但不是冻结、原生能力或正式研究资格。
本批不提供假成功 freeze 或绕过API的手工SQL。部署迁移为草稿成员单表授予受触发器
约束的 DELETE，不扩大其他app表的历史删除权限。

## 研究产物：真实字节上传与受限下载

`POST /api/v2/artifacts` 使用 `ArtifactCreate={schema_version:1,project_id,kind,content}` 和
Idempotency-Key，kind 只接受 CODE、PARAMETERS、REPORT。content 是原始 UTF-8 文本，
最多2 MiB；PARAMETERS/REPORT 必须为包含整数 schema_version=1 的 JSON object。
CODE 不在 API 进程编译或执行。所有此类用户/Agent提交均标记 SYNTHETIC/RESEARCH，
不能提交 origin、路径、producer、Run/Attempt、状态或 REAL/PACKAGE/METRICS 权限。

浏览器须近期 Operator；CLI/AUTOMATION/MISSION 使用精确项目的 ARTIFACT_SUBMIT。
这里不需要也不授予 Operator grant。Mission 凭据必须由受信任任务服务签发并绑定当前
Attempt；同一 Run 的所有历史上传累计占用冻结 output_bytes，不因换 Attempt 清零。
相同幂等键和完全相同原始字节返回原产物；哪怕字节长度相同但内容不同，也返回409。

`GET /api/v2/artifacts?project_id=UUID&limit=50&cursor=UUID` 返回公开元数据和下一游标；
`GET /api/v2/artifacts/{id}` 返回详情，`GET /api/v2/artifacts/{id}/content` 下载原生内容。
机器读取需 RESEARCH_READ，只有同项目 RESEARCH 可见；EVALUATOR_ONLY 不由这些接口
披露。下载为 attachment/application/octet-stream、no-store、nosniff，不直接运行 HTML。
429须区分错误码：AUTH_RATE_LIMITED按原生Retry-After等待；BUDGET_EXHAUSTED的
retryable=false且没有Retry-After，field_errors只返回安全资源标记（上传为artifact_output_bytes），
不能自动重试或换Attempt绕过。503存储不可用或未知提交应保留同key核对，不能凭本地文件存在
认定数据库已发表。生成Web客户端的该下载接口使用parseAs: 'blob'、'arrayBuffer'或'stream'，
按实际OpenAPI媒体合同保留原始字节；普通JSON接口不会因此跳过验证。上传内容不要写进Issue、错误日志或命令行参数；尚无专属远程CLI
子命令，不以手工SQL代替HTTP。接口本身不生成评估或资格，也不是原生模型工具闭环。

## 已接通的原生 stdio MCP

`server mcp` 复用官方 Rust MCP SDK，只服务一个由可信任务启动器绑定的 Mission。
运行入口实际为 `cargo run --locked -p server -- mcp --help`，不是另一个尚未存在的
CLI/MCP package。启动器必须已通过正常控制面取得有效 Mission 凭据；本命令不签发
身份、不创建 Run、不读取数据库、STATE_DIR、Provider 配置或浏览器 Cookie。

必填参数为 `--api-origin`、`--project-id`、`--cycle-id`、`--run-id`、`--attempt-id`、
`--brief-id`；五个 ID 使用既有 UUIDv7 合同。凭据仅由启动器通过 `QUAZONAI_MCP_TOKEN`
传入，不提供 token 命令行参数，也不要写入对话、Issue 或日志。生产必须 HTTPS origin，
不能带 userinfo、额外路径、query 或 fragment；开发 HTTP 还须显式 `--development-http`
并使用字面 loopback IP。禁止环境代理、Cookie、重定向和自动重试。

当前 tools/list **只有两个真实工具**：`research.get_brief {brief_id}` 读取启动绑定的
同项目 FROZEN Brief；`run.get {run_id}` 读取当前 Mission 的原生 RunSnapshotV1。
每次调用重新检查 MISSION 类型、RUN_READ/RESEARCH_READ、项目/Run/周期/Attempt、
凭据到期与 Run deadline；不同任务、草稿、未知字段、非 UUIDv7、撤销和接管均拒绝。
启动参数本身不是授权，服务端事务才是最终事实源。其他 DESIGN B3 工具仍未接通，
不允许用任意 HTTP、Shell、SQL 或原始数据访问来替代它们。

stdout 只传原生 MCP，日志使用 stderr。最多同时4次工具调用、不排等待队列；
连接超时3秒、单 HTTP 请求10秒、单工具15秒，均受任务/凭据到期约束；每个响应累计
最多1 MiB，整个 stdio 会话最多输入8 MiB（不是单帧或模型 token 预算）。断开客户
端或到期关闭协议不会取消远端 Run。错误仅含安全代码/HTTP状态，不返回上游正文。

原生协议回归：`cargo test --locked -p server --test mcp_transport`。这些测试使用真实
SDK、stdio 子进程和 HTTP 故障服务；故障服务不等于实际 PostgreSQL Mission 发证、
完整 Codex 研究工具循环或生产隔离验收。完整发证/启动/实验/评估链仍按 DESIGN 验收。

## 原生组件与合同验证

```sh
cargo run --locked -p job -- verify-native --output NEW_DIRECTORY
cargo run --locked -q -p contracts --example generate
cargo run --locked -q -p server -- openapi
```

`job` 命令只运行固定 Rust Clarabel/Nautilus/Arrow fixture，输出不可交付；不能生成正式资格或目标包。`contracts` 生成共享 DTO；`server openapi` 生成实际 HTTP 路由合同。原生 Codex 兼容性命令仍为 `cargo run --locked -p job --example codex_contract`，需 `CODEX_NATIVE_BIN` 与不存在的 `CODEX_PROBE_DIR`；不是完整模型工具循环。

## 开发测试

先选择仓库固定的编译器补丁，避免发行版 Cargo 或本机覆盖设置绕过 `rust-toolchain.toml`：

```sh
rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy
rustup run 1.98.1 rustc -Vv
rustup run 1.98.1 cargo fmt --all -- --check
rustup run 1.98.1 cargo check --locked -p contracts -p domain -p store -p server
```

下列 Cargo 命令在同一固定工具链运行；存在本机覆盖时使用 `rustup run 1.98.1 cargo`。
原生安装/版本/检查日志才是执行证据，配置文件里的版本不是已运行的编译器。

仅对可丢弃的 PostgreSQL18 + PGMQ1.10.0 使用：

```sh
DATABASE_URL=postgres://TEST_USER:TEST_PASSWORD@127.0.0.1:55432/postgres \
  cargo test --locked -p store -p server
```

SQLx 创建独立测试数据库并执行提交的迁移；不要使用生产 DATABASE_URL。HTTP 测试运行真实 Axum、Argon2、TOTP、AEAD、PostgreSQL Session Store，并另测非 owner 角色与 loopback TCP。它们不是完整研究/组合/交付的验收结果。

### Cycle/Run 事务组合与 HTTP 合同回归

```sh
# 在上述可丢弃 PostgreSQL/PGMQ 环境中，检查首任务与调用方事务的共同提交/回滚。
cargo test --locked -p store --test atomic_cycle_admission --test run_lifecycle

# 直接生成真实 HTTP OpenAPI 并检查引用；本测试本身不连接数据库。
cargo test --locked -p server --test http_openapi_references
```

内部服务可将原生 SQLx 事务交给 `Store::enqueue_run_in_transaction`；成功后取回事务，
完成其余领域写入并提交，不能把尚未提交的返回值发给用户或 Worker。失败不返回事务，
由 SQLx 回滚其拥有的范围。此接口复用原有准入，不是新增 `qz cycle start` 命令、
Brief 冻结接口或 Agent 通用执行工具。上述是验证命令，不是已有通过记录；完整链路
仍须满足 DESIGN 的数据/原生能力、权限及 W0–W8/T01–T42 合同。

### 完整迁移命令的提交边界

`cargo run --locked -p server -- migrate --application-role '<已创建的运行角色>'`
在一个专用连接/外层事务内运行完整领域与原生 session DDL、验证表合同并授予
运行角色 DML 权限，最后一次性提交。执行前停止应用写入并完成备份；这不是
零停机承诺。不再额外运行独立的 `PostgresStore::migrate()`。角色不存在、既有
session 表不兼容或任一授权失败时，不保留半次升级及 epoch 失效副作用。
已有数据/会话不会被删表“修复”。网络在 COMMIT 阶段断开时结果未知，应在
主库重连后通过原生迁移记录和权限复核，不直接宣称回滚或重复恢复备份。

## Run 查询、持久事件与取消 HTTP

以下路由包含于原生生成的 `server openapi`，不需要数据库直连权限：

| 路由 | 语义 |
|---|---|
| `GET /api/v2/runs?project_id=UUID&state=QUEUED&limit=50&cursor=UUID` | 稳定 UUID 顺序的受限分页；limit 为1–100 |
| `GET /api/v2/runs/{id}` | 同一事务快照的 state/revision/last_event_seq |
| `GET /api/v2/runs/{id}/events` | `text/event-stream`；`Last-Event-ID: <run UUID>:<decimal seq>`；不存在 cursor 时从0开始 |
| `POST /api/v2/runs/{id}/cancel` | body为 `{"schema_version":1,"expected_revision":"当前版本"}`，另带 Idempotency-Key；接受后202，版本过期409 |

浏览器使用现有同源私有会话；取消是写操作，必须在最近五分钟内完成 TOTP 认证，
超时先重新认证，读取不受该近期窗口限制。机器需要该项目的 RUN_READ 或 RUN_CANCEL；Mission
只读自身 Run，不能扩大到其他项目或取得操作员授权。取消仅停止计算，不表示下游
交易停止。尚未 dispatch 的任务可以直接 CANCELLED；已涉及远端的任务先显示
CANCEL_REQUESTED，须确认远端终止后才能终结，真实失败保留 FAILED。

SSE 每条 id 与 data.seq 对应。按最后收到的 id 重连，客户端对序列去重；过期或超前
cursor 在开始流之前410，错误 UUID/数字形状422。认证撤销、版本不兼容等发生在
已建立的流中时发送不带新 cursor 的 reset-required，客户端应重新认证/读快照。
兼容的 schema-v1 新事件保留事件名和公开 JSON envelope，推进游标但不更新未知的
状态投影；已知状态事件仍严格解析。未知主版本不是可跳过事件，应升级客户端。
每个 API 进程最多32条流，连接满额429；连接60秒后重连以更新认证。关闭浏览器不会
取消任务或确认队列。该节不声明远程 CLI/MCP 或完整 Worker 执行器已实现。

### 任务与迁移恢复边界

冻结 InputSet 保存当时的证据，不能延长数据许可。每次新任务准入和唯一首次发送
均在领域事务中重新锁定并核对当前授权；撤销后既有未知任务仍可对账，原始回执仍
可重读，不能盲目退款或重新发送。已经领取但从未发送、且租约和 deadline 均过期的
任务终结为 FAILED/DEADLINE_EXCEEDED；这不是远端失败或停止证明。

IMPORT/EXPORT/DATA_VALIDATE 可由受信任内部服务以无 Cycle 路径准入；实验预约
固定为零，仍需有界资源与项目并发限制。没有向 Agent 开放通用无预算执行入口，
也不把这个内部准入能力说成导入/导出业务已完成。

迁移命令在专用连接取消请求级 statement_timeout，保持五秒锁等待限制；迁移完成
或失败后关闭该连接。业务连接的请求超时不改变。升级前停止写入并备份；不要在
生产库用零散 SQL 文件代替完整迁移入口。
