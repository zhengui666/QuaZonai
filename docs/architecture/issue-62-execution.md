# Issue62 implementation evidence

DESIGN.md is the normative contract. This file records implementation and
version-bound evidence, not a second design or a claim that Issue #62 is complete.

## Implemented source boundaries

The first-party tree uses `apps/job`, `apps/server`, `crates/contracts`,
`crates/domain`, `crates/store` and `crates/integrations`, without `qz-` directories.
Legacy backend/frontend/plugin/deployment implementations, compatibility services,
obsolete tests and archived designs have been removed. Git history, LICENSE,
NOTICE, third-party notices and user-data boundaries remain intact.

Scientific foundations reuse Rust-native Nautilus0.63.0, Clarabel0.11.1 and
Arrow56.2.0 on Rust1.98.0. There is no production Python bridge. Native feasibility
is not complete shared-capital research/portfolio acceptance. The earlier native
fixture produced weights 0.7999999999997491 / 0.20000000000025078, 745 iterations,
12 native orders and 24 native events with an Arrow round-trip. It is explicitly
FIXTURE, `deliverable=false`, `python_runtime=false`, not investment evidence.

The typed contracts and pure rules include decimal/bigint boundaries, metric
nullability, UUIDv7, ISO currencies, Codex default-setting omission, budget and
mission-turn limits, qualification, cancellation, version and lease fencing.
Both Rust parsing and generated schemas consume shared decimal/bigint corpora.
These rules do not replace persistent authorization or native component tests.

SQLx0.8.6 owns migrations, transactions and independently migrated test databases;
PostgreSQL/PGMQ owns durable queue delivery. The Store owns project-specific
relationships, immutable identities, budgets and reconciliation decisions. A
Mission has one immutable Session/Thread. Reservation plus PGMQ send is atomic;
one committed dispatch intent grants one send, while retries reconcile. Native
binding, terminal and usage receipt are separate immutable facts. Missing usage
retains reservations; pause, cancellation or lease loss never fabricates a refund.
Exact receipts precede queue acknowledgement.

The relational constraints bind artifacts, inputs, experiment ancestry, Alpha
qualification, frozen candidate contents, evaluations, Release and approval/offer
identities. They also bind machine principals/scopes, forward corrections,
Codex profile revisions, event cursors and run/attempt result manifests. Creating
these records does not itself expose or implement all corresponding services.

The HTTP authentication vertical in `apps/server` reuses Axum0.8.9,
tower-sessions0.14, PostgreSQL PostgresStore0.15, totp-rs5.7, Argon2id,
RustCrypto XChaCha20Poly1305 and cap-std. There is no memory-session fallback.
Clap exposes `init-state`, `migrate`, local one-use `bootstrap`, `serve` and native
OpenAPI. Login uses TOTP only, with a bounded bootstrap capability, native private
cookie, database replay prevention, expiry/epoch/revocation and atomic rate limits.
Device lists paginate and used trusted devices update activity. This vertical
does not provide research, Reviewer or approval authority to an Agent.

## Evaluation publication and revocation correction

The additive `202609060005_evaluation_publication.sql` seals a completed
Evaluation and its metric membership together. A deferred native constraint
trigger publishes the aggregate before commit; consuming references can seal it
earlier in that same transaction. Later metric inserts are rejected. Candidate
and allocation-Evaluation circular creation retains its existing deferred foreign
key and is covered by a positive regression. Upgrade backfills only the internal
publication markers and does not rewrite historical evidence.

Degradation observations must join the exact project, policy mandate, Release
candidate, FORWARD Evaluation and frozen InputSet, backed by the corresponding
Forward evidence window. Historical incompatible rows abort migration rather than
being deleted or silently relabeled. This relationship check does not replace
current policy authorization, freshness, degradation thresholds or Wake admission.

Browser session epochs cannot move backward. Equal epochs allow ordinary state
maintenance; newer epochs invalidate old authority permanently, with native bigint
overflow failure rather than wrapping. The regressions check actual browser/device
authority and a real concurrent row-lock wait, not just the stored integer.

The native Codex probe validates the exact observed `originator/version` product
token against pinned upstream source and publishes that observed value. It rejects
version prefixes, prerelease/build suffixes and a matching string elsewhere in the
user agent. These narrow format regressions do not count as real-account model
inference. The normal native stdio probe remains a separate required CI execution.

The source adds twelve PostgreSQL regression functions and two native-version
unit tests. Their definitions alone are not acceptance evidence: each execution
must identify its exact source/tree, committed lock, command and outcome. Temporary
public development-input exporters are removed after tools are retrieved; no tool
archive, user database, secret or standalone patch publisher is part of the product.

## Historical evidence: exact baseline, not the current Head

At `e8668ca850def834735414ed9ba94fed38d4aa7e`, the suites contained 8 contract,
28 domain and 8 native/report tests: **44 total**.
[CI 33962063262](https://github.com/zhengui666/QuaZonai/actions/runs/33962063262)
validated that historical dependency lock. The upstream feasibility run
33952841460 is a development study, not a final product gate.

The latest baseline inspected for this correction is
`19496333808afc71d794af0871e5ef9704a3507a` with
[CI 34007972993](https://github.com/zhengui666/QuaZonai/actions/runs/34007972993):

| Job | Actual result at that exact baseline |
| --- | --- |
| `database-native` | Success: native PostgreSQL/PGMQ transaction contract |
| `store-postgres` | Success: **61 Store + 7 server = 68 tests**, zero failed/ignored |
| `rust-native-contracts` | Failure at `cargo fmt --all -- --check` on `evidence_bindings.rs` |
| `foundation-checks` | Failure, because not every required job succeeded |

The 61 Store tests in that job were auth 8, authority invariants 7, relational
constraints 7, device activity 3, evidence bindings 12, terminals 4, turn recovery
4 and turn transactions 16. The server suite had 7 tests; its database-backed
cases include an actual loopback HTTP listener using a distinct non-owner login.
The artifact `store-evidence-19496333808afc71d794af0871e5ef9704a3507a`
contains `tests.log`, `tested-commit.txt`, exact migrations, Cargo.lock and native
database/image versions. This count describes that artifact only.

Clippy, Rust tests, native scientific verification and Codex protocol probes were
**not completed by the failed native job at that baseline**. The separate passing
Store job cannot turn those skipped steps into success. Earlier unqualified
52/87/97-test snapshots are retired as current evidence: never combine different
commands, development locks or commits into a latest-Head pass count.

## Review correction in this source

`verify_runtime_role` inspects native PostgreSQL role membership, ACLs and ownership
across the entire `app` schema rather than sampling `operator_auth_state`. It
rejects destructive table privileges, schema CREATE, app object ownership and
elevated roles reachable by inheritance or SET ROLE, including an elevated
`session_user` hidden behind a restricted `current_user`. Ordinary non-owner DML
access remains supported. This is a startup guard, not a substitute for continuing
least-privilege administration or a complete machine-authorization service.

Before a new reservation or first dispatch, the Store holds the referenced Brief
lock and requires its frozen budget to equal the Cycle snapshot. The complete JSON
comparison includes tokens, costs/currency, turns, repairs and resource limits.
Existing receipt/terminal reads and actual-usage reconciliation remain separate
from permission to start new spending. Historical malformed snapshots cannot
expand new work or justify deleting actual consumption.

The additive `202609060004_doctor_boundary.sql` migration confines DOCTOR_READ to
a read-only CLI/AUTOMATION credential, never DOWNSTREAM/MISSION or a mixed research/
delivery scope. Existing issuer, epoch, project, Mission-lifetime and lock checks
remain. Previously issued invalid Doctor credentials require an explicit effective
revocation before migration can succeed; their immutable issuance stays in audit.
The upgrade regression uses native SQLx to apply the old migrations, verifies the
specific failing migration/SQLSTATE, then revokes and reapplies without rewriting
historical rows or changing applied migration checksums.

Terminal retries first verify the exact reservation/attempt binding, then compare
all immutable terminal fields before requiring a live fence. They recheck after
lock waits. Identical committed facts remain readable after lease expiry/takeover;
conflicting outcome, native identity, reason or observation time is a conflict.
Creating a missing terminal still requires current owner/epoch/lease. A terminal
never creates a usage receipt, releases a reservation or acknowledges the queue.

The correction adds 12 PostgreSQL regression test functions: 4 runtime-role,
2 budget-authority, 2 Doctor boundary/upgrade and 4 terminal retry/race tests.
The source also formats the existing evidence-binding suite without weakening CI.
**A test definition or successful `--no-run` compilation is not a passed database
test.** The modified source must obtain its own published-Head CI logs and review;
the historical 68-test result above must not be reused for it.

## Native upgrade cutover and control-plane vertical

The current source adds `Store::migrate` with the native SQLx migration lock on a
closed-on-exit dedicated connection and a transaction covering ordered PostgreSQL
application-table write barriers plus all pending native migrations. The existing
migration files retain their original bytes/checksums. Additive migration 0006
repairs a possible pre-guard cutover gap and invalidates all historical browser
epochs once, with an immutable audit receipt; it does not delete historical facts.
Upgrade regressions use real prior-schema writers, lock waits, failed batches,
connection cancellation and a bad observation committed while migration waits.

`contracts::control`, `domain::control`, `store::authority/commands/control` and
`server::access/control` implement a real Project/identity vertical. Native
Argon2id verifies bounded opaque Bearer capabilities from encrypted SecretVault
objects. Cookie and Bearer channels cannot be mixed or used as fallbacks. The
locked business transaction rechecks exact credential epoch, expiry, revocation,
project/Mission and scopes. Recent human authority, one-time CLI grants with full
request snapshots, CAS and immutable original response receipts are enforced in
the same transaction. No token/verifier enters public DTOs or receipts; issuance
retries never return the raw secret again.

The source tests cover real HTTP/private-cookie/TOTP/Argon2/SecretVault/database
requests, cross-project reads, forbidden automation management, grant substitution
and parent-resource binding, actual revocation lock waits, concurrent same-key
commands, stale CAS and rollback. These are bounded implementation proofs, not a
claim that the still-missing research/CLI/MCP/UI/worker/production acceptance is
complete. Current totals and outcomes must be taken from the exact tested commit's
CI log; historical totals above remain explicitly historical.

## Control review corrections: replay, verifier ownership and native rate limits

The grant HTTP path checks the authenticated CLI's exact existing receipt before
fresh TOTP/reauth quota, while still enforcing current authority and global epoch.
Credential issuance now holds the existing command transaction before materializing
a verifier. Reconciliation locks that same authority row and consults immutable
credential history on the primary before deleting only an authenticated, unpublished
MACHINE_VERIFIER object. A local maintenance command covers cancellation/crash orphans;
unknown database outcomes preserve files. No secret enters receipts or metadata.

PostgreSQL shared global/per-credential windows bound failed/in-flight native Argon2
checks. Successful verification refunds only its original windows, with independent
machine and human crypto slots. Native HTTP, SQL concurrency/rollback, filesystem
purpose/symlink and bounded-window tests exercise these paths. Counts and results
belong to the exact CI Head, not an unversioned claim in this document.

The database-native CI waits for final TCP readiness, not the official image's
initialization-only Unix socket server. The PGMQ contract itself remains unchanged.

## Verification commands and evidence rules

Run with the committed lock; do not format or regenerate tracked code inside a
read-only acceptance job. Generated outputs must compare equal to committed files.
Use an isolated native PostgreSQL/PGMQ database, never an existing user database.
The CI workflow pins and verifies the exact pull-request Head before execution.

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --workspace --all-targets
cargo test --locked --workspace --exclude store --exclude server
DATABASE_URL=postgres://TEST_USER@127.0.0.1:55432/postgres \
  cargo test --locked -p store -p server -- --test-threads=4
cargo run --locked -q -p contracts --example generate > /tmp/domain-v1.openapi.json
diff -u contracts/generated/domain-v1.openapi.json /tmp/domain-v1.openapi.json
cargo run --locked -q -p server -- openapi > /tmp/api-v2.openapi.json
diff -u contracts/generated/api-v2.openapi.json /tmp/api-v2.openapi.json
node tests/contracts/decimal-wire.mjs
node tests/contracts/bigint-wire.mjs
```

The workflow additionally executes the native solver/Nautilus/Arrow report,
locked official Codex stdio/schema probe, PostgreSQL/PGMQ transactions and legacy-
path rejection. The aggregate requires every foundation job. Record the actual
commit, command, zero/nonzero exit, passed/failed/ignored counts and artifact/run;
compile-only, missing credentials, skipped steps or cancelled runs are not passes.
No test fixture or source archive is evidence of a real subscribed model request.

Store restart tests recreate a client, not an OS/container crash. Real lock waits,
lease clock checks, missing-queue rollback and independent test databases cover
specific relational/concurrency contracts; they do not prove deployed isolation,
production TLS, backup restoration or end-to-end research. Crypto is native, but
a passing authentication test does not establish all role-specific research rights.

## Native service schema hardening (2026-09-06)

Based on remote `0f71046d4bb69b8d37b76429000e2d33daea57be`, the runtime
privilege query now covers app, tower_sessions and pgmq; missing service schemas,
object/schema ownership, CREATE, TRUNCATE, TRIGGER and reachable delegated
privileges fail closed. Ordinary native session and queue DML remains allowed.
The session catalog contract also verifies persistence, RLS, constraints, triggers,
rules, inheritance, column semantics and native index operator classes.

Three new PostgreSQL runtime-role tests and one real CLI migration test exercise
these boundaries. The migration test tries fourteen incompatible definitions and
checks that the epoch, exact migration history and existing session bytes stay
unchanged after each failed command; an ordinary expiry index and native session
CRUD remain positive controls. The two representative new tests were also run
against the unchanged baseline SQL and reproduced the original missing rejection,
not a connection/setup failure. No existing migration or Cargo.lock was edited.

The full locked local workspace, strict Clippy, formatting, build and native
contract/scalar comparisons passed for this candidate. Exact test totals and the
candidate tree are recorded in the PR evidence comment; this paragraph is not a
permanent claim that every future Head passed. Remote CI and independent review
must validate the published Head separately, and none of these checks replaces
complete product acceptance.

## Administrative delegation and exact evidence producers (2026-09-06)

This iteration starts from remote `6cada81a6f94bfa29831cf4ede220c8d3ca4e711`.
Native PostgreSQL membership traversal includes ADMIN OPTION even when INHERIT
and SET are false, follows delegated paths, and retains the original session
identity. It also rejects reachable native server-file/program roles. Four new
PostgreSQL tests include a real self-regrant counterexample, multi-hop and masked
session cases, and harmless-membership positive controls; no OS command is run.

Downstream principals require a project when created. The domain and Store
regressions prove rejection without a stranded principal or command receipt.
The new additive `202609060010_evidence_producers.sql` binds evaluation reports
and metric sources to the exact project/run and expected artifact roles.
Approval context must be frozen, belong to the release project, and contain its
exact evaluation reports. Four PostgreSQL tests cover both valid source layouts,
wrong projects/runs/kinds, draft or unrelated approval contexts, and rollback of
an upgrade containing incompatible historical evidence. Prior migrations, the
product Cargo.lock, original test assertions and historical evidence are retained.

Local validation of this code snapshot used Rust 1.98.0, the committed Cargo.lock,
and isolated native PostgreSQL 18.1 with PGMQ 1.10.0. The complete workspace test
command passed 206 tests, zero failures and zero ignored; strict Clippy, rustfmt,
the all-target build, generated API/domain diffs, and the 204 decimal plus 242
bigint shared Node cases passed. Exact source-tree identity and published-Head
CI are recorded in the PR, not retroactively assigned to these local logs.
These source/authorization regressions do not establish the still-missing
production research, runtime-isolation, portfolio or protected-account acceptance.

## Immutable research preparation (2026-09-06)

This implementation starts from remote `a41e8709576d344919ffbe121b2619932793fe0d`
and adds typed InputSet and EvaluationPolicy HTTP/Store commands, with DESIGN A4.3
written before code. Public input metadata never contains native storage references
or sealed bytes. Metadata registration is neither algorithm execution nor capability,
PIT, PASS, qualification or deliverability proof. Complete Brief/worker/native runtime
admission remains separate work, including rechecking current data authority.

The existing OperatorCommand/receipt path authorizes and atomically publishes a
frozen input aggregate or a policy/family bound to the project's immutable lineage.
Policy versions are allocated under the project lock. Dataset role/cutoff, source
and runtime availability, current immutable grant/revocation and exact artifact
ownership are rechecked with native row locks. Revocation inserts lock the same
grant; a separate read after lock acquisition uses a fresh READ COMMITTED snapshot.
Only exact scoped machine reads are enabled; no new MachineScope or SQL backdoor.

Twenty-two new tests cover three native wire/schema cases, two pure contract cases,
twelve real PostgreSQL cases and five real Axum/native-authentication cases. The
latter use actual Cookie/Bearer, TOTP, Argon2 and SecretVault with disposable migrated
databases; the new HTTP suite uses the actual router, not a mocked API. Real lock
waits, opposite-order revocation/consumption, concurrent idempotency, versions,
sealed/WF identity rules, complete CLI-grant request binding, safe field diagnostics,
large request limits and full rollback of failed receipts are exercised.

These tests exposed an existing zero-argument `guard_revision` defect: PostgreSQL
provides NULL TG_ARGV, and FOREACH failed with SQLSTATE 22004 for otherwise legitimate
Runtime/Downstream updates. The additive 0012 migration coalesces only that native
argument array; the regression executes the unchanged original function to reproduce
22004, then verifies updates, versioning and immutable-identity/source rejection
under the replacement. Applied migrations and Cargo.lock remain untouched.

Validation results, exact source tree and the independent published-Head CI/review
are recorded in the PR, not inferred from test definitions or a historical Head.
Parallel Run/SSE/CLI and Ant Design implementation is preserved and is not claimed
as part of this preparation slice.

## Explicit gaps and completion boundary

Complete Run admission/takeover, research services and machine authorization,
remaining API/Worker/CLI/MCP, same-Thread model/tool/job/evidence integration,
independent Reviewer, PIT/sealed isolation, multi-Alpha shared-capital portfolio,
target-only approval/feedback/wake services, Ant Design UI/PWA, user-data migration,
backup/restore, protected real-account acceptance and production deployment remain
incomplete. JSON container/version checks are not complete policy validation.
Removing legacy tests is not acceptance, and foundation CI is not full W0–W8/T01–T42.

Keep PR #63 Draft until the entire Issue #62 contract is implemented and evidenced.
Completion requires the PR, all applicable CI on its latest Head passing, all
review findings resolved and an explicit no-findings Codex review of that Head;
only then merge. Verify main and required migration/isolation/recovery/end-to-end
evidence before closing #62. **On GitHub, Codex is review-only: never ask it to fix,
implement, commit or autonomously handle problems.**

## Run/Attempt 与持久 SSE 实现增量

新增 `crates/store/src/lifecycle.rs`、`crates/contracts/src/lifecycle.rs` 和
`apps/server/src/runs.rs`。复用 SQLx/PostgreSQL/PGMQ/Axum/Tokio，提供受信任域服务
准入、单 Attempt 接管/发送意图、取消/终态回执/归档，以及带认证作用域的 Run 查询
和 SSE；不是开放任意命令执行，也未把远端结果当成科学 qualification。

验证入口：

```sh
cargo test --locked -p store --test run_lifecycle
cargo test --locked -p server --test runs_http
cargo test --locked -p contracts --test lifecycle_wire
```

数据库套件验证同键并发、预算超发、PGMQ/事件故障整体回滚、lease/epoch、丢 ACK、
取消/完成竞态、锁等待后过期、回执关联与真实失败代码。HTTP套件以真实 TCP listener、
Axum middleware、原生加密/会话及 PostgreSQL 检查 cookie/Bearer、跨项目拒绝、
SSE 多批终态重放/断点续读/权限撤销/连接上限和断线不取消。共享 fixture 仅提供
领域记录，不模拟 HTTP 认证或将 FIXTURE 变为 REAL。

首次回归复现原有零参数 revision trigger 错误；新增迁移修复空 TG_ARGV。
其余已应用迁移原样保留，两个新增领域表的期望计数及生成合同同步更新。CI仍须在
推送后的精确 Head 上运行；测试定义、本地编译或本节文档均不能冒充远端 CI/Review
通过。没有在本节写入随后可能失效的当前测试总数或 release-ready 声明。

## 2026-09-06：交付边界与原生仓位审查增量

基于已入 PR 的 `0584a0c34ee931bdf7d31f64ee23c6e017ef17ed`，本轮新增
`202609060013_delivery_boundary.sql`，原有 SQLx 迁移与 Cargo.lock 不变。
覆盖 Package 精确项目/角色/版本、REAL 来源元数据、DEMO 不可审批或发出 Offer、
Research lineage 无环、Claim 行锁后 DB 实时过期检查、Forward 已转移状态与拒绝竞态。
升级检测到非法历史时原记录及全部迁移历史保持不变，不改写、删除或重新标注证据。

新增9个真实 PostgreSQL测试和1个真实Nautilus引擎测试。修复后的全workspace执行
得到238项通过，0失败、0忽略；严格Clippy与all-target构建通过。Nautilus Rust0.63.0
实际输出745 iterations、12 orders、24 events、**12 positions**；报告包含原生
position count并拒绝零仓位。它仍明确是 `FIXTURE`、`deliverable=false`、无Python。

旧审批/反馈关系测试的正向对照现在创建独立 PACKAGE 元数据和显式 REAL 关系样例，
DEMO helper始终保留FIXTURE且不能审批，反馈正例必须先真实执行Claim状态转换。
这些仅是隔离测试库里的身份/约束样例，不包含实际市场数据或声称科学资格，也没有
生产测试开关、重标现有fixture、下游真实交易或绕过产品Gate的代码。

此处结果不替代新增Head的GitHub CI和独立Review；完整产品仍按DESIGN验收。

## 2026-09-06：研究用途、Policy/Family 与字段诊断审查修复

基线为 PR Head `6191ceadb79ab2db03c2af1d0f214687200ef2d3`。
锁住的 DataUseGrant 事实现在包含封闭 allowed_uses；RESEARCH 不能登记
PORTFOLIO/FORWARD 用途，较高授权仅允许对应准备，不替代后续 Live 发布检查。
Policy 与 Family 通过原生生成的 UUIDv7 关系列、两个精确复合延迟外键一一绑定，
同事务可按任一顺序创建；不存在/错项目/错根/额外 Family 均不能永久提交。
新增 015 迁移审计历史而不改写它，保留 014 的 Brief 作者工作包序号。

指标代码、scope、方法列表及各项字符串的边界由原生 utoipa 发布到两份生成合同，
不手写第二套 schema。精确 Decimal 比较器保持不变，阈值错误现在指向真正的
缺失/多余端点，BETWEEN 倒序同时标记两端，不向客户端回显数值。

修复前对原 6191 源码实跑：6 项真实 PostgreSQL 回归、1 项原生 schema 回归、
1 项 Domain 诊断回归均复现失败。修复后新增总计 13 项回归，包括 10 项真实 PG、
1 项真实 HTTP、1 项生成 schema、1 项比较器诊断；还覆盖合法历史升级、坏历史
整体回滚、生成列不可覆盖和用途矩阵。原并发测试的 pg_stat_activity 精确查询
前缀同步新增 allowed_uses 字段，仍须观察真实 PostgreSQL 锁等待，未删除断言。

当前候选源使用原样 Cargo.lock、Rust 1.98.0、真实 PostgreSQL 18.1/PGMQ 1.10.0
执行全部 273 项 workspace/all-target 测试：0 失败、0 忽略。严格 Clippy、fmt、
全目标构建和两份原生生成合同已核对；204 Decimal、242 Bigint 共享语料通过。
本地结果不代表新提交 CI 或独立审查已通过；这些仍必须针对实际推送 Head 完成。
本增量不改变完整 W0–W8/T01–T42 验收、Draft 状态或 Codex review-only 边界。


## 2026-09-06：Run 恢复、当前许可与 Forward 来源审查修复

本地基线是远端 a52fa0fd88f44aeeee59a6d7f1bccfadf7b11fe0 的原生 tree
469e918b0eacaf193416e8d2cf133c38e1e57bdd；原样 Cargo.lock/Rust1.98。
新增016/017迁移，001–015原字节保留，014仍为Brief作者流程的预留序号。

修复前新增回归实跑复现六项故障：10ms请求超时杀掉25ms迁移DDL、过期NOT_SENT
仍被续租、终态Attempt仍可改、成功Attempt含错误码、未知兼容事件拒绝、旧浏览器
可取消。另用未安装017的真实数据库复现了跨项目Forward报告被接受。负向结果是
旧缺陷证明，不作为通过结果。原有终态重传测试改为真正等待活动期设置的短租约
过期，不再通过修改已终结Attempt安排测试；PGMQ批量读取的测试一次保留两个消息，
不在其visibility窗口内错误地二次读取。并发测试仍要求观察原生锁等待。

实现复用SQLx原生事务/锁、PGMQ、BigDecimal、utoipa和Axum SSE：新消费重新授权，
确切回执与未知结果对账不受事后撤销抹账；无Cycle只允许有界管理任务；未发送过期
保存NOT_DISPATCHED事实而非伪造远端失败；已终结Run的任何Attempt写入均拒绝。
部署连接独立取消statement_timeout而不污染请求池。政策fraction与capabilities
由原生schema生成并用214条共享精确语料检验，不重建Decimal实现。

Forward保存一次性领取元组和显式legacy来源，拒绝领取前的历史消息、凭拒绝状态
伪造领取，以及不符合精确项目/角色/schema/origin/access的报告。合法既有反馈在
随后拒绝时保留；坏历史令升级整体回滚。测试中的REAL元数据仅为可丢弃库内的
关系正例，没有市场数据/报告字节，不宣称产品验收。

验证入口：全workspace/all-target locked测试、严格Clippy、all-target构建，
contracts/example generate与server openapi原生生成后比对，两份schema的204
Decimal、242 Bigint、214 Fraction共享语料；真实PostgreSQL18.1/PGMQ1.10与HTTP。
实际通过结果绑定发布Head的CI/PR证据，不在本节冒充远端Review已通过。
完整W0–W8/T01–T42仍须继续交付，Codex仅独立review。


公有开发依赖/工具已取回到隔离开发目录，删除完成的client-development-inputs、
prepare-web-dependencies、web-development-inputs工作流；它们不是验收，不保留
随每次PR提交重新取依赖的永久开发任务。正式CI仍只读、使用已提交锁文件。

## 2026-09-06：Brief 草稿作者流程与 SSE payload 合同

本地基线为远端205bc4c0fd16b593263dd744895826a7ee5456cf的完整tree
5456cc177ceb45d0b59e1e46fed89cb3b22dd2f6。该基线独立CI34049936238成功，
但其Codex审查指出payload生成schema缺少对象/版本约束，不能将Completed当无问题。

新增真实Brief create/read/list/PATCH：完整非秘密规范意图绑定路径project_id与
schema_version，既有Operator单次CLI grant、幂等原始响应、项目锁/CAS和绑定
替换同事务。014使用预留迁移号，不改任何已提交迁移或Cargo.lock。父Brief锁将
DRAFT编辑与FROZEN成员封口串行化。运行身份只新增brief_data_bindings一张表的
DELETE权限，旧不可变历史守卫与其他表无DELETE/TRUNCATE/TRIGGER边界保持。
保存草稿不证明当前许可、PIT或原生能力，不提供假成功的freeze入口。

205的payload审查通过原生utoipa发布JSON object、必需schema_version=1及可扩展
公开属性修复；已有事件类型运行时仍严格校验，未知兼容事件不改状态投影。

新增18项Brief测试（9真实PostgreSQL、4真实HTTP/私有Cookie/TOTP/Argon2、3领域、
2线协议）和1项SSE schema测试。非owner真实部署身份可创建/替换草稿，不能删除
其他表或改冻结成员；原生PG锁等待覆盖编辑/冻结，故障注入证明绑定与CAS回滚。
旧浏览器测试通过原生SessionStore关联合法历史BrowserLogin，不倒退认证时间，
不关闭触发器或改变生产鉴权。

本地对本增量完整源码运行Rust1.98与原样Cargo.lock：313项workspace/all-target
测试通过，0失败、0忽略；fmt、严格Clippy、all-target build通过。两份OpenAPI
由实际Rust生成。发布必须再跑对应新Head独立只读CI和review；本地结果不等于
完整W0–W8/T01–T42、原生账号验收或可合并状态，Codex仅review。
