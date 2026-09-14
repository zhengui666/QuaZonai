# CLI 命令

`client automation authorize PROJECT_UUID`提交AutomationAuthorizeV1到
POST /api/v2/projects/{id}/automation-policies：schema_version=1、expected_project_revision、
content（完整AutomationPolicyContentV1，字段见DESIGN A7.0）。POLICY_AUTHORIZE人工grant
绑定原项目及完整请求；201冻结新版本并更新项目当前政策。旧版本不改写，409先重读
项目revision。两组指标均使用正式MetricRequirementV1，不能用空列表默认通过。
`client automation list PROJECT_UUID`和`show POLICY_UUID`读取原政策，需精确项目
RESEARCH_READ。原登记和历史读取不证明当前有效性，也不代表已有自动审批或交付。
`client automation revoke POLICY_UUID`提交PolicyRevokeV1到
POST /api/v2/automation-policies/{id}/revoke：schema_version、expected_latest_revocation_id
（首次null）、effective_at（null立即，或未来时刻）、reason；需精确POLICY_REVOKE人工grant。
201只追加撤销，后续记录不能推迟最早生效时间，不停止已领取执行。归档项目仍可撤销。
`client automation revocations POLICY_UUID --limit 50`分页原撤销；未知结果保留原键/正文。
政策管理及自动 Paper/Live 原生消费已接通，对应界面尚未接通。

`client handoff ack HANDOFF_UUID`提交HandoffAckV1到POST /api/v2/handoffs/{id}/ack：
schema_version=1、external_ack_id（同Claim编号规则）、external_claim_id、
outcome=ACKNOWLEDGED|REJECTED、reason_code及reason。幂等键须等于external_ack_id，
使用精确项目/下游的DOWNSTREAM_ACK凭据，不使用人工grant。已领取时须引用原claim编号；
领取前仅能在Offer期限内以null claim编号拒绝。200记录反馈，不是新交付授权。
同编号/请求重试返回原回执；新编号或改正文不能覆盖终态。已领取后的晚到ACK不因
审批撤销而抹除；当前凭据仍须有效。ACK不代表QZ拥有下游订单或停止权限。

`client approval revoke APPROVAL_UUID`提交ApprovalRevokeV1到POST /api/v2/approvals/{id}/revoke：
schema_version=1、expected_latest_revocation_id（首次null）、effective_at（null立即，
否则为未来时刻）、reason_code和reason。需APPROVAL_REVOKE人工grant绑定原审批及完整请求。
201追加不可变撤销，不能推迟更早的生效记录；立即生效时只把原审批下尚未领取的Offer
标为REVOKED。Worker补记定时撤销与到期，Claim自身也检查最早生效时间。已领取事实保留。
`client approval revocations APPROVAL_UUID --limit 50`按id倒序分页读取历史，需原项目
RESEARCH_READ；归档项目仍可由Operator撤销。409先重读最新历史，不自动覆盖CAS。

`client handoff claim HANDOFF_UUID`向 POST /api/v2/handoffs/{id}/claim 提交
HandoffClaimV1：schema_version=1、external_claim_id（1..200 UTF-8字节，无首尾空白/控制字符）、
package_schema_version（当前字符串1）。Idempotency-Key必须等于external_claim_id；
使用原项目/下游的DOWNSTREAM_CLAIM机器凭据，不使用人工grant。200返回原领取记录
及TargetPackage；replayed=true只重放原转移，不刷新期限或再次领取。换编号重领、
换Offer复用编号均冲突；已撤销/过期或当前审批/来源/下游不可用时拒绝新领取。
当前凭据无效时也不能读取旧回执。Worker按数据库时间清理未领取的到期Offer，
Claim独立检查时间；已领取记录不会因此变成停止或撤单。

`client handoff offer` 提交 HandoffOfferV1 到 POST /api/v2/handoffs：schema_version=1、
release_id、approval_id、supersedes_handoff_id（首次null，否则精确最新Offer）、expires_at。
需绑定approval_id和完整请求的HANDOFF_OFFER人工grant。服务端从审批取下游/环境，
重验原来源、拒绝历史、撤销、期限及最新readiness；不允许覆盖或重复发送原Release。
新Offer会同事务撤销仍未领取的前版；已领取前版保留事实，不代表停止或撤单。
`client handoff show UUID` 读 GET /api/v2/handoffs/{id}当前状态。原创建回执重放仍是
原结果，不能据其OFFERED判断当前状态。下游仅凭对应项目/下游的CLAIM或ACK scope
读取自身Offer。人工Offer、Claim、ACK及显式审批撤销已接通；界面仍待实现。

`client release approve RELEASE_UUID` 向 POST /api/v2/releases/{id}/approvals
提交 ReleaseApproveV1：schema_version=1、downstream_id、environment=PAPER|LIVE、
expected_downstream_revision（十进制字符串）、expected_latest_decision_id（首次null，
重新考虑后为最新REOPEN）、valid_until。需要绑定原Release和完整请求的
RELEASE_APPROVE人工grant与幂等键。服务端重验原REAL Package、当前来源许可/资格、
下游配置及新鲜探测，并在同一事务冻结原评估报告引用；不接收evidence_set_id。
201返回不可变Approval，不能当作已领取或执行。`client approval show APPROVAL_UUID`
读取 GET /api/v2/approvals/{id} 原元数据，需精确项目RESEARCH_READ。历史记录不是
当前有效性证明；人工Offer/Claim已接通；自动 Paper/Live 原生消费已接通，审批界面尚未接通。未知结果保持原请求/键。

`client release reject RELEASE_UUID`读取ReleaseRejectV1；`client release reconsider DECISION_UUID`
读取ReleaseReopenV1，字段/最新决定CAS见DESIGN A7.1。需要对应精确目标的
RELEASE_REJECT/RELEASE_REOPEN人工grant。`client release decisions RELEASE_UUID`
按cursor/limit读取同Candidate跨Release历史。201是追加决定，不是审批、恢复旧授权或撤单。
未知响应保留原body/key；409后先重读最新决定，不自动改expected_latest_decision_id。

`client release create`从stdin读取`{"schema_version":1,"candidate_id":"UUID","evaluation_id":"UUID"}`，
POST /api/v2/releases，需原Candidate的RELEASE_CREATE人工grant及Idempotency-Key。
201仅表示不可变Package/Release已冻结，不是审批或交付。`client release show UUID`
读取GET /api/v2/releases/{id}的原版本，CLI需精确项目RESEARCH_READ。
未知提交保持原请求/键重试；不能覆盖权重、有效期、来源或上传包绕过PORTFOLIO/PASS。
完整原生成功链路和下游交付仍待验收。

原生单币种模拟中 CurrencyPair 仅支持 MARGIN，Equity 支持 CASH/MARGIN；
执行假设和实际运行共用锁定 Nautilus 0.63.0 的该限制，不自动转换旧配置。

完整产品合同在 DESIGN。原生 `server client` 复用已实现 HTTP 控制面的同一 Rust 请求/响应合同；原生任务、认证、数据许可、研究准备与运行命令见下文。完整研究/组合/交付闭环仍须逐项验收，不提供绕过 API 的手工 SQL 业务路径。

## 面向用户的原生 HTTP CLI

发行二进制 `server` 与开发入口 `cargo run --locked -p server --` 使用相同命令。`client` 不读取数据库连接或应用 SecretVault。先通过已初始化系统的正式机器身份管理 HTTP 接口创建 CLI 主体并发行适当的受限凭据，将首次显示的凭据保存为本机仅本人可读的文件；不要把令牌、TOTP 或新的服务凭据放在命令参数、Issue、日志或 shell history 中。

```sh
cargo run --locked -p server -- client --help
server client --origin https://research.example --credential-file /private/cli.token data source list --limit 50
server client --origin https://research.example --credential-file /private/cli.token runtime list
```

上例 origin 和路径须替换为本人部署及凭据文件。Unix 凭据文件权限不得授予 group/other；末尾允许一个换行。私有CA部署使用 `--ca-certificate /absolute/ca-bundle.pem`，不存在忽略证书的选项。开发环境只有字面量127.0.0.1或::1且显式 `--development-http` 才可HTTP；服务器也须同意该入口。连接默认3秒、普通请求20秒，失败不自动重试、不使用环境代理、不跟随重定向。

### 当前命令与严格正文

所有 `create/update/register/freeze/start/propose/submit/cancel/revoke` 正文从stdin读取严格JSON。下表尖括号表示必须传入已取得的真实资源ID，不是由CLI猜测的名称。

| `server client …` 后的子命令 | 正文 / 结果 |
|---|---|
| `project list/show <id>/create/update <id>` | ProjectCreate/ProjectUpdate；ProjectView/分页/完整回执 |
| `brief list <project_id>/show <id>/create <project_id>/update <id>/freeze <id>` | BriefCreate/BriefUpdate/BriefFreezeV1；冻结不是默认通过科学门禁 |
| `cycle list <project_id>/show <id>/start <project_id>` | CycleStartV1必填researcher_profile/reviewer_profile，各含profile_id/expected_revision；202含真实Cycle/Run及冻结选择，查询不启动第二次研究 |
| `cycle selection <id>/trials <id>` | 原Mission收尾的冻结选择及分页原试验，区分原Validation版本与可空review_alpha_version_id；404表示未形成，COMPLETE不是资格或科学PASS |
| `data source list/show <id>/create/update <id>` | DataSourceCreate/DataSourceUpdate；已登记原生身份不可更改 |
| `data grant list <source_id>/create <source_id>/revoke <id>/revocations <id>` | DataGrantCreate/DataGrantRevoke；正文source_id须与父路径相同 |
| `data revision list/show <id>/register` | 列表可加 `--source-id/--partition`；DatasetRegister不接收自报origin/PIT或URL |
| `data universe list/show <id>` | Universe元数据及 `NATIVE_METADATA/LEGACY_UNVERIFIED` 登记证据状态；历史记录不被冒充为原生登记 |
| `data validate` | DataValidateRequest；202仅返回唯一排队Run，人工grant目标为已有InputSet，不是新的RunID |
| `runtime list/show <id>/create/update <id>/probe <id>/readiness <id>` | RuntimeCreate/RuntimeUpdate/RuntimeProbeRequestV1；配置与真实探测分离 |
| `codex list/show <id>/create/update <id>/homes` | CodexProfileCreateV1/CodexProfileUpdateV1；只选择部署登记的非秘密目录标签，不输入宿主路径 |
| `codex probe <id>` | CodexProbeRequestV1；正文profile_id必须等于命令ID，原生探测不执行付费推理 |
| `codex models <id>/account <id>` | CodexObservationV1；只读当前版本观测，STALE/UNPROBED不触发后台模型调用或登录刷新 |
| `codex login/logout` | stdin CodexAccountRequestV1；SYSTEM Profile 的原生账号命令，202为接受，不是认证成功 |
| `codex login-cancel` | stdin CodexLoginCancelV1；绑定operation_id及操作revision；取消意图不冒充原生取消结果 |
| `codex login-status <id>` | CodexAccountOperationV1；只读指定操作，不输出设备码、不重启登录 |
| `downstream list/show <id>/create/update <id>/probe <id>/readiness <id>` | DownstreamCreate/DownstreamUpdate；配置不是下游订单执行授权 |
| `input-set list --project-id <id>/show <id>/create` | InputSetCreate；同一不可变数据、许可与用途校验 |
| `policy list --project-id <id>/show <id>/create` | EvaluationPolicyCreate；登记不证明方法或数据已经可用 |
| `experiment list --project-id <id>/show <id>/propose` | ExperimentProposalV1；PENDING不等于运行或合格 |
| `alpha list --project-id <id>/versions <id>/show <id> <version>/evaluations <version-id>` | 原Alpha/不可变版本及已发表Validation；登记状态不替代资格 |
| `evidence show <id>/metrics <id>` | 三层评估状态、来源/期限及分页MetricValueV1；不下载受限报告、不披露Sealed |
| `artifact list --project-id <id>/show <id>/submit/export <id>` | ArtifactCreate；export先核对元数据、media和字节数，再向stdout写原始字节 |
| `run list/show <id>/cancel <id>/watch <id>` | RunCancelV1；list可选 `--project-id/--state`，watch只观察 |
| `operator-grant` | OperatorGrantRequest含完整command、target_id与新TOTP；201为单次人工授权 |
| `credential-register` | IntegrationSecretCreate；仅返回用途/原生引用，不显示或存储请求明文 |

列表统一支持 `--limit 1..100` 和 `--cursor UUIDv7`；服务端返回的bigint/Revision保持十进制字符串。每次只读取一页，不暗中跨项目遍历。输入文件采用仓库原生导出的OpenAPI中同名DTO，不依据上表摘要猜字段。未知字段、本地错误ID/枚举/正文与未提供幂等键会在发送前拒绝；实际授权、最新revision与不变性仍由服务器裁决。

### 单次人工授权、原请求重放

Codex账号操作与保存配置、探测分开。`login/logout`正文为
`{"schema_version":1,"profile_id":"<profile UUID>","expected_revision":"<profile revision>"}`；
`login-cancel`正文为`{"schema_version":1,"operation_id":"<operation UUID>","expected_revision":"<operation revision>"}`。
按下述流程为完整意图申请单次人工grant，网络结果未知时保留原正文、grant与Idempotency-Key。
同键`login`重放只读取原接受回执，并可取回原进程仍持有的设备码，不发起第二次登录。
CLI的登录响应可能包含一次性设备码，仅在私人终端使用，不转发到LLM、日志、CI或issue。
`login-status`不返回设备码；原生令牌只由Codex保存，CLI不读取auth.json。
`UNKNOWN`、本地等待截止或进程退出都不代表注销/取消成功，须核对实际账号状态并重新探测。

普通机器scope不授予持久Operator身份。Source、许可、政策、配置等管理写入需近期人工grant。先准备完整 `OperatorGrantRequest`（含当前TOTP），以stdin申请；随后使用返回 `resource.id` 作为 `--operator-grant`，请求应与grant所绑定的DTO及target完全相同。创建类target使用返回 `resource.target_id`，不能自造另一个UUID。

```sh
server client --origin https://research.example --credential-file /private/cli.token \
  --idempotency-key "my-source-grant-1" operator-grant < /private/operator-grant-request.json
server client --origin https://research.example --credential-file /private/cli.token \
  --idempotency-key "my-source-create-1" --operator-grant "$GRANT_ID" \
  data source create < /private/source-create.json
```

`$GRANT_ID` 是上一条成功响应的真实引用。CLI不自动读取TOTP种子或申请新的grant。结果未知时保存同一key、原始输入和grant，先查当前记录，必要时显式重放；同key不同正文返回409，禁止自动换key规避。过期/撤销的授权须按服务端错误处理，不宣称原操作已回滚。成功JSON为stdout，失败时退出1并向stderr输出一个已验证Problem或封闭本地错误；`CLI_SERVER_UNAVAILABLE_OR_RESULT_UNKNOWN` 尤其不表示服务端操作未提交。

### 有界运行观察与产物导出

```sh
server client --origin https://research.example --credential-file /private/cli.token \
  run watch "$RUN_ID" --after "$LAST_EVENT_ID" --max-seconds 300 --max-events 1000
server client --origin https://research.example --credential-file /private/cli.token \
  artifact export "$ARTIFACT_ID" > /private/exported-artifact
```

watch以NDJSON输出 `schema_version/event_id/event`，最后输出 `watch_ended/last_event_id/events_received/cancellation_requested=false`。`$LAST_EVENT_ID` 格式为同一Run的 `UUIDv7:十进制seq`；首次观察可省略 `--after`。流量16MiB、最多3600秒/10000事件；Ctrl-C、断线或达到上限均不取消服务器任务。兼容未知事件只保留公开envelope；reset-required或不兼容合同返回错误，不假装连续。需继续观察时使用最后已验证cursor显式调用，不自动重连。导出失败时调用方不得把空或未完成的重定向文件视为成功产物；必须检查退出码。

当前这些CLI命令与已有HTTP实现同步；Alpha资格、完整组合Release/自动化等B2后续命令仍属于Issue62必交范围，不能因入口列表增加而宣称全量生产验收完成。

## 可信 Worker 与正式数据验证

`server worker` 与 `server serve` 是独立进程；二者连接同一正式数据库、使用同一已初始化 `STATE_DIR` 和明确的 `RUNTIME_TARGETS`。Worker 只驱动固定原生任务，不在控制面执行科学代码；不得使用数据库所有者账号启动。`--parallelism` / `WORKER_PARALLELISM` 默认2，范围1–32；每条消息有独立领取身份。

```sh
server worker --help
server worker --state-dir /private/quazonai --parallelism 2
server client --origin https://research.example --credential-file /private/cli.token \
  --idempotency-key "validate-input-1" --operator-grant "$GRANT_ID" \
  data validate < /private/data-validation.json
```

数据库连接及 Runtime allowlist 由部署环境提供，不在命令中暴露凭据。`data-validation.json` 采用原生 `DataValidateRequest`：`schema_version=1`、`project_id`、`input_set_id`、`runtime_id`、`expected_runtime_revision` 和 `limits`。limits 包含 `schema_version=1`、`experiments=0`、正数十进制字符串 `cpu_seconds`、`wall_seconds`（1–86400）、`memory_mib`（1–1048576）、正数十进制字符串 `output_bytes`（最多67108864），同时不能超过真实 Runtime 探测的能力。CPU核数由累计CPU秒/墙钟上限向上取整，不允许客户端指定镜像、命令或额外文件。

该操作的人工 `OperatorCommand` 为 `DATA_VALIDATE`，grant 的 `target_id` 必须为既有冻结 InputSet。只接受同项目正式登记的 DISCOVERY/VALIDATION 数据，拒绝 SEALED、任意原始报告和artifact-only输入；最多两个并行无Cycle数据验证任务。202返回唯一QUEUED Run，不代表已完成；同一key必须保存相同正文及grant重放。用 `run show/watch` 读取真实状态，用 `run cancel` 申请取消。

Worker在首次提交之前刷新必要的原生探测；提交结果未知时只查询同一远端任务，不能重发新任务。退出Worker只停止新驱动，不等于远端任务已停止，也不会提前archive未知结果。固定任务成功会把原始结果清单和生产者绑定产物原子登记后再确认队列；质量报告不是PIT或Alpha资格。当前该入口及故障回归不替代尚需完成的完整Mission/研究/评估/组合/交付验收。
bar-notional/1镜像的原生质量报告新增last_bar_notionals：逐资产最后已知BAR的
价格、数量及Nautilus原生收盘估值名义金额，保留原币种与事件/可用时间。null表示
未测量；SEALED不输出这些明细。它不是真实逐笔成交额或未来流动性保证，也不会
自动授予DATA_BACKED、参与率准入或组合资格；原生来源消费链仍须独立核验。

启用原生Mission还须同时提供`--codex-deployment` / `CODEX_DEPLOYMENT`、
`--mission-api-origin` / `MISSION_API_ORIGIN`、`--mission-workspaces` /
`MISSION_WORKSPACES`。前者复用API的部署绑定文件；API origin须可由可信MCP进程
访问，本地HTTP仍须明确`--development-http`。工作区根必须已存在、绝对路径且权限
0700，不得指向源仓库、个人HOME或Codex认证目录。缺配置不消费Mission消息。
科学任务与Mission各自最多parallelism个在途驱动；不新增Agent工具循环。
当前自动入口已接首轮准备和原生轮账本，单独结算一轮不等于研究流程收束。
实验在编译首阶段预约一次trial，编译失败/取消仍留账，原预测不再重复计数；
这不改变每个阶段的CPU/输出/墙钟预算，也不改变既有历史账目。
已采纳的原编译/Discovery失败通过同一Thread的预算Turn回送。预测成功后自动登记
RESEARCH Alpha并进入正式Validation，不为中间抽样另开Turn；评估发表后回送精确
元数据和冻结选择指标，不读取受限报告字节。每个结果最多一次，保留origin及有效期，
不构成资格或排名。修复新提案必须引用原parent_experiment_id，不覆盖已执行输入。
成功结算的原生Turn另保留最多64KiB公开回答REPORT（qz.mission_summary），原生
summary视图来源、原Turn/item/phase如实记录；失败重投只补摘要，不重新调用模型。
它不是新Agent工具，也不从回答文字推断研究通过或Mission已完成。
原请求和总结按不可变Mission角色保存：研究者RESEARCH、Reviewer EVALUATOR_ONLY；
Reviewer只能发送本Run/Attempt的可信Turn请求，不获得任意封存产物读取权限。
两种Mission角色共用Runtime能力刷新检查；配置版本变化不能因Reviewer角色被跳过。
研究者成功收束且冻结选择COMPLETE、有原审阅目标时，ACK事务使用冻结Reviewer
Profile准入独立Mission；账号暂不可用保留原消息。Reviewer使用不同Thread和工作区，
仅接原CODE、原PARAMETERS及有界Validation上下文，不接研究对话或Sealed原始数据。
每目标一条原生Turn，公开JSON回答绑定原版本/预约/总结；无效回答记INCONCLUSIVE，
不另开付费修复轮。Reviewer无ARTIFACT_SUBMIT/EXPERIMENT_SUBMIT；其PASS不是
Operator审批、Sealed通过或资格。全部目标审阅完成后，可信Worker每次消费为一个
原PASS目标准入Sealed任务，不调用额外模型、不借用人工alpha evaluate授权。
成功收束/ACK等待原审阅到Sealed Run的关联齐全；取消不补做，预算不足或输入
需处理如实记录Cycle状态。真正Sealed计算/发表由科学Worker完成；原独立审阅关联
的封存ACK按DESIGN A4.14裁决精确版本资格，单独科学PASS或人工202均不授资格。
资格没有手填/强制发证命令；当前真实数据正向授予与完整资格操作面尚未验收。
当前没有新增人工命令或审阅结果公开接口。
全部Turn/科学任务结算、反馈回答及提案处理齐全后，可信Worker才提交Mission执行
终态并归档PGMQ；ACK失败重放原事务，不重开模型。SUCCEEDED不是Cycle完成、实验
裁决或资格；只有公开限制说明且无实验的会话也不构成“无有效Alpha”的科学证据。
取消/到期收束不会新开验证或反馈Turn；已有科学任务须有真实终态，缺最终模型
用量仍保持未知。只有原账本证明无发送意图才记NOT_SENT，不能手填零费用清账；
取消无公开回答时保存null，不创建假总结。原Validation发布队列继续独立恢复。
取消恢复仅重连已登记Thread，不签发Mission凭据或启用MCP，不准备新Turn；
清理窗口至多110秒且保留原资源上限。未知最终用量仍保留原队列和预约。
成功Discovery预测还会由可信Worker登记一次RESEARCH Alpha首版本，固定原CODE/
MODEL、预测镜像、根血缘和Brief的单位/horizon；没有校准则保留null。该元数据步骤
不调用模型、不请求Runtime、不把PENDING改为SUPPORTED，也不是新增Agent审批工具。

## 原生科学任务入口

`job` 是受信任运行时启动的一次性计算进程，不是浏览器/Agent 的任意命令执行代理。每次调用只运行一个任务，API/Worker 不在本进程内嵌入 Nautilus。Clap 原生帮助：

```sh
cargo run --locked -p job -- --help
cargo run --locked -p job -- allocate < tests/contracts/allocation-input.json
```

第二条是明确标记的合成两资产数值回归输入，不产生生产资格或交付权。`allocate` 使用真实 Clarabel 求解并检查存储用十进制目标；无解/失败不输出备用权重，必须检查 `solver_status` 而非只看进程退出码。

`allocate`要求原`forecasts`集合，不再接受资产上的
`expected_return`。原Alpha版本、单位/期限/时点、原生bar、完整资产顺序及固定
混合权重先检查，再用ndarray聚合进入同一Clarabel问题；MIN_RISK也不跳过。
这些输入标识不代替数据库资格或许可。必须重建并登记带`portfolio-ensemble/1`
能力的新镜像，不能沿用旧参数或旧镜像。原生命令不授予Agent组合审批能力。

同一输入还必须显式提供`optimizer`和`alpha_ensemble`，形状为NativeModelRefV1：
schema_version、adapter_kind、upstream_class、upstream_version、parameters。
当前仅支持CLARABEL_QP / clarabel::solver::DefaultSolver / 0.11.1（参数沿用
AllocatorSettingsV1）及FIXED_WEIGHTED_FORECAST / ndarray::ArrayBase::dot / 0.17.1
（参数为空对象，混合权重在原forecasts中）。顶层settings已删除；未知类/版本、
错误角色、额外参数均拒绝，不默认选择模型。新镜像还需portfolio-models/4能力。
正Decimal的risk_aversion现在冻结在optimizer.parameters中，顶层同名字段已删除。
VARIANCE 的 constraints.max_ex_ante_risk 可为正的每决策周期收益方差上限，或 null；
不是标准差/年化波动率。CLARABEL_QP 原生适配可带二阶锥约束，镜像还须
portfolio-variance-bound/1 与 SECOND_ORDER_CONE；发布复核允许上限乘 exposure_tolerance
的相对误差，不允许同数值的绝对方差误差。
CVAR 支持相同 MIN_RISK/MAX_UTILITY，必须明确 optimizer.parameters.cvar_confidence
为大于0、小于1的Decimal（VARIANCE必须为空）；不默认95%。它用原 return_history
的等权损失场景进入原生LP，不使用协方差。此时 max_ex_ante_risk 为同周期的预期
损失收益率上限，含分数尾部质量；不是方差、VaR或年化值。要求 portfolio-cvar/1
与 LINEAR_PROGRAM 镜像能力。
RISK_BUDGETING 必须填写 optimizer.parameters.risk_budgeting：schema_version=1、
正 risky_gross_exposure、覆盖原资产的 assets（instrument_id/share/sign=LONG或SHORT）。
非负share精确合计1；零份额资产固定零，不默认预算、方向或总敞口。其他目标此字段
为空。VARIANCE原生二次锥先求指定方向预算，再按总敞口规范化并进入
原组合约束问题；冲突不可行，不裁剪。两阶段共用max_iterations，发布复核真实贡献
比例。原生两阶段均使用冻结solver_tolerance，不改变低精度授权或发布要求。
需portfolio-risk-budget/1与SECOND_ORDER_CONE能力；
有方差上限还需其原能力。
CVAR风险预算需portfolio-cvar-risk-budget/1、POWER_CONE及原CVAR/LINEAR_PROGRAM
能力。原生幂锥加权几何平均约束配合完整场景CVaR线性目标；第一阶段gap容差取
min(solver_tolerance, exposure_tolerance²)，不改变最终贡献容差和低精度授权。
成功结果cvar_risk_budget_witness保存原场景顺序的对偶权重；发布独立核对概率和、
尾部质量上界、经验CVaR最优值和各资产风险贡献，不能挑选并列场景或归一化对偶。
只接受有限正总风险的预算；原生无界、零风险、失败和不兼容约束均无目标/无证据，
不改变普通CVAR目标允许负风险的规则。

协方差数值适配的引用为SAMPLE_COVARIANCE / ndarray_stats::CorrelationExt::cov /
0.7.0，parameters仅为`{"ddof":1}`，不能传年化、补值或另一估计器参数。
本机allocate必须带covariance_estimator和return_history，
不再接收covariance矩阵。历史收益须保留与forecasts相同资产/bar/期限/币种、
严格递增窗口结束和不晚于决策的可用时点；VARIANCE由原生样本估计进入Clarabel，
CVAR直接使用完整场景，不能预先挑选极端收益或把未知历史补零。
格式见合成输入文件和DESIGN A5.2；历史来源的可信目录/产物/许可绑定仍待完整编排。

受管PORTFOLIO_BUILD不接受上述手填数值输入；必须使用dataset_revision_id与
NativePortfolioBuildRequestV1，包含selection、mandate、current_weights_artifact_id、current_weights、
assets与原Alpha/model/calibration成员。仅挂载明确FORWARD目录和MODEL产物，
原生运行生成预测与历史收益，输出qz.native_portfolio/1。模型数值执行不代替
Store的当前资格、许可、政策与资金来源检查，完整Candidate编排仍待完成。

current_weights是PortfolioCurrentWeightsV1的原冻结副本，独立REPORT输入必须提供
同一current_weights_artifact_id的原JSON。source严格区分FORWARD_SNAPSHOT
（downstream_id/external_message_id）和LAST_TARGET（candidate_id），后者是目标假设。
asof_ns/available_ns/valid_until_ns、base_currency、cash_weight、同序weights必须
匹配Mandate、决策时点与assets.current_weight；缺失、过期、未来或同长度替换均失败。
新镜像声明portfolio-weights/1。不接受NONE或隐式全现金；本机allocate仍只是数值入口。

### 下游原始当前权重

`POST /api/v2/forward/weights`接受`DownstreamWeightsSubmitV1`：schema_version、
project_id、environment（PAPER/LIVE）、external_message_id、asof_ns、available_ns、
valid_until_ns、base_currency、cash_weight、weights。时间使用纳秒整数字符串，
权重使用精确十进制字符串，现金加资产权重必须合计1；不传账户、NAV或持仓数量。
仅限精确项目/下游的DOWNSTREAM身份和FORWARD_SUBMIT范围，环境须被集成允许。
原生CLI复用统一写命令参数，但服务端按external_message_id重放，不按传输键另建消息：

```sh
cargo run --locked -p server -- client --origin https://qz.example --credential-file downstream-token --idempotency-key original-message forward-weights < weights.json
```

返回201/CommandResult_DownstreamWeightsViewV1及不可变报告身份。同一项目、下游、
环境内相同原消息重放原回执；改内容返回409。Operator grant不能代替下游身份。
PAPER标为SYNTHETIC；LIVE只证明认证下游提交，不等同独立研究资格或交付许可。
此入口登记FORWARD_SNAPSHOT，不接收自称LAST_TARGET的报告。

### 不可变 Portfolio Mandate

组合构建入口为`POST /api/v2/portfolio-builds` / `client portfolio build`，请求
`PortfolioBuildRequestV1`仅引用cycle_id、mandate_id、input_set_id、runtime_id、
expected_runtime_revision、current_weights_source、environment、
members[{qualification_id,ensemble_weight}]与limits（另含schema_version）。
Runtime revision及有界大整数使用字符串，成员权重使用精确Decimal字符串。
current_weights_source为`{"kind":"FORWARD_SNAPSHOT","snapshot_id":"UUID"}`或
`{"kind":"LAST_TARGET","candidate_id":"UUID"}`，两种引用不能混用。LAST_TARGET
保留原目标、发布时间与期限，派生权重标为假设；不会读取真实账户或升级原来源。
CLI需要目标为mandate_id、完整意图相同的PORTFOLIO_BUILD人工grant及原幂等键；
返回202的原Run回执不代表Candidate已经生成或通过共享资金验证。

```sh
cargo run --locked -p server -- client --origin https://qz.example --credential-file cli-token --idempotency-key build-original --operator-grant GRANT_UUID portfolio build < build.json
```

Store核对当前资格、独立Reviewer/原REAL报告、许可、原模型、Forward目录及下游
原权重，不接收手填预测/持仓/费用。保守BAR使用原taker费用及显式原生滑点规划；
组约束使用原Forward Universe在决策时有效且已可用的唯一成员记录。
成员groups未提供/null表示未知，[]表示明确无组；有组约束时未知、歧义或组无参与
资产均拒绝，Candidate发布重读原来源。历史流动性/参与率见下文，DATA_BACKED仍未接通。
成功准入的完整原生链及Candidate发布仍待验收，不能将此命令当作交付入口。

原生SIMULATE_CANDIDATE仅为保持原目标的模拟适配，尚不是Operator评估命令。
参数含schema_version、candidate_id、candidate_available_ns、dataset_revision_id、target_artifact_id、
settings_artifact_id、原登记source_selection和原NativeSimulationRequestV1；需candidate-simulation/2。
原生`job study-portfolio --catalog PATH --objects PATH`从stdin读取
NativePortfolioStudyRequestV1，stdout输出NativePortfolioStudyResultV1；仅本机可信验证，
不是Operator/Agent准入接口。托管STUDY_PORTFOLIO需portfolio-study/6，只挂载原
DISCOVERY/VALIDATION目录、模型/校准MODEL和费用PARAMETERS；拒绝FORWARD/SEALED，
不改变目录原用途。输出qz.data_quality、qz.portfolio_study
及qz.portfolio_history/1 TARGETS（Apache Arrow IPC File）。后者按原帧/资产顺序
保存纳秒时间、decimal128(38,18)权重/独立现金列及求解状态；失败帧权重为null。
同manifest采纳逐项核对报告/请求与Arrow值，不接受缺失或换成JSON；完整列合同见DESIGN A5。
可选rolling_liquidity需portfolio-rolling-liquidity/1及Mandate.liquidity_ref的原
PARAMETERS政策文件，字段见DESIGN；不复用单次BAR快照的过期量。每帧使用本目录
前缀的原生BAR估值，原参与率约束实际权益下的权重变动；执行时再次检查年龄。
固定间隔或MANUAL原参数manual_cutoffs_ns的2–256次截止分别重算原模型与历史前缀，
手动截止严格递增、首项匹配评估开始且TTL覆盖下一项/评估末尾；不排序或补点。
在同一个Nautilus账户中按实际权益/权重调用Clarabel；不可行只保留诊断，不生成模拟。
CALENDAR_SESSION需portfolio-calendar/2，calendar字段绑定原PARAMETERS会话表及artifact_id；
字段/覆盖/原可用时间见DESIGN，必须逐值匹配原Runtime元数据登记的完整会话表。
Universe读取返回可空calendar_artifact_id，不返回会话内容或补默认表。截止取原close_ns加
显式秒偏移；不推断节假日、不排序、不补点，也不接受手动覆盖。
正式PORTFOLIO准入/发布仍待实现，不能授予PASS。
SIMULATE_PORTFOLIO_SEQUENCE使用portfolio-sequence/1，sources逐项绑定原Candidate、
可信可用时间和目标文件，同一settings_artifact_id重读核验；完整源质量与实际
模拟结果分别输出。不提供手填权重的正式评估API，Store序列准入/发布仍待接入。
同一任务复用原生数据检查，输出原登记窗口qz.data_quality及实际模拟窗口
qz.native_simulation；两者随原manifest绑定，不能把缩窄窗口行数当整个目录覆盖率。
唯一FORWARD目录搭配原qz.portfolio_targets/1 REPORT与执行设置PARAMETERS。
一个原目标点，生效/选择起点取原asof与原Candidate可用时间较晚者，终点不超过
原valid_until；可用时间须由可信准入绑定，不允许人工回填。job重读两份文件，
拒绝身份、权重/现金、时间或设置不同。不回放到Candidate产生前，不恢复实际账户，
成功进程不等于PASS或Release。可信Worker从原任务双报告发布Candidate的FORWARD
保持研究Evaluation，并在封口后才ACK；完整策略滚动评估及交付链仍待验收。
原政策portfolio_metric_requirements、日收益样本数、原目录载入覆盖及当前来源
分别核对；取消/失败和证据不足保留INCONCLUSIVE，重放不刷新有效期。

`POST /api/v2/candidate-simulations`与`client portfolio simulate`使用
CandidateSimulationRequestV1，只有schema_version、
cycle_id、candidate_id、input_set_id、runtime_id、expected_runtime_revision、limits，
人工操作标识PORTFOLIO_SIMULATE，CLI grant的target_id为candidate_id，绑定完整
请求与Idempotency-Key。202返回原Run，不是评估通过；同键同内容重放原Run。
不自行构造原生任务代替准入；完整科学/评估发布与交付链仍待验收。

执行假设入口为`POST /api/v2/execution-assumptions`，请求ExecutionAssumptionsCreateV1
（schema_version、project_id、runtime_id、expected_runtime_revision、input_set_id、
dataset_revision_id、完整NativeSimulationSettingsV1、settlement_rule_ref）。
需相同的Operator/精确CLI grant及幂等键；来源必须为已冻结非Sealed输入，费率和
币种匹配原登记Rust InstrumentAny，近期探测须支持PORTFOLIO_SIMULATE和锁定模型。
保存不可变原模型配置及来源，不启动模拟，也不证明DATA_BACKED或组合资格。

```sh
cargo run --locked -p server -- client portfolio assumptions create < assumptions-create.json
cargo run --locked -p server -- client portfolio assumptions list PROJECT_UUID
cargo run --locked -p server -- client portfolio assumptions show ASSUMPTIONS_UUID
```

读取对应`GET /api/v2/projects/{id}/execution-assumptions`和
`GET /api/v2/execution-assumptions/{id}`，分页/身份边界同Mandate，无修改或删除入口。
同一入口可选rolling_liquidity={schema_version:1,maximum_age_seconds,participation_limit}，
与bar_liquidity互斥。需portfolio-rolling-liquidity/1能力；返回原政策及
rolling_liquidity_artifact_id。没有历史快照期限，不表示跳过每步年龄校验；
登记不启动Study；Build另需portfolio-build-rolling/1能力。
原生Build的同名字段需portfolio-build-rolling/1，重读原政策PARAMETERS，在原选择
测量最后BAR；报告bar_notionals与求解资产逐值绑定。Store准入冻结原政策输入，
Candidate发布重读原政策并复核BAR年龄，过期保留求解状态但不发布可用目标。
当前声明式入口仅支持BAR/CONSERVATIVE_ASSUMPTION。可选bar_liquidity为
{schema_version:1,report_artifact_id:Id,maximum_age_seconds:正u32,
participation_limit:大于0且不超过1的Decimal字符串}；不使用时传null或省略。
必须引用同项目/Runtime/冻结输入中该Dataset已采纳的原生DATA_VALIDATE报告，
不是同schema的目录登记副本。全部原测量币种须匹配基础币种，创建时仍在明确期限内。
读取返回原配置与bar_liquidity_valid_until（到期边界不含）；过期不自动刷新，
需新建假设。该单BAR历史规划上限不保证未来成交，不授DATA_BACKED或组合资格；
Store在Build准入与Candidate发布时重读原报告/配置、核对当前许可与原期限，
最终数据库时点再次检查到期；来源损坏保留重试，真实到期不再具备资格。
原生Build请求的可选bar_liquidity冻结schema_version=1、assumption和source
（原Dataset/选择）；报告必须以DATA_QUALITY角色提供原字节。job逐项核对原报告
与assets.available_notional、原选择、币种、年龄及Mandate参与率，不接受无绑定
的数值。需portfolio-liquidity/1镜像能力及原结果版本声明；不得把原生检查替代
Store来源采纳与当前期限检查。DATA_BACKED和完整独立组合验证仍待完成。
原生Build还必须冻结完整execution_settings，并以PARAMETERS角色传入原
transaction_costs_ref文档；job核对完整原字节解析值、币种、本金及逐资产taker费用。
准入与结果均要求portfolio-cost-source/1，Candidate发布重读原文档与保存配置。
同一配置还须匹配本次Forward原instrument definitions的币种及maker/taker费率；
Build复用模拟的原生市场检查，发布重验原目录，拒绝同名资产沿用另一版本费率。
非零prob_slippage需portfolio-slippage/1，结果必带slippage_references：逐资产原
instrument_id/currency/event_ns/available_ns/close_price/price_increment。job提取
原最后BAR，规划比例为f+p·tick/close·(1+f)，向上舍入18位；发布核对原tick和
结果系数。零概率时引用列表为空。仅为参考价下舍入前的模型期望，不是未来上界、
逐笔实付费用或DATA_BACKED；实际模拟不再扣一次规划成本，详见DESIGN A5.2。
原数据不改写；没有此原生来源关系的历史行不投影成新接口版本。

`POST /api/v2/portfolio-mandates`接受MandateCreateV1：schema_version、project_id、
runtime_id、expected_runtime_revision与完整content（DESIGN A5）。需要近期Operator
或该完整意图的单次CLI grant及Idempotency-Key，返回201/CommandResult_MandateViewV1。
同键重放原版本，不因后来探测变化重新创建；不同意图409，新键分配新的项目内版本。
没有PATCH/DELETE，配置变更必须新建版本，不覆盖已引用的Mandate。

读取`GET /api/v2/projects/{id}/portfolio-mandates`（UUID cursor、默认50/上限100）
和`GET /api/v2/portfolio-mandates/{id}`，只允许Operator或精确项目RESEARCH_READ的CLI。
Mission、Automation、Downstream不能借这些入口取得配置权。

```sh
cargo run --locked -p server -- client portfolio mandate create < mandate-create.json
cargo run --locked -p server -- client portfolio mandate list PROJECT_UUID
cargo run --locked -p server -- client portfolio mandate show MANDATE_UUID
cargo run --locked -p server -- client portfolio candidate list PROJECT_UUID --limit 50
cargo run --locked -p server -- client portfolio candidate show CANDIDATE_UUID
cargo run --locked -p server -- client portfolio candidate evaluations CANDIDATE_UUID --limit 25
cargo run --locked -p server -- client evidence show EVALUATION_UUID
cargo run --locked -p server -- client evidence metrics EVALUATION_UUID --limit 25
```

写入仍按CLI全局选项携带同一幂等键和精确人工grant，不把TOTP或凭据写入请求文件。
创建校验原生模型版本、有效Runtime探测、CONVEX_QP、执行镜像、政策项目及原执行
假设，币种/资本/费用/流动性/参与率/日历须一致。无能力或引用不一致时不落版本。
保存配置不是科学PASS、Alpha资格或Candidate/Release交付；Ant Design操作页尚待接通。

已有受授权只读 Nautilus Parquet 快照、实际 Wasm 模型和相应冻结请求文件时，运行时使用以下入口；路径不是 HTTP/MCP 请求字段：

```sh
job forecast --catalog /input/catalog --model /input/model.wasm < forecast-request.json
job validate-alpha --catalog /input/catalog --model /input/model.wasm < alpha-validation-request.json
job evaluate-sealed-alpha --catalog /input/catalog --model /input/model.wasm --calibration /input/calibration.json < alpha-sealed-request.json
job simulate --catalog /input/catalog < simulation-request.json
```

请求分别是 `NativeForecastRequestV1`、`NativeAlphaValidationRequestV1`、`NativeSimulationRequestV1`，统一定义在Rust合同。stdin 上限8MiB；stdout为完整JSON，计算失败为非零退出码及安全的 `QZ_NATIVE_JOB_FAILED`，不回显原生异常、路径或输入。`--catalog` 只允许运行时的已登记只读挂载，`--model` 不接受软链/FIFO/超限文件；外层仍须配置真实进程、文件系统、网络和资源隔离，不能直接用这些本地参数授予Agent宿主访问权。

`validate-alpha`请求含schema_version=1、forecast（完整原生预测请求）、split_policy
及target_kind=SCORE/EXPECTED_RETURN。固定horizon必须一致；每折训练/测试重建模型，
训练标签专用于原生OLS校准。输出所有折的预测/标签/训练索引、真实训练标签截止
training_end_available_ns、IC/RMSE及缺失原因，
不平均不同折或授予资格。unique_test_observations只去重，不证明样本独立。
仅用于验证分区CV，不允许将SEALED数据作为训练标签。此本地数值入口不是Agent工具
或已发布Evaluation；不能手工上传stdout替代可信采纳。

可信Worker在原正式Validation发布事务内冻结每资产最后一个原生折的SCORE模型，
保留精确训练子集、原生系数、报告/Evaluation/InputSet关联；仅SUCCEEDED + VALID
且所有资产最后折可校准时产生记录及原Alpha的下一不可变版本。它不是新的CLI/Agent
写入口，不改变源Alpha版本、原试验或REJECT决定，不复制评估/资格；无可用校准
`alpha qualifications <version-id> [--cursor <id>] [--limit <n>]`只读原资格历史。
grant_window_open仅表示服务端checked_at处于授予时间窗且撤销未生效，不检查当前
政策、生命周期和许可证，不可据此交付；最早撤销包含未来生效记录，不隐藏过期历史。

不能回退赢家折或手填scale。`alpha calibration <version-id>`只读该版本已附加的
校准元数据与源版本Validation；未附加返回404，不下载系数/训练行。见DESIGN A4.4。

`forecast` 保留未完成标签与指标预热的 null+reason，Wasm没有宿主导入且受fuel/内存/栈限制。`simulate` 在一个原生账户执行全部资产的冻结目标，先确认减仓成交再提交增仓，保留原生费用、数量步长及独立结果。公开 `returns_kind=PORTFOLIO_DAILY` 仅含原生权益快照的UTC日收益，绝不使用单仓收益回退；日内数据不足时 `returns_status=INSUFFICIENT_DATA`、`returns_reason=PORTFOLIO_DAILY_RETURNS_UNAVAILABLE`，不是0收益。跨日全现金的真实0收益可以为OK，但仍须符合评估最小样本要求。

`simulate`的settings必须显式带fee_model、fill_model、latency_model原生引用，
分别为NAUTILUS_MAKER_TAKER、NAUTILUS_DEFAULT_FILL、NAUTILUS_STATIC_LATENCY，
完整类名及参数见DESIGN A2/生成合同，版本均为0.63.0。费用参数为空对象；填充参数
为prob_fill_on_limit、prob_slippage及random_seed；延迟参数为base_latency_ns、
insert_latency_ns、update_latency_ns、cancel_latency_ns。概率用Decimal字符串，
种子/纳秒用DbCounter字符串，插入总延迟必须大于零。旧settings.insert_latency_ns
和缺失模型不兼容；不使用随机默认种子、默认填充或默认零费用。费率仍必须逐资产
匹配目录原生定义。新镜像声明simulation-models/1，旧镜像不能冒充这一执行合同。

`evaluate-sealed-alpha`仅为可信本机数值入口，输入NativeAlphaSealedRequestV1，
显式绑定forecast、target_kind和research_available_through_ns；SCORE必需原冻结
校准JSON（最多8MiB），EXPECTED_RETURN省略--calibration。Wasm仍最多2MiB，
标准输入最多8MiB。复用原预测/校准/指标，不训练Sealed；输出保留原预测与逐行
expected_returns、真实完整标签数量及缺失原因。此命令不预约读取机会、不授予
目录权限或资格；受管Runtime已有独立内部操作，但尚未开放完整封存准入。
不要把本机命令交给Mission执行。

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

`VALIDATE_ALPHA`是ALPHA_EVALUATE的独立分折操作，固定原dataset_revision_id/
model_artifact_id/request；只接收VALIDATION分区、MODEL、PARAMETERS，不能读
Sealed训练数据或借用Discovery预测。原生结果为qz.alpha_validation.v1 REPORT，
含每折source_row_count和全部原始索引；严格采纳核对原请求全部分折，不等于授资格。
受信任指标转换的scope/单位/频率/时间精度见DESIGN A4.6；这不是新的Agent工具或
手工上传MetricValue的入口，原始REPORT仍完整保留，不用转换值覆盖原纳秒证据。

`brief freeze`和`cycle start`重验已登记Validation元数据及原生方法能力，不接受
total或错版本/单位/周期的Selection。当前单Validation目录版本可含多资产；多版本
或非固定bars明确拒绝，不选择子集继续。政策登记成功不表示能够冻结执行。

Sealed要求还须使用真实资产的`asset:N`，不能引用Validation的fold；原Sealed目录
资产/bar顺序须与Validation一致，仅核对登记元数据，不读取市场行。旧政策缺少
Sealed要求明确报SEALED_POLICY_NOT_DEFINED，不补阈值或开始封存任务。

内部EVALUATE_SEALED_ALPHA操作绑定原SEALED目录、Wasm MODEL及SCORE的原校准
MODEL，输出qz.alpha_sealed.v1。PARAMETERS只引用校准产物ID，不复制拟合配置；
Job与可信采纳分别读取原对象。该操作不是新的Agent工具或手工上传评估入口，
不替代Sealed机会预约、独立Review和资格判定。Runtime镜像须含alpha-sealed/1。
首次受管Sealed能力需同事务登记原Attempt的读取机会；原验证已过期、不独立、
累计机会耗尽或缺少精确Alpha/政策绑定均不能取得能力。重放不重复消费，取消不退款。
可信Worker在ACK前发表原SEALED评估及asset:N指标；使用独立封存阈值，不借源验证PASS。
失败/取消仍正式记录INCONCLUSIVE，报告限EVALUATOR_ONLY；发表不授Reviewer或资格。

人工`alpha evaluate <alpha_version_id>`调用`POST /api/v2/alpha-versions/{id}/evaluations`。
输入为`schema_version=1, cycle_id, policy_id, input_set_id, runtime_id,
expected_runtime_revision, limits`；limits.experiments必须为0，其余原生资源限额必填。
必须使用同项目RUNNING Cycle冻结的政策、Sealed输入和Runtime，仍占该Cycle累计预算；
新政策使用新Cycle，不修改源模型。CLI使用ALPHA_EVALUATE原请求及精确Alpha目标的
Operator grant，HTTP返回202和原Run。重放不重新读写参数，不重复计原编译试验。
模型、原Validation校准及研究可见截止由服务确定；不能填写镜像/路径/模型或PASS。
本入口只接受评估，不授Sealed读取能力、Reviewer结论或资格；首次读取仍按原Attempt预约。
机会已耗尽或已有不相容披露时，尚未授能力且未发送的Run会记录
FAILED/SEALED_OPPORTUNITY_UNAVAILABLE，并在失败评估发表后确认队列；不假称远端执行失败。

正式验证的内部准入绑定原Alpha/Policy/Validation目录，不增加可由Agent指定Run或
免费试验的CLI/MCP接口。原生Worker通过A4.8收尾入口在终态采纳后、ACK前原子发表
评估/逐折指标/试验结论；不是手填PASS命令。qz.alpha_evaluation和原始分折报告均
EVALUATOR_ONLY。Mission自动发起该阶段，并等待评估和原Thread回答后才能收束；
这仍不授予Alpha资格、独立Reviewer批准或Cycle交付结论。

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

`POST /api/v2/integrations/downstreams/{id}/probe` / `client downstream probe <id>`
接收 DownstreamProbeRequestV1：schema_version=1、expected_revision，需要近期人工认证
或精确 DOWNSTREAM_PROBE 单次grant。200只表示观察已记录，检查 outcome 与 readiness。
`GET /api/v2/integrations/downstreams/{id}/readiness` / `client downstream readiness <id>`
只读；DOCTOR_READ可用，不启动网络。返回原观察、配置revision、可用版本/环境交集。
探测开始后60秒失效；配置修改或较新失败不能用旧成功、较早开始的迟到响应或回执
重放覆盖。accepting_targets=false、空交集、过期和未探测都没有交付准入资格。

serve 和 worker 的 `--downstream-targets` / `DOWNSTREAM_TARGETS` 使用下述 origin/addresses 格式，
与 RUNTIME_TARGETS 独立，默认[]。原生下游固定 GET /downstream/v1/capabilities，只读
DownstreamCapabilitiesV1 target-only合同。网络在事务外，总请求10秒；完成采纳总期限
20秒，ArtifactStore与不可变观察/回执关联。人工审批/Offer/Claim已消费该观察，完整交付链尚未验收，不能据此宣称
完成交付。回归：隔离PostgreSQL执行 `cargo test --locked -p store --test downstream`、
`cargo test --locked -p server --test downstream_http --test downstream_transport`。

Worker现自动刷新尚未到期的未领取Offer及ACTIVE项目当前有效自动政策所需的下游；
每个进程最多一个探测，跨进程使用原生数据库短租约。观察还剩15秒以上不重复请求，
失败/崩溃至少30秒后重试，完成采纳限20秒，原观察期限仍为开始后60秒。配置和租约
失效、发布失败或数据库写入超时会回滚；清理等待同一下游发布锁，未知提交不删已引用
文件。使用同一STATE_DIR原DOWNSTREAM凭据和独立部署允许列表，不继承Runtime目标。
关闭Worker停止新领取并等待已开始的有界I/O。自动刷新不创建审批或交付；冻结政策
自动消费与完整交付仍待实现。

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
数据源/数据版本使用本文原生登记入口；执行假设使用`portfolio assumptions`。
这些入口保存来源与配置，不替代完整Candidate与交付资格验收。
PORTFOLIO是输入用途，不是数据分区；其成员保留DISCOVERY/VALIDATION，至少需
RESEARCH_AND_PAPER许可。共用读取器仅按调用方明确允许的用途消费，DATA_VALIDATE
仍仅接受DISCOVERY/VALIDATION用途，不能拿PORTFOLIO头替代它或据此启动正式Study。

评估政策创建须显式提供 `sealed_metric_requirements`（1..64项，至少一项required），
与Validation的`metric_requirements`分别冻结；selection按evaluation_kind绑定对应组。
历史政策该字段为null，不得自动复制阈值用于Sealed；需要新建完整政策和研究周期。
`portfolio_metric_requirements`独立冻结组合指标要求：null表示纯研究政策，不能
授予组合PASS；非空为1..64项且至少一项required，沿用精确阈值/code/scope/方法
allowlist校验。不得从Alpha或Sealed组自动复制；历史政策保持null，不原地补写。
保存条件不表示原生方法支持或Evaluation发布已接通。
可同时提交portfolio_study_plan={schema_version:1,input_set_id,evaluation_start,
manual_cutoffs:null|Time[2..256]}，必须有组合阈值、同项目PORTFOLIO输入及唯一原数据版本。
起点严格位于原数据范围内，结束固定取原event_end；手动时点从起点严格递增，
均早于原结束。时间为非负、微秒精度RFC3339；省略/关闭计划保持null，不启动Study。
改变计划必须新政策，运行命令不能更换窗口；Mandate调仓模式、TTL和模型可用截止
仍须正式准入核对。列表/详情返回原完整计划，不读原始市场字节。
Store的start_portfolio_study已实现受Operator授权的原计划准入，严格意图仅为
schema_version/cycle_id/candidate_id/runtime_id/expected_runtime_revision/limits。
正式入口为POST /api/v2/portfolio-studies及client portfolio study，从stdin读取
该严格意图，需Idempotency-Key；CLI使用同一PORTFOLIO_STUDY命令签发Operator grant。
它绑定原成员、费用和政策，复用PORTFOLIO_SIMULATE权限、预算/PGMQ，返回202 Run。
网页候选详情的“请求组合 Study”提交同一六字段意图；只读原计划，不覆盖输入或成员。
可信Worker为原Study绑定发表独立PORTFOLIO评估，现有Candidate评估读取保留
PORTFOLIO与FORWARD类型。完整三报告/Arrow及来源重验、发表回执后才允许ACK；
取消/失败/不可行不产生PASS，不能调用HOLD结果替代Study或据此自动交付。
原生组合指标适配支持portfolio范围的PORTFOLIO_DAILY_RETURN_MEAN、
PORTFOLIO_RETURN_VOLATILITY、PORTFOLIO_SHARPE_RATIO；方法分别为
nautilus-analysis.ReturnsAverage/ReturnsVolatility/SharpeRatio，版本0.63.0、
频率UTC_DAY，后两项保留原生252日年化。单位与缺值合同见DESIGN A5；不把
日内不足样本补零，不借指标映射授予Evaluation或Release。
Candidate模拟Run同事务冻结原Candidate、政策、Forward数据版本及完整请求；
相同Idempotency-Key重放不重新选择来源，失败不遗留任务绑定。
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

Wasm提案的PARAMETERS内容采用
`{schema_version:1,dataset_revision_id:UUID,parameters:{schema_version:1,fast_period,slow_period,label_horizon_observations,total_fuel}}`。
dataset_revision_id明确选自该Brief冻结Discovery；label_horizon_observations必须等于
FIXED_BARS Brief的horizon_value，其他horizon当前不支持。fast_period>=1，
fast_period<slow_period<=10000，horizon为1..100000，total_fuel为1..1000000000的
十进制字符串。不得带MODEL ID、宿主路径或额外字段；模型由原编译Run确定。
这只说明可信预测准备入口接受的文档，不表示artifact上传或experiment.propose会自动执行。

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

自动 Paper：ACTIVE 项目当前有效 AUTO_PAPER/AUTO_HANDOFF 政策由 Worker 轮询消费，原审批和 Offer 同事务产生。每日限额按数据库 UTC 日、原项目/下游及不同 Candidate 计数，包含人工记录；换政策版本不重置。政策替换、禁用或撤销阻止未领取记录继续领取，已领取事实不改写。`client handoff list PROJECT_UUID --limit 50`（可选 `--cursor UUID`）查询原绑定与当前状态；下游凭据仅见自己的记录。Live 自动晋级已接入同一 Worker，条件与证据边界见下段。

自动 Live：仅当前有效 AUTO_HANDOFF 政策可消费原 Candidate/下游的 Paper 观察。全部已报告原 Paper Handoff/stream 均须有当前原生 HEALTHY 观察，每个流分别满足样本数、完整窗口时长与两组指标；不合并样本或挑选有利流，超过255个流拒绝。审批与Offer同事务冻结完整排序的观察UUID集合；首次Claim重验同一集合、完整来源、Live数据用途、Release与下游readiness。新流、更正、撤权或过期会阻止旧证据继续授权；已有Claim重放保持原事实。同一Candidate当日Paper/Live合计占一次额度。人工Live审批行为不变，完整市场/部署验收与界面仍未完成。

Forward 报告：原 Handoff 领取后，精确项目/下游 FORWARD_SUBMIT 凭据使用 `client --idempotency-key MESSAGE_ID forward submit < forward-message.json` 提交 ForwardMessageSubmitV1（完整字段见 DESIGN A7.3）。external_message_id 必须与请求头/CLI的MESSAGE_ID一致，是原幂等编号，未知结果保持原报告重试；换编号重传相同逻辑消息也只返回原记录。纠正必须引用最新原消息、revision加1并保留窗口。三个时间使用UTC微秒精度；原始收益报告仅保存在EVALUATOR_ONLY Artifact，不能夹带账户/NAV/订单或执行权限字段。`client forward list PROJECT_UUID --limit 50 --cursor UUID`只读元数据；首次省略cursor，下游仅见自己的记录。收到报告不表示连续窗口、统计评估或Live晋级已通过，这些消费链仍在实现。

`client forward window HANDOFF_UUID --stream STREAM`读取原始窗口来源投影：最新纠正版、连续性、合格样本数及缺序/partial/缺失/重叠原因。partial纠正不会回退旧complete版本，任一缺口或重叠将合格计数清零；原报告仍不公开。空stream返回NO_MESSAGES。此投影尚不是已封口的Forward统计评估，不授予自动Live或Wake资格；完整字段与原生来源/容量边界见DESIGN A7.4。

Forward收益频率：报告可声明 `returns_frequency=UTC_DAY`（完整UTC日，样本为右端午夜，complete不得缺日）或 `REPORTED_OBSERVATION`；缺失表示未知，不能推断日频/年化或独立样本。同stream含纠正历史的频率不一致时窗口返回 `FREQUENCY_MISMATCH`、合格计数为0。此来源检查尚不等于原生ForwardEvaluate完成。

原生 ForwardEvaluate 科学job已支持冻结原REPORT输入、原纠正链重验及nautilus-analysis 0.63.0的UTC日均值/波动率/Sharpe（365日年化）。Runtime仅在明确登记FORWARD_EVALUATE固定镜像后宣布该能力；不接受额外目录/模型。当前已具备受冻结政策约束的Store准入与原Run/PGMQ队列，已接原Worker终态Evaluation/window发布，自动调度已接通，原生观察、待处理Wake及受限Cycle消费已接通，尚待Live晋级，不把直接job结果当作Live/Wake资格；输入/结果/上限见DESIGN A7.5。

Forward可信准入仅供内部Worker调用：沿用原Candidate Runtime，完整原日频报告在项目锁内冻结为受限FORWARD输入，固定30CPU秒/60墙钟秒/512MiB/1MiB输出、无Cycle并发2，同来源重放不新增Run。纠正、撤销、替换或过期会阻止未发送任务；普通InputSet接口仍不能复制受限报告。原Worker会在终态采纳后、ACK前发表测量Evaluation与封口窗口，decision为INCONCLUSIVE；更正/撤权/过期或不完整统计不产生有效样本。Worker沿用五秒项目轮询，每轮至多预约一个原反馈流，失败三十秒后公平重试，新增/纠正可提前重试；相同来源不重复入队。Worker在ACK前按原政策追加原生观察：维持要求不通过为DEGRADED，维持通过但晋级要求不通过为WATCH，两组通过为HEALTHY，缺失/过期/无效为INSUFFICIENT_DATA。只有当前有效DEGRADED追加唯一待处理Wake；观察/Wake失败保留原消息重试。Wake消费复用原人工研究上下文及冷却/日额度；Live晋级消费完整原生HEALTHY观察集合，详见DESIGN A7.6–A7.11。


原生退化 Wake 由现有可信 Worker 自动化轮询消费，继承原 Candidate 研究 Cycle 的
人工 Brief/Profile 上下文并重验当前数据/运行时/政策。冷却与 UTC 日额度不足延后；
暂停保留待处理记录，更正/过期/撤权取消旧 Wake。与 DATA_VALIDATE/PGMQ/startup
同事务绑定唯一新 Cycle，不发放 Operator 身份或交付权限。无 Agent/HTTP 手动强制
消费入口；可用 `cargo test --locked -p store --test forward_evaluation --test cycles`
验证原生事务和受控协议，不能把该测试称为真实多日反馈/模型/OCI验收。
