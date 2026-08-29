# QuaZonai 用户运行操作模型

> 本文件是 [`DESIGN.md`](DESIGN.md) 的用户运行视图，不定义新的产品状态或技术事实；冲突时以 `DESIGN.md` 为准。

## 1. 用户真正需要做什么

QuaZonai V1 是单用户、自托管私有工作台。正常 Research Program 生命周期只有两类常规人工操作：

1. **提出 Research Idea**；
2. **审批系统推荐的 Paper / Live Candidate Handoff**。

其余动作都属于低频 Administration 或故障处置，不应成为每个 Program 的必经步骤。Operator 登录属于部署访问边界，也不增加 Research Program 的常规业务步骤。

完整用户视图：

```text
可选 Operator 登录（认证启用时）
→ 提出 Idea
→ 系统自治研究
→ 系统构建 Alpha / Portfolio
→ 有实质价值时通知审批
→ 用户批准或拒绝
→ 系统发布 Handoff Package
→ 独立下游运行
→ QZ 接收 Forward Feedback
→ 系统自动继续研究或进入 Cooling
```

## 2. 运行责任帽子

系统只有一个人类操作者，但不同场景承担不同责任帽子。

### 2.1 Research Operator

常规操作：

- 提出 Idea；
- 必要时回答一次澄清；
- 查看 Research Pulse / Observatory；
- 审批唯一推荐 Candidate。

不需要：

- 手工选模型；
- 手工选 Alpha；
- 手工调权；
- 手工运行回测；
- 手工选择第二名候选；
- 手工管理 Codex Mission。

### 2.2 Administrator

低频操作：

- 决定是否启用单 Operator 登录，并在启用时配置 Google Authenticator-compatible TOTP；
- 决定是否在受控设备上启用 `Trust this browser`；
- 完成首次 `RESEARCH_READY`；
- Codex 登录/认证；
- 在 Runtime Configuration 配置 Codex provider/model 与 Worker limits；
- 配置 Data Source / Universe / Mandate / Capital Context；
- 配置 Paper/Live downstream；
- 安装/激活/停用 research/data/handoff plugin；
- 处理 storage、worker、evaluator、connector 故障；
- Pause/Resume/Archive/Restore Program。

### 2.3 QuaZonai Automation

自动负责：

- Charter 解析与 overlap detection；
- Mission Graph / scheduling；
- Codex Mission execution；
- Data acquisition 与 quality validation；
- Feature/Alpha/Calibration research；
- Search Ledger / Evidence Exposure；
- Sealed Promotion Evaluation；
- Alpha Library qualification；
- Portfolio Assembly；
- Material Improvement Gate；
- Approval freshness；
- Candidate Bundle；
- Handoff state；
- Feedback validation；
- Degradation Monitoring / Research Wake-up。

### 2.4 Independent Downstream

独立下游负责：

- Paper/Live runtime；
- market/broker connectivity；
- orders/fills/positions/accounts/NAV；
- execution risk；
- runtime stop/recovery；
- 把约定的 Feedback Package 返回 QZ。

QZ 不拥有这些状态。

## 3. 首页

Home = `Action Center + Research Pulse`。

### 3.1 Action Center

仅在需要人时出现：

- Paper / Live Approval；
- Approval 即将过期；
- Idea 澄清未完成；
- 必须人工处理的关键 Admin 事件。

优先级：

```text
资金交付审批
→ Idea 澄清
→ 证据/系统完整性 Admin 事件
→ 其他信息
```

### 3.2 Research Pulse

无人工任务时首页展示：

- ACTIVE / COOLING / PAUSED / BLOCKED Programs；
- 最近晋级 Alpha；
- Portfolio readiness；
- 等待 downstream feedback 的候选；
- 最近 material evidence changes。

Token、命令数、trial 数不展示为研究成果。

## 4. Idea Composer

### N0：提交 Idea

用户输入自然语言 Idea。

系统自动解析：

- Research Question；
- Market / Universe；
- Horizon；
- Data Domain；
- Explicit Exclusions；
- 与已有 Program 重叠程度。

若存在重大歧义，最多一次提出 1–3 个问题。

### N1：重叠处理

系统可能建议：

- 复用已有 Program；
- 在原 Program 创建 Branch；
- 创建关联的新 Program。

用户可以在 Idea 提交阶段接受或坚持独立创建；这不增加第三类常规操作。

### N2：冻结 Charter

点击 `Start Research` 后 Charter 永久冻结。之后普通用户不能用聊天不断改变研究方向。

## 5. Autonomous Research

### N3：Program ACTIVE

系统自动创建有限 Mission：

```text
Planning
Data Requirement
Data Quality
Hypothesis
Feature Research
Alpha Discovery
Calibration
Robustness
Promotion Review
```

Codex 运行状态只在 Observatory 展示，不需要用户确认 shell/网络权限。

### N4：COOLING

当没有信息增量时自动进入 `COOLING`，等待：

- 新市场数据；
- 新 Dataset Revision；
- 新 Forward Feedback；
- 新 Sealed Episode；
- Alpha/Portfolio degradation；
- 新 data capability；
- 新颖 hypothesis。

Cooling 是正常状态，不是失败。

### N5：BLOCKED

典型：

- 缺少必要 Data Capability；
- Codex 不可用；
- Sealed Evaluator 不可用；
- 关键 storage 不可用。

若是需要新增 license/credential 的数据源，只在 Administration 创建任务，不反复中断普通 Research workflow。

## 6. Alpha Library

用户主要是查看，不需要手工管理研究资产。

可见状态：

```text
SHADOW
ACTIVE
WATCH
QUARANTINED
RETIRED
```

旧 Qualification 不会“恢复”。若失效 Alpha 后来重新成立，系统创建新的 Qualification Version。

Shadow Alpha 可以参与受限组合贡献研究，不能直接用于 Handoff。

## 7. Portfolio Lab

系统基于启用的 Portfolio Mandate 和 Alpha Library 自动决定何时创建 Portfolio Program。

普通用户不：

- 手工拖入 Alpha；
- 改 Candidate 权重；
- 改优化器；
- 在 Approval 时改变 Mandate。

Portfolio Lab 用来解释：

- 哪个 Mandate；
- 哪些 Alpha roles；
- redundancy / marginal contribution；
- multi-universe exposures；
- risk/cost/capacity；
- 为什么当前 Candidate 是推荐版本。

## 8. Candidate Approval

### N6：Approval Inbox

只有通过全部 Promotion Gate + Material Improvement Gate 的唯一推荐 Candidate 才进入。

每张卡显示：

```text
Paper / Live purpose
Mandate Version
Capital Context
Candidate Version
Evidence Summary
Risk / Drawdown / Turnover / Cost / Capacity
Selected logical downstream
valid_until
freshness state
```

系统只展示兼容的 downstream。

### N7：Approve

Paper：允许把当前 Package 交给指定 Paper consumer。

Live：必须基于完整 Paper Forward Evidence 和新的 Promotion Evaluation 单独生成；Paper Approval 不预授权 Live。

Approve 后系统创建 Handoff Offer。

### N8：Reject

选择结构化 reason code，可附 note。

Reject 后：

- 不自动递补第二候选；
- 不改写 Charter；
- 必须有新证据/实质改进才可能再次出现 Approval。

## 9. Approval Freshness

`PENDING` Approval 可能变为：

- `STALE`：已知关键依赖变化；
- `EXPIRED`：超过最长有效期。

二者都只读，不能继续批准，也不能简单延长；系统需要基于当前事实重新生成新 Snapshot。

## 10. Handoff

### N9：AVAILABLE

下游尚未领取时，用户可以 `Withdraw offer`：

```text
PUBLISHING → REVOKED
AVAILABLE  → REVOKED
```

撤回不判 Candidate 失败，也不删除 Approval。

### N10：CLAIMED

一旦下游领取：

- QZ 不再提供 revoke runtime；
- QZ 不提供 stop/undeploy/close position；
- 用户必须在独立 downstream 处理运行与资金操作。

QZ 可以发布 `WithdrawalAdvisory` / `DegradationAdvisory`，但不声称下游已停止。

## 11. Feedback

正常：

```text
FEEDBACK_PENDING
→ FEEDBACK_IN_PROGRESS
→ FEEDBACK_PARTIAL
→ FEEDBACK_COMPLETE
```

异常：

```text
FEEDBACK_STALE
FEEDBACK_INCOMPLETE
FEEDBACK_INVALID
CONSUMER_UNREACHABLE
```

这些异常不自动等于 Candidate 失败。

只有 complete + contract-valid 的 Paper Feedback 才能进入 Live Promotion。

## 12. Degradation

QZ 监控研究有效性，不监控订单执行。

```text
HEALTHY → WATCH → DEGRADING → INVALIDATED
```

达到 Degradation Policy 门槛时自动 wake Research Program，创建诊断 Mission，产生新 Qualification/Candidate。

不会自动：

- 停止下游；
- 平仓；
- 调仓；
- 替换已批准 Package。

## 13. Pause / Resume / Archive

### Pause

- 停止创建新 Mission；
- 不响应自动 wake；
- 保留所有研究事实；
- 不影响 downstream runtime。

### Resume

系统重新判断：

```text
ACTIVE / COOLING / BLOCKED
```

不重置 Search Ledger、Exposure 或 consumed Episodes。

### Archive

退出 active research pool，不再自动研究或生成 Approval；历史、Alpha、Package、Handoff、Feedback 全保留。

### Restore

恢复后继续继承全部历史研究负担，绝不当作 fresh Program。

## 14. Administration

### 14.1 Readiness

分别显示：

```text
System
Research
Paper handoff
Live handoff
```

首次安装只要求达到 `RESEARCH_READY`。

### 14.2 Operator Authentication

QuaZonai V1 只有一个部署 Operator。它不是业务用户系统、tenant 或 RBAC。`QUAZONAI_AUTH_ENABLED=false` 保留 direct access；只有显式设为 `true` 才启用下述登录门。

认证启用后的 Web 登录输入：

```text
QUAZONAI_AUTH_ENABLED=true
+ QUAZONAI_AUTH_USERNAME
+ QUAZONAI_AUTH_PASSWORD
+ Google Authenticator-compatible 6-digit TOTP
```

TOTP setup key 来自 `.env` 的 `QUAZONAI_AUTH_TOTP_SECRET`。在 Google Authenticator 中选择 **Enter a setup key**，使用该值并选择 **Time based**。TOTP secret、Operator password、cookie key 和 machine API token 都属于启动级 secret，不能放进聊天、截图、事件或日志。`QUAZONAI_AUTH_COOKIE_KEY` 必须独立生成且不能与 `QUAZONAI_MASTER_KEY` 相同；`QUAZONAI_API_TOKEN` 必须使用 RFC 6750 `b64token` 可安全写入 Authorization header 的 ASCII 字符集。

登录时可以勾选 **Trust this browser**。选中后服务器在当前浏览器 profile 写入长期 HttpOnly trusted-browser credential；短期 session 过期后，只要该 trusted credential 仍有效，就会自动恢复新 session，用户不再输入 password/TOTP。默认 trusted-browser 有效期 30 天，默认短 session 为 12 小时。

只应信任自己控制的浏览器 profile。公共/共享电脑不要勾选。正常 Sign out 会同时清除 session 与 trusted-browser credential、写入当前浏览器 profile 的 `HttpOnly`/`SameSite=Strict` logout barrier 和 sealed local issuance epoch，并使当前 API 进程中已经打开的事件流在下一轮认证检查时停止。下一次成功的 password + TOTP 登录只清除 barrier，保留 epoch 并把新 cookie 绑定到它，因此 logout 已先应用而旧 login/trusted-browser renewal response 后到时，旧 cookie 仍不能恢复访问。已认证退出还推进当前 API 进程的 global browser-cookie issuance generation；匿名/public logout 仅改变请求者浏览器的 local barrier/epoch，不会阻塞其他浏览器登录或续期。浏览器 credential 还绑定每个 API runtime 新生成的随机 issuance epoch，因此 API 重启会有意使所有已有 browser session/trusted-browser credential 失效，避免重置后的 generation 重新接受退出前 credential。退出请求失败时 UI 不会伪装成已退出。

失窃/不再可信设备的处置：

1. 立即轮换 `.env` 中 `QUAZONAI_AUTH_COOKIE_KEY`；
2. 重启 API；
3. 全部现有 session 与 trusted-browser credential 随即失效；
4. 所有浏览器必须重新执行 password + TOTP 登录。

其他 credential 轮换：

- `QUAZONAI_AUTH_TOTP_SECRET`：所有旧 Authenticator code 失效，需要重新配置 Authenticator；
- `QUAZONAI_API_TOKEN`：旧 CLI/automation Bearer token 失效；
- `QUAZONAI_AUTH_PASSWORD`：之后的完整登录使用新密码，但已有 cookie 仍由 cookie key 控制，所以设备级紧急撤销应轮换 cookie key。

`QUAZONAI_ENV` 只能为 `development`、`test` 或 `production`（忽略大小写与首尾空白）。认证启用时，`QUAZONAI_AUTH_PUBLIC_ORIGIN` 与浏览器 `Origin` 都按 browser-origin 规则 canonicalize 后精确比较：scheme/host 小写、Unicode host 使用 IDNA ASCII、IPv6 压缩并保留 brackets、默认端口省略、非默认端口保留。production 必须为 HTTPS，反向代理/Tunnel 应在可信 TLS 层终止 HTTPS，并把该外部 Origin 写入 `.env`。

若 TLS reverse proxy/tunnel 把浏览器请求转发到 API，配置 `QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS` 为 **API/容器实际看到的 direct proxy peer** 的精确 IP/CIDR，不要使用 `/0` 或包含普通客户端的宽泛网络。只有 proxy 与 API 直接同机运行或使用 host network 时，`127.0.0.1/32` 才可能正确；Compose 容器通常看到 Docker bridge/gateway 地址，应按实际连接来源配置。仅在该匹配成立时，登录限流才读取一个 `X-Forwarded-For`：它会从右向左移除受信 proxy hop，并使用最近的非受信 literal IP。proxy 必须 append 自己实际观测到的 peer（例如 Nginx 的 `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;`），或以已验证的 client IP overwrite 此 header，不能只转发客户端送入的值。没有此配置、peer 不匹配，或 header 缺失/重复/非法时，QuaZonai 有意忽略 header 并以 direct peer 限流。Compose 已明确运行 Uvicorn `--no-proxy-headers`；手工启动也必须显式传入此 flag，且不能传入 `--proxy-headers` 或设置 `FORWARDED_ALLOW_IPS`，否则 Uvicorn 会在 QuaZonai 校验前改写 direct peer。

QuaZonai 提供的 Web workbench 会返回 `Content-Security-Policy: frame-ancestors 'none'` 和 `X-Frame-Options: DENY`，不能嵌入任何 iframe。反向代理不得移除或放宽这两个响应头；这项控制补充而不取代 cookie `SameSite` 和 Origin 校验。

`QUAZONAI_AUTH_ENABLED=false` 在所有环境保留 direct access，此时 auth credential/TTL 值均 dormant，应保持 loopback-only 或使用另一个明确可信的访问边界。设为 `true` 后，任一 Operator auth 必需值缺失或非法都会使 API fail closed；启用认证的 production 还要求 HTTPS 并自动使用 Secure cookie。连续失败登录会触发 1–5 秒的短退避，但不会形成持久账户锁定；被限制的请求仍显示统一的无效凭据错误。

Operator Authentication 启用时，CLI/automation 不使用 Web cookie、密码或 TOTP，而是从环境读取符合 RFC 6750 `b64token` 语法的 `QUAZONAI_API_TOKEN`；认证关闭时不要求该 token。Downstream consumer 的 Bearer service token 仍独立，只能操作其自身 Handoff/Feedback 合同。

### 14.3 Codex / Runtime Configuration

Administration 是 Codex runtime 配置的事实入口，显示并允许修改：

- Codex executable/version 与 login 状态；
- 可选 `model`；
- 可选自定义 OpenAI-compatible `Base URL`；
- 可选 Codex API key；API key 只写、永不回显；
- App Server preflight；
- Agent worker health。

自定义 Base URL 必须是绝对 HTTP(S) URL，不能把 username/password、query token 或 fragment 嵌入 URL。配置了 Base URL/API key 时，Mission 使用独立 Codex model provider；未配置时继续使用持久 `CODEX_HOME` 中的标准 Codex 登录。已有 API key 时更改 Base URL，必须重新输入该 endpoint 对应的 key 或显式清除旧 key。

Codex API key 由 `QUAZONAI_MASTER_KEY` 使用 AES-256-GCM 加密后保存到 PostgreSQL。Secret/token 不在 Web 展示，也不写入事件 payload；运行时通过受信任 runner 的 one-shot credential broker 交给 Codex provider auth，不进入 App Server/Mission 环境变量。

`.env` 只负责启动级基础设施与 Operator access：运行环境、PostgreSQL、master key、`QUAZONAI_AUTH_ENABLED`、Operator username/password/TOTP、browser cookie key、CLI machine token、public origin、存储根目录和 HTTP port。Codex model/API key/Base URL 不由 `.env` 配置。

Runtime Configuration 使用 revision + 幂等 mutation：页面保存携带当前 revision，若其他请求已先更新则返回冲突并要求刷新，不覆盖较新配置；网络重试复用同一个 `Idempotency-Key`，不会重复修改 revision、重复写事件或重复保存 secret。

### 14.4 Worker limits

以下运行参数在 Runtime Configuration 中由管理员维护，而不是 `.env`：

- plugin wheel 最大字节数；
- plugin validation timeout；
- runtime bundle build timeout；
- plugin job timeout；
- research Mission job timeout；
- job poll interval（最小 `0.01s`）；
- job lease duration。

Worker 在领取后续 job 时读取最新配置；修改这些值不要求重建 Compose stack，也不改变已经运行中的 child process 的既定 deadline。

### 14.5 Data

管理员配置 approved Data Sources/Connectors。Codex 只调用批准的数据能力。

### 14.6 Universe / Mandate

Universe 和 Mandate 是长期配置。首次启用默认 Mandate；其他模板按需启用。

### 14.7 Capital Context

可以由 Administration 或 downstream feedback 提供现实资金规模快照；QZ 不读取 broker account/positions。

### 14.8 Downstream

配置逻辑系统及连接，例如：

```text
Nautilus Paper Lab
Nautilus Live Primary
External Validator
```

QZ 只验证 Handoff/Feedback contract，不检查其交易节点内部状态。

### 14.9 Plugins

只允许 Data/Research/Handoff plugins。运行时 side-by-side install/activate/drain/remove，不允许 execution broker plugins。

## 15. 故障呈现原则

产品必须区分：

- Research failure；
- Data quality failure；
- Codex/runtime failure；
- Sealed evaluator failure；
- downstream operational failure；
- negative market evidence。

不能用一个 `FAILED` 混合所有问题。

首页只提升需要人工处置的关键问题；低级 retry/command failure 留在 Level 2/3 Observatory。

## 16. 用户不应看到的内部实现

普通工作流不暴露：

- PostgreSQL row / job lease；
- Codex Thread ID；
- worktree path；
- MCP transport；
- plugin venv path；
- Sealed raw metrics；
- secrets；
- model hidden reasoning。

这些只在必要的 Level 3/Admin diagnostics 中以安全形式出现。

---

QuaZonai 的产品体验应始终保持：**用户提出投资研究问题，系统自治完成研究与组合，只有真正需要资本决策时再把一个可解释、不可变、经过独立验证的 Candidate 交给用户审批。**

## Remote Nautilus runtime operations (Issue 22)

- Research 与 Sealed Gateway 必须独立部署，分别配置 URL/token；生产环境必须使用 HTTPS/mTLS 边界，Core 中的 token 只能调用 research-only API。
- Gateway 镜像必须精确安装 `nautilus_trader==1.231.0`，持久化 ParquetDataCatalog，禁止暴露 live/order-management endpoint。
- 数据接入先调用 catalog ingest/validate，再把 `catalog_uri`、provider/license、Instrument scope、schema revision、quality 与 point-in-time 结果写入 Dataset Revision。
- 升级 Nautilus 版本时必须同时更新 pin、协议契约、真实 BacktestNode CI、Candidate Bundle conformance fixture；禁止静默漂移。
- `QUAZONAI_NAUTILUS_SEALED_*` 只能提供给 sealed evaluator worker，不得提供给 Research Mission/Codex 子进程。


### Remote Nautilus SOURCE_BUNDLE OS isolation

The remote Nautilus Gateway is a Linux-only execution boundary for Mission-authored `SOURCE_BUNDLE` code. Install `bubblewrap` (`bwrap`) and permit unprivileged user/mount/network namespaces for the Gateway service account. The Gateway fails closed when `bwrap` is unavailable. Each authored strategy runs with an empty network namespace and a mount namespace containing only trusted Python/runtime libraries plus the single disposable operation workspace; the Gateway data root, sibling catalogs, service environment and host home are not mounted. The Python AST gate remains defense-in-depth, not the isolation boundary.

### Sealed catalog provisioning and registration

Sealed observations never transit QuaZonai API, Codex workspaces, ordinary workers or Core job payloads. On the independently deployed SEALED Nautilus host, set `NAUTILUS_GATEWAY_ROLE=SEALED` and provision the typed `CatalogIngestRequest` from a local protected file with `quazonai-nautilus-sealed-provision --input /secure/release.json`. Core then queues metadata-only registration with `POST /api/v1/market-universe-versions/{universe_version_id}/sealed-dataset-revisions/register` (Idempotency-Key required). Only `sealed-evaluator` possesses the sealed Gateway credential; it calls the sealed-only catalog validation route and freezes the validated DatasetRevision metadata as `SEALED`. No sealed QuoteTick row is persisted in Core. Candidate promotion requires a second, non-overlapping sealed revision beyond the Alpha qualification episode and performs an independent portfolio-level sealed disclosure before creating a Paper Approval.


### SOURCE_BUNDLE OS sandbox prerequisite

Remote Nautilus Gateway hosts that execute `SOURCE_BUNDLE` artifacts must be Linux hosts with
Bubblewrap available. On Ubuntu 24.04, keep the system-wide AppArmor unprivileged-user-namespace
restriction enabled and load the `bwrap-userns-restrict` AppArmor profile (from
`apparmor-profiles`) before starting the Gateway. QuaZonai fails closed when the OS sandbox cannot
create its isolated user/network/pid namespaces; do not work around this by granting the Gateway
Docker socket access or by exposing sealed catalogs to the ordinary research runtime.
