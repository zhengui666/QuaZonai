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

- 决定是否启用单 Operator 登录，并在可信私网中完成首次 Google Authenticator-compatible TOTP 绑定；
- 决定是否在受控设备上启用 `Trust this browser`；
- 完成首次 `RESEARCH_READY`；
- Codex 登录/认证；
- 在 Runtime Configuration 配置 Codex provider/model、reasoning effort、Fast mode 与 Worker limits；
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
- Candidate Package；
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

启用新认证时：保留并备份 `QUAZONAI_MASTER_KEY`、独立 cookie key、machine API token、public origin 与 TTL 配置；从 `.env`/部署 Secrets 中删除旧浏览器用户名和密码变量，并运行 Alembic migration `0010_operator_auth_configuration`。没有 durable binding 的健康实例首次 Web 访问会进入 setup：在 loopback/VPN/SSH tunnel/受保护 proxy 后打开页面，扫描本地二维码或使用 manual key，在 10 分钟内输入当前动态码完成 first claim，再公开实例。第一次成功确认永久绑定该安装；setup candidate 不写 pending DB row，过期后可重新生成。

QuaZonai V1 只有一个部署 Operator。它不是业务用户系统、tenant 或 RBAC。`QUAZONAI_AUTH_ENABLED=false` 保留 direct access；只有显式设为 `true` 才启用下述登录门。

认证启用后的 Web 登录输入只有 Google Authenticator-compatible 6-digit TOTP。登录页不再展示或提交用户名/密码；`Trust this browser` 行为保持不变。

TOTP-only 是单因素登录，抗在线猜测能力弱于密码 + TOTP；若把 Web/API 暴露到公网，仍必须使用 HTTPS、窄化可信代理 CIDR，并优先叠加部署侧网络访问控制。

新安装启用认证时不需要预置 TOTP secret；`QUAZONAI_MASTER_KEY`、`QUAZONAI_AUTH_COOKIE_KEY`、`QUAZONAI_API_TOKEN` 与 `QUAZONAI_AUTH_PUBLIC_ORIGIN` 必须先配置。Web setup 只在数据库不存在 binding、不存在 initialized marker 且数据库可用时出现；marker 存在但 binding 缺失、数据库故障、master key 错误或 binding 解密失败必须保持 fail closed，不会重新打开 setup。既有部署可暂时保留 `QUAZONAI_AUTH_TOTP_SECRET` 作为一次性 legacy importer：启动时原子加密导入缺失 binding，已有 binding 则恒定时间比较，验证后删除该变量；正常登录始终以数据库 binding 为准。所有 TOTP secret、setup candidate、cookie key 和 machine API token 都不能放进聊天、截图、事件或日志。`QUAZONAI_AUTH_COOKIE_KEY` 必须独立生成且不能与 `QUAZONAI_MASTER_KEY` 相同；`QUAZONAI_API_TOKEN` 必须使用 RFC 6750 `b64token` 可安全写入 Authorization header 的 ASCII 字符集。

登录时可以勾选 **Trust this browser**。选中后服务器在当前浏览器 profile 写入长期 HttpOnly trusted-browser credential；短期 session 过期后，只要该 trusted credential 仍有效，就会自动恢复新 session，用户不再重复输入 TOTP。默认 trusted-browser 有效期 30 天，默认短 session 为 12 小时。

只应信任自己控制的浏览器 profile。公共/共享电脑不要勾选。正常 Sign out 会同时清除 session 与 trusted-browser credential、写入当前浏览器 profile 的 `HttpOnly`/`SameSite=Strict` logout barrier 和 sealed local issuance epoch，并使当前 API 进程中已经打开的事件流在下一轮认证检查时停止。下一次成功的 TOTP 登录只清除 barrier，保留 epoch 并把新 cookie 绑定到它，因此 logout 已先应用而旧 login/trusted-browser renewal response 后到时，旧 cookie 仍不能恢复访问。已认证退出还推进当前 API 进程的 global browser-cookie issuance generation；匿名/public logout 仅改变请求者浏览器的 local barrier/epoch，不会阻塞其他浏览器登录或续期。浏览器 credential 还绑定每个 API runtime 新生成的随机 issuance epoch，因此 API 重启会有意使所有已有 browser session/trusted-browser credential 失效，避免重置后的 generation 重新接受退出前 credential。退出请求失败时 UI 不会伪装成已退出。

失窃/不再可信设备的处置：

1. 立即轮换 `.env` 中 `QUAZONAI_AUTH_COOKIE_KEY`；
2. 重启 API；
3. 全部现有 session 与 trusted-browser credential 随即失效；
4. 所有浏览器必须重新执行 TOTP 登录。

其他 credential 轮换：

- `QUAZONAI_AUTH_TOTP_SECRET`：仅用于一次性 legacy importer，不是已绑定安装的轮换开关；已有 binding 时修改它会导致启动冲突。当前没有浏览器内 TOTP rotation；验证器丢失或泄露时，应先保持服务在可信私网内，保留当前 master key 与数据库备份，并执行经单独授权的数据库恢复/更换流程，不得删除 binding 来期待 setup 重新开放；
- `QUAZONAI_MASTER_KEY`：没有可用备份时无法解密 durable TOTP binding；恢复原 key 与数据库备份，不能通过重新 setup 绕过；
- durable Operator TOTP binding：若确需更换 Authenticator，先在受控维护窗口清理并重新绑定对应安装记录，再重新执行私网 first claim；不得通过 cookie/session 过期触发 setup；
- `QUAZONAI_API_TOKEN`：旧 CLI/automation Bearer token 失效；

`QUAZONAI_ENV` 只能为 `development`、`test` 或 `production`（忽略大小写与首尾空白）。认证启用时，`QUAZONAI_AUTH_PUBLIC_ORIGIN` 与浏览器 `Origin` 都按 browser-origin 规则 canonicalize 后精确比较：scheme/host 小写、Unicode host 使用 IDNA ASCII、IPv6 压缩并保留 brackets、默认端口省略、非默认端口保留。production 必须为 HTTPS，反向代理/Tunnel 应在可信 TLS 层终止 HTTPS，并把该外部 Origin 写入 `.env`。

若 TLS reverse proxy/tunnel 把浏览器请求转发到 API，配置 `QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS` 为 **API/容器实际看到的 direct proxy peer** 的精确 IP/CIDR，不要使用 `/0` 或包含普通客户端的宽泛网络。只有 proxy 与 API 直接同机运行或使用 host network 时，`127.0.0.1/32` 才可能正确；Compose 容器通常看到 Docker bridge/gateway 地址，应按实际连接来源配置。仅在该匹配成立时，登录限流才读取一个 `X-Forwarded-For`：它会从右向左移除受信 proxy hop，并使用最近的非受信 literal IP。proxy 必须 append 自己实际观测到的 peer（例如 Nginx 的 `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;`），或以已验证的 client IP overwrite 此 header，不能只转发客户端送入的值。没有此配置、peer 不匹配，或 header 缺失/重复/非法时，QuaZonai 有意忽略 header 并以 direct peer 限流。Compose 已明确运行 Uvicorn `--no-proxy-headers`；手工启动也必须显式传入此 flag，且不能传入 `--proxy-headers` 或设置 `FORWARDED_ALLOW_IPS`，否则 Uvicorn 会在 QuaZonai 校验前改写 direct peer。

QuaZonai 提供的 Web workbench 会返回 `Content-Security-Policy: frame-ancestors 'none'` 和 `X-Frame-Options: DENY`，不能嵌入任何 iframe。反向代理不得移除或放宽这两个响应头；这项控制补充而不取代 cookie `SameSite` 和 Origin 校验。

`QUAZONAI_AUTH_ENABLED=false` 在所有环境保留 direct access，此时 auth credential/TTL 值均 dormant，应保持 loopback-only 或使用另一个明确可信的访问边界。设为 `true` 后，任一 Operator auth 必需值缺失或非法都会使 API fail closed；启用认证的 production 还要求 HTTPS 并自动使用 Secure cookie。连续失败登录会触发最长 30 秒的有界短退避，但不会形成持久账户锁定；被限制的请求仍显示统一的认证失败。

Operator Authentication 启用时，CLI/automation 不使用 Web cookie、浏览器 TOTP 或已废弃的用户名/密码，而是从环境读取符合 RFC 6750 `b64token` 语法的 `QUAZONAI_API_TOKEN`；认证关闭时不要求该 token。Downstream consumer 的 Bearer service token 仍独立，只能操作其自身 Handoff/Feedback 合同。

### 14.3 Codex / Runtime Configuration

Administration 是 Codex runtime 配置的事实入口，显示并允许修改：

- Codex executable/version 与 login 状态；
- 可选 `model`；
- 可选 reasoning effort：`minimal`、`low`、`medium`、`high`、`xhigh`；省略/`null` 时沿用 Codex/模型默认值；
- 可选 Fast mode；启用时只对新 Mission 使用 Codex 原生 `service_tier="fast"`，不写全局 `config.toml`，也不自动降为 Standard；
- 可选自定义 OpenAI-compatible `Base URL`；
- 可选 Codex API key；API key 只写、永不回显；
- App Server preflight；
- Agent worker health。

Linux Docker 部署还会在 `finite-worker` 启动检查中执行一次真实 Codex workspace sandbox preflight。若宿主 Docker seccomp 不允许用户 namespace，worker 会保持未就绪并拒绝领取 Mission；不要通过 `privileged` 或关闭 Codex sandbox 绕过。Compose 为 worker 单独使用专用 seccomp profile，同时保留 no-new-privileges、capability drop、只读根文件系统和 Mission 网络隔离。

自定义 Base URL 必须是绝对 HTTP(S) URL，不能把 username/password、query token 或 fragment 嵌入 URL。配置了 Base URL/API key 时，Mission 使用独立 Codex model provider；未配置时使用 Administration 中连接的 ChatGPT Auth。ChatGPT Auth 的 access/refresh token 加密保存于 PostgreSQL，`CODEX_HOME/auth.json` 仅用于升级时一次性导入，不是运行时事实源。已有 API key 时更改 Base URL，必须重新输入该 endpoint 对应的 key 或显式清除旧 key。

Codex API key 由 `QUAZONAI_MASTER_KEY` 使用 AES-256-GCM 加密后保存到 PostgreSQL。Secret/token 不在 Web 展示，也不写入事件 payload；运行时通过受信任 runner 的 one-shot credential broker 交给 Codex provider auth，不进入 App Server/Mission 环境变量。

管理员可在同一 Runtime configuration 页面选择 **Sign in with ChatGPT**：打开固定的 OpenAI device authorization 页面，输入页面显示的短期 code，等待状态变为 Connected。浏览器只保留当前 dialog 的 code/login id 于内存；不会写入 Web Storage。Device login start 即使 direct access 开启也要求非 safelisted `application/json` 请求体，避免跨站 form/`no-cors` 反复触发持久化事件；收到 OpenAI `slow_down` 时服务端会增加并保存轮询间隔。Connected 账号可显示 email、plan 与刷新时间，但不会显示 account id 或任何 token。若 refresh token 被撤销或保存密文无法解密，状态会变为 **Re-authentication required**；Disconnect 只删除本地凭据，不依赖远端 revoke 成功，并且必须先成功删除本地 legacy `auth.json` 才提交数据库删除；清理失败时数据库凭据保持不变。旧 `auth.json` 导入在 DB commit 后自动删除；删除失败时 official Mission 会 fail closed，不能回退读文件。运行中的 Mission 刷新回调固定原始 auth UUID 与 account，不会切换到后来登录的账号。

`.env` 只负责启动级基础设施与 Operator access：运行环境、PostgreSQL、master key、`QUAZONAI_AUTH_ENABLED`、Operator TOTP、browser cookie key、CLI machine token、public origin、存储根目录和 HTTP port。Codex model/API key/Base URL 不由 `.env` 配置。

Runtime Configuration 使用 revision + 幂等 mutation：页面保存携带当前 revision，若其他请求已先更新则返回冲突并要求刷新，不覆盖较新配置；网络重试复用同一个 `Idempotency-Key`，不会重复修改 revision、重复写事件或重复保存 secret。

reasoning effort 与 Fast mode 相互独立；未来 Mission/Profile 显式值优先于 Runtime Configuration 默认值，Runtime Configuration 又优先于 Codex/模型默认值。保存后的新配置只影响之后启动的 Mission Thread，已运行 Mission 不切换条件。

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

### 14.10 Remote Nautilus quant runtime

QuaZonai Core 与 NautilusTrader 分开部署。管理员在 Core 的受信部署边界配置独立的 Research 与 Sealed runtime endpoint/token；Codex Mission child 不会继承这些值。Core Compose 的 API 不加入 runtime bridge，而是通过只转发到两个 runtime endpoint 的 `nautilus-runtime-proxy` 访问它们；不要把 API 直接接入 runtime network。

同一 Docker 主机运行 `deploy/nautilus-runtime.compose.example.yml` 时，先让 Core Compose 创建稳定的 `quazonai-core` network，再启动 runtime 示例；示例中的 proxy 是唯一同时加入 Core network 与 `quazonai-runtime` 的服务。跨主机部署不要共享 Docker network，改用受信 HTTPS endpoint/token。

```dotenv
QUAZONAI_NAUTILUS_RUNTIME_URL=https://research-runtime.example
QUAZONAI_NAUTILUS_RUNTIME_TOKEN=<research-runtime-service-token>
QUAZONAI_NAUTILUS_SEALED_RUNTIME_URL=https://sealed-runtime.example
QUAZONAI_NAUTILUS_SEALED_RUNTIME_TOKEN=<sealed-runtime-service-token>
QUAZONAI_NAUTILUS_VERSION=1.231.0
QUAZONAI_NAUTILUS_CONTRACT_VERSION=2
```

运行顺序：

```text
远程 Nautilus Catalog ingest/validate
→ QZ 创建 immutable Dataset Revision + Catalog binding
→ Idea / Mission 生成 EXPERIMENTS.json
→ finite-worker 调用 Remote Research Runtime
→ 每个 run 的结构化 evidence 进入 QuantRuntimeRun / Search Ledger
→ 独立 Sealed Runtime 执行 promotion evaluation
→ PASS 后在 discovery catalog 上执行 Portfolio simulation
→ 产生 Alpha / Candidate / Paper Approval
→ 批准后生成 Nautilus-native Candidate Bundle
→ 独立 Paper/Live runtime claim 并回传 Forward Evidence
```

Remote runtime 不可用时，run 保留失败 evidence 并由 durable job policy 处理重试；不得退回 QZ 自研模拟器。Sealed endpoint/token/catalog 必须独立，Sealed raw report 不展示给 Agent。QZ 不启动、停止、撤单、平仓或恢复任何 downstream runtime。

### 14.11 PMXT Archive 历史数据

PMXT Archive 以公开 HTTPS 小时 Parquet 提供 Polymarket v2 与 Kalshi 的历史 orderbook。管理员在已批准的 Data Source / Universe 边界内，可以为单个小时文件和 `asset_id`/`market_ticker` 建立 Catalog ingest，也可以为全市场历史建立 `ArchiveManifest`；Core 只保存 source specification、Manifest/ Dataset Revision 和受控引用，具体探测、按需下载与 Parquet 转换在独立 Remote Nautilus runtime 中完成。

示例 `source_spec`（插件 release 与 runtime bundle 由管理员先激活/预热）：

```json
{
  "kind": "plugin",
  "config": {
    "venue": "kalshi",
    "archive_url": "https://r2kalshi.pmxt.dev/kalshi_orderbook_2026-06-11T03.parquet",
    "instrument": "<market_ticker>"
  }
}
```

请求同时绑定 `plugin_release_id` 与 `plugin_runtime_bundle_id`；当前 PMXT primary wheel
的 plugin id 为 `pmxt_archive`。该绑定只允许 `ACTIVE` release 和 `READY` 的 `IMPORTER`
bundle，具体下载与 Parquet 转换由独立 runtime 的通用 connector-runner child 完成。

Reference Nautilus runtime 不允许第三方 plugin 直接执行 `sealed=true` Catalog ingest；sealed raw data 必须由受信 provisioning/import path 预置到独立 sealed Catalog，之后只读提供给 evaluator。

PMXT Archive 读取不需要 API key；本地部署也不应填写 PMXT 交易凭据。单个 Polymarket v2 小时文件可能较大，因此先按 instrument 过滤，避免把全市场文件载入 Research Catalog。PMXT 接入完成只代表历史研究数据可用，不代表已连接交易系统。

全市场全历史使用下面的清单配置，不下载整库：

```json
{
  "kind": "plugin",
  "config": {
    "venue": "polymarket_v2",
    "selection": "all_markets",
    "archive_start": "2026-04-13T19:00:00Z",
    "archive_end": "2026-08-31T03:00:00Z"
  }
}
```

`archive_start`/`archive_end` 是 UTC 小时边界，`archive_end` 不包含在范围内。Manifest 检查只发送固定规则的 HTTPS HEAD 探测；缺失小时和探测错误会被分别记录，不会伪装成连续历史。研究时调用 `POST /api/v1/quant-runtime/archive-manifests/{manifest_id}/materialize`，提交一个 instrument 和 `[start, end)` 小时范围；单次最多 168 小时、估算源文件最多 20 GiB，且每个 AVAILABLE 分片必须有已知大小，插件只下载并过滤选中的分片，使用分批 Parquet 解码（最多 16,384 行/批、64 MiB/批、累计解码输入 4 GiB），在独立 child 的 6 GiB address-space 与 runtime 容器的 10 GiB memory 上限内生成新的 immutable Dataset Revision。runtime 对生成的 Catalog 只以最多 16,384 行和 64 MiB 的 Arrow 批次扫描时间列完成边界校验，不把整库 materialize 到内存；子进程地址空间与 2 GiB 暂存输出配额合计低于容器上限并留有 runtime headroom。Core 会校验请求范围逐小时无重叠/缺口；缺失小时与探测错误会进入 quality evidence，不会伪装成连续数据，也不会直接判定 Alpha 失败。不能把整库下载到本机。PMXT v2 当前可用历史的起点由归档源决定，不等于 Polymarket 的全部历史。

每次导入使用与 Catalog 分离、每个 runtime 实例 3 GiB 配额的 tmpfs 暂存区；发布前最多允许 2 GiB、10,000 个常规 Parquet 文件，插件 child 继承 6 GiB address-space 上限，插件 stdout/stderr 也会流式有界读取。超出任一边界会终止该次导入并保留原 Catalog 不变。

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

## Mobile Web / PWA

手机浏览器和已安装 PWA 使用与桌面相同的 Web 客户端、路由、字段、校验和 mutation，不需要单独注册或迁移移动端业务。小屏底部导航提供 Home、Research、Approvals、Portfolio；Idea、Alpha、Handoff、Administration、语言、主题、安装/更新和退出在 More 中可达。列表、审批、表单、图表和图谱都使用触控可操作的响应式视图。

在浏览器支持安装提示时，More → Install QuaZonai 由操作者确认安装。部署新前端后，已打开的 PWA 会在启动注册、前台周期检查（最长约 15 分钟）、后台恢复前台或联网恢复时发现新的 waiting Service Worker；恢复事件受 60 秒最短间隔保护。发现更新会弹出确认 Dialog：选择“稍后”不会重载，More → Update 仍可手动更新；选择“立即更新”才会激活 waiting worker 并重载当前页面。更新检查失败不打断工作台，会由后续触发点重试。排障时检查 `/sw.js` 是否返回新内容且 `Cache-Control: no-cache`，并确保 CDN / reverse proxy 没有把 `sw.js` 配成 immutable 长缓存。离线只保证静态壳能够启动；工作台会明确提示需要连接 QuaZonai server，API 查询和所有 mutation 不从本地缓存伪造成功，也不把认证/API 数据写入 PWA 缓存。
