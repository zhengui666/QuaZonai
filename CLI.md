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

## 原生组件与合同验证

```sh
cargo run --locked -p job -- verify-native --output NEW_DIRECTORY
cargo run --locked -q -p contracts --example generate
cargo run --locked -q -p server -- openapi
```

`job` 命令只运行固定 Rust Clarabel/Nautilus/Arrow fixture，输出不可交付；不能生成正式资格或目标包。`contracts` 生成共享 DTO；`server openapi` 生成实际 HTTP 路由合同。原生 Codex 兼容性命令仍为 `cargo run --locked -p job --example codex_contract`，需 `CODEX_NATIVE_BIN` 与不存在的 `CODEX_PROBE_DIR`；不是完整模型工具循环。

## 开发测试

仅对可丢弃的 PostgreSQL18 + PGMQ1.10.0 使用：

```sh
DATABASE_URL=postgres://TEST_USER:TEST_PASSWORD@127.0.0.1:55432/postgres \
  cargo test --locked -p store -p server
```

SQLx 创建独立测试数据库并执行提交的迁移；不要使用生产 DATABASE_URL。HTTP 测试运行真实 Axum、Argon2、TOTP、AEAD、PostgreSQL Session Store，并另测非 owner 角色与 loopback TCP。它们不是完整研究/组合/交付的验收结果。

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

浏览器使用现有同源私有会话。机器需要该项目的 RUN_READ 或 RUN_CANCEL；Mission
只读自身 Run，不能扩大到其他项目或取得操作员授权。取消仅停止计算，不表示下游
交易停止。尚未 dispatch 的任务可以直接 CANCELLED；已涉及远端的任务先显示
CANCEL_REQUESTED，须确认远端终止后才能终结，真实失败保留 FAILED。

SSE 每条 id 与 data.seq 对应。按最后收到的 id 重连，客户端对序列去重；过期或超前
cursor 在开始流之前410，错误 UUID/数字形状422。认证撤销、版本不兼容等发生在
已建立的流中时发送不带新 cursor 的 reset-required，客户端应重新认证/读快照。
每个 API 进程最多32条流，连接满额429；连接60秒后重连以更新认证。关闭浏览器不会
取消任务或确认队列。该节不声明远程 CLI/MCP 或完整 Worker 执行器已实现。
