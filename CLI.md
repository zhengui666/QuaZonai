# CLI 命令

完整产品合同在 DESIGN。当前已实现原生验证、逐轮 Store、浏览器认证、Project/机器身份和不可变研究准备 HTTP 控制面；研究/组合/交付命令仍待实现，不提供绕过 API 的手工 SQL 业务路径。

## 原生科学任务入口

`job` 是受信任运行时启动的一次性计算进程，不是浏览器/Agent 的任意命令执行代理。每次调用只运行一个任务，API/Worker 不在本进程内嵌入 Nautilus。Clap 原生帮助：

```sh
cargo run --locked -p job -- --help
cargo run --locked -p job -- allocate < tests/contracts/allocation-input.json
```

第二条是明确标记的合成两资产数值回归输入，不产生生产资格或交付权。`allocate` 使用真实 Clarabel 求解并检查存储用十进制目标；无解/失败不输出备用权重，必须检查 `solver_status` 而非只看进程退出码。

已有受授权只读 Nautilus Parquet 快照、实际 Wasm 模型和相应冻结请求文件时，运行时使用以下入口；路径不是 HTTP/MCP 请求字段：

```sh
job forecast --catalog /input/catalog --model /input/model.wasm < forecast-request.json
job simulate --catalog /input/catalog < simulation-request.json
```

请求分别是 `NativeForecastRequestV1`、`NativeSimulationRequestV1`，由同一 Rust 合同生成。stdin 上限8MiB；stdout为完整JSON，计算失败为非零退出码及安全的 `QZ_NATIVE_JOB_FAILED`，不回显原生异常、路径或输入。`--catalog` 只允许运行时的已登记只读挂载，`--model` 不接受软链/FIFO/超限文件；外层仍须配置真实进程、文件系统、网络和资源隔离，不能直接用这些本地参数授予Agent宿主访问权。

`forecast` 保留未完成标签与指标预热的 null+reason，Wasm没有宿主导入且受fuel/内存/栈限制。`simulate` 在一个原生账户执行全部资产的冻结目标，先确认减仓成交再提交增仓，保留原生费用、数量步长及独立结果。公开 `returns_kind=PORTFOLIO_DAILY` 仅含原生权益快照的UTC日收益，绝不使用单仓收益回退；日内数据不足时 `returns_status=INSUFFICIENT_DATA`、`returns_reason=PORTFOLIO_DAILY_RETURNS_UNAVAILABLE`，不是0收益。跨日全现金的真实0收益可以为OK，但仍须符合评估最小样本要求。

验证这些入口及native协方差、OLS校准、Walk-forward/CPCV使用：

```sh
cargo test --locked -p job --tests
```

目录许可/PIT、来源、Run/Attempt、独立评估、资格及审批仍由上层可信服务核验；成功退出不等于 REAL、PASS 或 Issue62 完整验收。

## 原生 Runtime 网关与受管 job

网关使用自己的原生 SQLite 任务日志和 Docker Unix socket，不连接研究 PostgreSQL、不读取 Codex profile、不持有真实券商权限。配置、凭据文件、原生镜像装配、TLS 与隔离验收步骤见 [runtimes/native/README.md](runtimes/native/README.md)。

```sh
cargo run --locked -p runtime -- openapi
cargo run --locked -p runtime -- doctor --config /absolute/runtime.json
cargo run --locked -p runtime -- serve --config /absolute/runtime.json
```

`doctor` 经真实 Docker 与已登记镜像验证能力；`serve` 支持已有任务状态、唯一请求重放、受限对象传输与取消。Docker 暂不可用不抹去 SQLite 中的既有身份，也不构成重新执行许可。对外只允许同机 TLS 反向代理连接 loopback 监听端口；不要将明文端口或 Docker socket 暴露给浏览器/Agent。

生产镜像固定调用 `job run-bounded`，从只读 `/input/spec.json` 读取剩余绝对截止时间和墙钟上限，交给原生 GNU timeout 再执行 `job execute`。编译、目录验证、预测、组合求解与共享资金模拟使用封闭 `NativeTaskParametersV1`；HTTP/MCP 没有任意命令、环境、挂载或路径字段。受信任本机诊断可使用 `job execute --input-root /absolute/input --output-root /absolute/output`，该 CLI 覆盖不能变成远程调用者权限。

`GET /runtime/v1/jobs/{external_job_id}/artifacts/{storage_ref}` 返回已封口 manifest 中的精确对象：原生 Wasm 为 application/wasm，登记的原生 JSON 报告为 application/json；不返回任意路径或跨任务对象。storage_version 固定原生版本1，schema/kind/media_type 由同一 Rust 登记表绑定，未知输出不是可采纳的科学证据。

普通 Runtime 单元/SQLite/HTTP 测试不证明 OCI 隔离；`.github/workflows/native-runtime.yml` 对精确源码启用独立必跑的 `native-oci` 测试。缺 Docker、固定镜像或 cgroup 前提会失败，不能按跳过处理成通过。取消时只有原生进程已停止且晚到 CREATE/START 已被持久身份屏障阻断才报告 CANCELLED，404 或超时不等于取消确认。

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

## 集成配置与只写凭据 HTTP

`POST /api/v2/settings/credentials` 接收 `{intent:{schema_version:1,purpose,label},value}`。
purpose 仅 RUNTIME、DOWNSTREAM、CUSTOM_PROVIDER、TLS_CA；value 只写，不返回、记日志或
写入 SQL/幂等回执。返回的 id 是原生不可变加密对象引用；同 key、同 intent、同原始值才重放，
不同值409。凭据轮换创建新对象，不能覆盖旧值。TLS_CA 须为1–65536字节ASCII、原生TLS实现可接受的非空PEM
证书集合；RUNTIME须为32–8192个可打印非空白ASCII字节，DOWNSTREAM / CUSTOM_PROVIDER为1–8192字节。
最小长度不是熵保证；旧短Runtime凭据须在真实运行端轮换，并通过正式凭据登记和Runtime更新入口绑定后重新探测。
不要把真实值放在CLI参数、Issue或Git，也不得补字符或手工改SQL绕过验证。

Runtime 使用 `GET/POST /api/v2/integrations/runtimes` 和 `GET/PATCH /{id}`；Downstream 使用
对应的 `/api/v2/integrations/downstreams`。create 传 schema_version、严格 configuration 和
credential_ref；update 另带 expected_revision，credential_ref=null 表示保留原对象。
PINNED_CA 必须关联 TLS_CA 用途的 ca_certificate_ref；更新为空表示保留原CA，明确选择
SYSTEM_CA 则只移除绑定，不删除旧加密对象。配置查询只显示 credential_configured/ca_configured，
不回传对象路径或凭据引用。注册配置不访问网络，enabled 不代表 readiness；实际 probe 单独执行。

所有这些写操作仍需近期 Operator 浏览器，或 CLI 的一次性完整命令 grant。新增 operation 为
INTEGRATION_SECRET_REGISTER（request 仅 intent，不含 value）、RUNTIME_CREATE/UPDATE、
DOWNSTREAM_CREATE/UPDATE。未提供 grant 的 DOCTOR_READ 只可读无秘密配置，不可写。
Production 只接受 HTTPS origin；literal-loopback HTTP 还须配置和部署双方显式允许。
本节给出实际 HTTP 合同，不把尚未实现的专属远程 CLI 子命令或 Runtime 网络执行说成已验收。

回归入口为 `cargo test --locked -p domain --test settings`、
`cargo test --locked -p integrations --test secret_identity` 及隔离 PostgreSQL 上
`cargo test --locked -p store --test settings` / `cargo test --locked -p server --test settings_http`。

## Runtime 探测与版本化 readiness

`POST /api/v2/integrations/runtimes/{id}/probe` 接收 schema_version=1、expected_revision，
需要近期人类认证或 RUNTIME_PROBE 单次 CLI grant。响应200表示探测已记录；必须检查
resource.outcome.status，UNAVAILABLE 不是可执行。`GET /api/v2/integrations/runtimes/{id}/readiness`
返回当前配置版本、最近观察、是否失效和精确 available_job_kinds。相同 key 重放原结果，不刷新有效期。

`serve` 的 `RUNTIME_TARGETS` 是仅由部署管理的 JSON 数组，默认 `[]` 拒绝所有出站探测。例如：

```json
[{"origin":"https://runtime.example:8443","addresses":["10.0.0.4:8443"]}]
```

每项最多16个批准地址，端口必须与origin一致，主机名仍用于原生TLS的Host/SNI验证；不会再次
依赖系统DNS或环境代理。配置保存不修改此部署允许列表。明确开发模式才允许literal-loopback
HTTP；PINNED_CA必须绑定原生CA证书，缺失时不回退到SYSTEM_CA。metadata/link-local等目标拒绝。

观察有效期最长60秒，配置变更和新的失败观察立即使旧成功不再准入。产物是运行环境审计，
不是Alpha资格、数据授权或科学PASS。原生验证命令：`cargo test --locked -p server --test runtime_transport`、
隔离PostgreSQL上的`cargo test --locked -p server --test runtime_http`及`cargo test --locked -p store --test runtime`。

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

tools/list 的实际入口包含 `research.get_brief {brief_id}`、`run.get {run_id}`、
`artifact.submit` 和 `experiment.propose`。前两个读取精确 FROZEN Brief/本 Mission Run；
写工具调用同一 HTTP 产物与实验事务，不产生科学成功或资格。每次调用重新检查 MISSION、
RUN_READ/RESEARCH_READ、项目/周期/Run/Attempt、撤销、凭据期限与任务截止。

`artifact.submit` 参数为 `{schema_version:1,kind,workspace_relative_path,idempotency_key}`，
kind 为 CODE/PARAMETERS/REPORT。启动器另外通过 `--workspace-root /absolute/worktree`
明确授予本 Mission 目录；参数不是 Agent 工具字段，未配置不猜当前目录。还需 ARTIFACT_SUBMIT；
只读取这个已打开目录句柄内的非隐藏单链接普通 UTF-8 文件，最多2MiB，拒绝软链、FIFO、
越界及绝对路径。内容通过现有 HTTP 发布并绑定实际作者 Run/Attempt，保持 SYNTHETIC/RESEARCH。

`experiment.propose` 参数为 `{idempotency_key,proposal:ExperimentProposalV1}`，还需
EXPERIMENT_SUBMIT。proposal.cycle_id 必须等于启动绑定；Family/提案/参数/代码引用和额度
由现有 Store 核对，返回初始 PENDING 和真实作者身份，不启动科学 Run。请求响应丢失时复用原key，
同key不同文件字节或提案字段仍409；不要自动换键。其他 DESIGN B3 工具只在相应真实服务接通后
登记，不以任意 HTTP、Shell、SQL 或原始数据访问代替。启动参数不是授权事实，服务端事务最终裁决。

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
