from pathlib import Path
exec(Path('.github/personal-lean.py').read_text().split('# Compile one immutable')[0])
replace('apps/server/tests/auth_http.rs', 'use contracts::Id;\n', '')
write('apps/server/tests/budget_errors.rs', read('apps/server/tests/budget_errors.rs').rstrip()+'\n')
replace('apps/server/src/main.rs', '/// Migrate a new database using a separate privileged migration identity.', '/// Explicitly migrate the database; optional grants support a separate runtime role.')
replace('apps/server/src/main.rs', 'Domain and native session migrations completed. Run serve with the non-owner application identity.', 'Domain and native session migrations completed.')

write('README.md', '''# QuaZonai

**个人、本机、自托管的量化研究工作台。** Rust 负责科学计算，Codex 组织研究，React / Ant Design 提供 Web / PWA。输出可追溯的 Alpha、组合回测与 target-only 目标包；不持有券商凭据、不发送真实订单。

[部署](docs/user-guide.md) · [运行手册](OPERATIONS.md) · [Agent 操作](docs/agent-operations.md) · [CLI](CLI.md) · [架构](docs/architecture.md)

[![CI](https://github.com/zhengui666/QuaZonai/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/zhengui666/QuaZonai/actions/workflows/ci.yml)

> 当前是开发版本，完整生产验收尚未完成。[验收证据](docs/architecture/issue-62-execution.md#acceptance)区分真实执行、合成测试与未验收项；专用账号实测已由所有者豁免，并不等于执行通过。

## 本机使用

网页直接进入工作台，不需要账号、验证码或设备信任。API 与网页仅监听 loopback，不提供公网免登录模式。数据库可由同一所有者账号迁移和运行；独立低权限角色是可选部署方式，不是启动条件。

Codex 自动沿用同一 OS 用户的 `PATH`、`HOME` / `CODEX_HOME` 和原生配置。先在该用户终端执行 `codex login`，设置页只选择模型与推理强度；启用“本机默认”即不覆盖原生设置。右上角切换浅色／深色主题。

正式部署需要 Linux x86_64、Rust 1.98.1、PostgreSQL / PGMQ；前端使用 Node.js ≥22.12。按[个人部署指南](docs/user-guide.md)构建、初始化独立状态目录、显式迁移并启动 API / Worker。已有数据先备份，不通过删除状态目录解决启动问题。

<a id="quickstart"></a>
## 无凭据界面预览

```sh
git clone https://github.com/zhengui666/QuaZonai.git
cd QuaZonai
make demo-preview
```

打开 <http://127.0.0.1:4179>。预览标注 **SYNTHETIC / FIXTURE**，使用内存数据；重启清空，不执行真实模型、科学计算或交付。`Ctrl+C` 停止。正式部署不要使用 `vite preview` 或演示数据代替服务。

## 结构与文档

Web / CLI / MCP → Rust API → PostgreSQL / PGMQ → Worker → Codex / 独立 Runtime。科学任务复用 NautilusTrader、Wasmi、Clarabel、Arrow 与 ndarray；QZ 保留研究规则和证据关联，不重写撮合、优化器或 Agent 工具循环。

| 需要 | 入口 |
| --- | --- |
| 部署、配置、备份与恢复 | [个人部署](docs/user-guide.md)、[OPERATIONS](OPERATIONS.md) |
| 服务 Agent 与命令接口 | [Agent 操作](docs/agent-operations.md)、[CLI](CLI.md)、`server --help` |
| 字段、状态机与模块边界 | [DESIGN](DESIGN.md)、[架构](docs/architecture.md) |
| 开发与验证 | [CONTRIBUTING](CONTRIBUTING.md)、[AGENTS](AGENTS.md) |

## 开发

按[贡献指南](CONTRIBUTING.md#verify-the-change)选择与改动相关的检查。CI 保留真实数据库、原生科学计算、接口生成与浏览器回归；不再生成每次提交的双份 SBOM 或运行专属 CodeQL 扫描。编译复用的手动微基准：

```sh
cargo run --locked --release -p job --example benchmark_signals
```

基准同时核对结果与 fuel；计时不设置通过阈值，也不代表完整研究任务的提速倍数。

## License

原创代码：[AGPL-3.0-only](LICENSE)。第三方许可与版权：[NOTICE](NOTICE)、[THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES.md)。
''')
write('AGENTS.md', '''# QuaZonai Agent 治理

## 事实源与顺序

`DESIGN.md` 定义产品、领域与接口；最新[单人精简修订](DESIGN.md#personal-lean)覆盖旧部署门槛。`OPERATIONS.md` / `CLI.md` 展开实际操作，`docs/architecture.md` 导航源码。`skills/quazonai/SKILL.md` 仅供操作运行中服务的 Agent 使用，不是开发文档。

## 所有者修订与实现纪律

优先复用现有代码、标准库和成熟原生组件，Rust 优先；Python 例外须在 `docs/research/reuse.md` 记录具名缺口。前端使用 React / TypeScript / 官方 Ant Design。不重建数值优化、撮合、Agent Harness、队列、认证算法或容器平台；不新增无实际需求的工厂、服务层、hash 身份或门禁。

删除失去用途的实现及专属测试，不在 legacy/archives 保存副本。Git 保留历史；用户数据、旧迁移、备份和许可证不是清理目标。保留并发开发的有效改动。

## 不可越过的边界

QZ 只研究和交付目标，不操作真实订单或券商账户。研究 Agent 的数据/预算/任务边界和独立评估仍有效，不能用单人模式让它接触 Sealed 标签、凭据或任意宿主路径。冻结版本、账本、回执、恢复和防重复执行保护的是结果正确性，不是多用户管理负担。取消必须以实际远端结果为准；Demo 不得变成真实证据。只记录可观察调用和结果，不索取模型隐藏推理。

## 开发权限

网页 ChatGPT 亲自编写源码、测试、配置、脚本与文档；GitHub Actions 执行原生构建、格式化、生成和验证。不使用 CodexPro / 本机 Codex。GitHub Codex 只做只读 review，不得实现、修复、提交或推送。普通 PR 不使用生产秘密，不操作既有数据库、服务或无关进程。

## 工作顺序与验证

读取相关合同与调用链 → 更新受影响事实源 → 最小实现 → 最窄有效测试及跨边界验证 → 独立 review。一个任务只需 `.opensdlc/tasks/<task-id>/task.md` 记录目标、改动、真实验证和剩余项；不要复制多份计划、日报或批准账本。测试说明它保护的失败模式，不用 mock 替代必要的原生验证。

## 交付与 Issue #62 完成边界

新改动使用新 PR；声明范围完成、当前 Head 的适用 CI 通过、review 线程解决且 `@codex review` 明确无问题后合并。未回复、仅 emoji、取消、缺失和应执行却跳过都不是通过。历史 Head 结果不能证明新代码。维护 PR 不自动关闭 Issue #62；真实账号豁免是 NOT_RUN，其他产品验收见 [DESIGN 0.4](DESIGN.md#acceptance-scope) 和[实现证据](docs/architecture/issue-62-execution.md)。
''')
write('.opensdlc/project.md', '''# QuaZonai project context

<a id="purpose"></a>
## Purpose and architecture
Single-user local research and target-only delivery. Read [architecture](../docs/architecture.md) for module boundaries and [DESIGN](../DESIGN.md) for contracts.

<a id="commands"></a>
## Working commands
Use [CONTRIBUTING](../CONTRIBUTING.md#verify-the-change), the root Makefile and actual native CLI help. Do not maintain another command table here.

<a id="conventions"></a>
## Conventions
Reuse native components; keep one contract source. Preserve data, licenses and immutable migration history. Remove obsolete source and its dedicated tests rather than archiving them. See [AGENTS](../AGENTS.md).

<a id="owners"></a>
## Responsibilities
The repository owner is the sole user and requirements authority. Web ChatGPT authors; GitHub Actions executes; GitHub Codex reviews read-only. Product Mission/evaluator/downstream boundaries are not extra human users.

<a id="sources"></a>
## Sources
Product: DESIGN. Operation: OPERATIONS / CLI. Source navigation: docs/architecture.md. Task intent and actual results: `.opensdlc/tasks/<task-id>/task.md`. Current delivery policy: [review](review.md); maintenance response: [operations](operations.md).

<a id="native"></a>
## Native integration points
Existing GitHub Issues, PRs, Actions and review comments hold delivery evidence. The operational Skill is [skills/quazonai](../skills/quazonai/SKILL.md), not a development harness.
''')
write('.opensdlc/review.md', '''# Project review policy

<a id="scope"></a>
## Scope
Follow [AGENTS](../AGENTS.md) and the current [owner amendment](../DESIGN.md#personal-lean). One task record and one PR are sufficient; no separate approval ledger or review calendar.

<a id="focus"></a>
## Focus
Check changed behavior, data flow, native reuse, numerical validity, recovery and unintended scope. Deletions must remove their obsolete callers and docs without deleting useful regression coverage.

<a id="severity"></a>
## Severity
Prioritize data loss, incorrect results and unauthorized external effects; then broken workflows and performance. Cosmetic suggestions are not new product requirements.

<a id="approval"></a>
## Approval
GitHub Codex performs read-only review. Web ChatGPT addresses findings. Require current-Head applicable CI and explicit clean review before merging; missing evidence is not success. This is the owner's delivery procedure, not a claim about native branch protection.

<a id="quality"></a>
## Finding quality
Each finding identifies a changed path, reproducible failure and minimal correction. Distinguish actual execution, fixtures and untested assumptions. Do not create new gates merely to satisfy this document.
''')
write('.opensdlc/operations.md', '''# Engineering operations

Production procedures live in [OPERATIONS](../OPERATIONS.md) and the [hosting guide](../docs/user-guide.md).

<a id="controls"></a>
## Action boundaries
Modify only task-owned source. Preserve user data, credentials, migrations, licenses and unrelated changes. Normal CI has no production secrets; deployment and account operations are separate actions.

<a id="delivery"></a>
## Delivery and recovery
Use a new branch and PR, actual [checks](../CONTRIBUTING.md#verify-the-change), and the [review policy](review.md). Record current-Head evidence once in the task/PR. Revert a source change with Git; never erase data to simulate rollback.

<a id="observe"></a>
## Observe and respond
Read failed Actions logs, identify the narrow failing contract, fix source and rerun affected checks. Superseded branch runs may be cancelled; the newest Head still requires its own results. No extra polling service or compliance inventory is required.

<a id="metrics"></a>
## Measures
Report actual failures, command outcomes and measured timings with their workload/profile. Do not infer production readiness or whole-system speedups from a smoke test or microbenchmark.
''')
p='DESIGN.md'
anchor='## 0. 所有者修订：语言与复用的决策顺序'
replace(p, anchor, '''<a id="personal-lean"></a>
## 0.6 单人本机精简与性能

本项目仅有所有者一名人类用户。删除不承担当前功能的文档、测试、CI 与安全管理代码，不用企业式管理流程约束本机使用。本节覆盖下文旧部署/验证门槛；不改变研究和资金结果的正确性合同。

API/Worker 可直接使用数据库所有者连接，不扫描角色委派闭包、ACL 或强制拆分迁移/运行账号。`migrate --application-role` 仅供保留分离角色的部署选用。显式迁移、事务、不可变记录、备份与恢复保持；已执行 SQLx 迁移不能改写或删除。

机器请求不再创建或更新 PostgreSQL 失败次数窗口，也不因历史失败把所有者锁在门外。原生 Bearer 校验、精确作用域、撤销、到期和独立的两槽计算上限保留。loopback / Origin 保护本机免登录服务；研究沙箱、Sealed 数据隔离和预算保护科学结果及本机资源，不能当作多用户功能删除。

Wasmi 的编译 Module 在一次任务内复用，每个品种/独立折/不连续块仍创建独立 Store、memory、globals 和 fuel。组合成员共享同一截止时点的已选目录，不跨任务/截止时点缓存数据。ndarray::dot 保持原模型身份，消除之前的成员深复制和结果副本。关闭非默认的 Wasmi extra-checks，保留原生 validate、确定性及执行资源限制。

CI 不运行专属 CodeQL 或双格式 SBOM 生成，不保留只汇总其他结果的额外 job。PGMQ 合同在真实 Store 数据库 job 内执行一次；同一分支新提交可取消旧运行。锁定依赖、许可证、真实数据库/科学/接口/浏览器/恢复回归和当前 Head 独立 review 保留。数值相等/fuel/隔离是回归条件；计时只有观测值，不设易波动的速度门槛。

'''+anchor)
replace(p, '''运行数据库身份检查包含 PostgreSQL 原生 ADMIN OPTION 委派闭包（即使 INHERIT/SET
暂为 false），并拒绝可达的服务器文件读写/程序执行预定义角色，不能仅核对
`rolsuper` 或单张表的 ACL。只有成员身份但无 INHERIT/SET/ADMIN 的边不产生权限。''', '运行数据库身份由本机所有者选择；不实施角色权限扫描或强制分离账号。独立低权限角色仍可通过迁移命令的可选授权参数使用。')
s=read(p)
s=re.sub(r'^机器认证限流复用PostgreSQL原生原子窗口[^\n]*', '机器凭据复用原生 Argon2 校验和独立两槽计算上限；不维护失败次数窗口或持久锁定。旧 machine_auth_rate_windows 仅为不可改写的迁移历史，不被当前请求路径消费。', s, flags=re.M)
s=s.replace('`server worker` 使用现有非owner应用角色、', '`server worker` 使用所有者选择的数据库账号、')
a=s.index('## B9. Required checks'); b=s.index('## B10.',a)
s=s[:a]+'''## B9. Required checks 与验收

检查集中在当前源码的实际失败模式：Rust 格式/Clippy/locked build 与测试、真实 PostgreSQL/PGMQ 事务和恢复、生成接口一致性、前端构建与浏览器/PWA、原生 Codex/Runtime/科学计算及文档链接。配置和命令以 `.github/workflows` 与 Makefile 为准，不在这里复制另一张门禁清单。

普通 PR 不使用生产秘密。删除功能时删除其专属测试，不删除数值、数据完整性或恢复回归来掩盖失败。缺失、取消和应执行却跳过不能算当前 Head 通过。历史代码扫描/SBOM 不是当前交付条件；依赖锁与许可证义务仍保留。当前 Head 适用检查和只读 review 按 AGENTS 执行。

真实账号实测按[第0.4节](#acceptance-scope)豁免且记为 NOT_RUN；其余完整产品验收仍独立记录。局部 CI 或性能微基准不证明真实研究链或生产部署通过。

'''+s[b:]
write(p,s)
p='OPERATIONS.md'
replace(p, '由原生 PostgreSQL 管理工具创建不带超级用户、创建数据库、创建角色权限的应用登录角色，密码通过交互或受保护配置输入；迁移身份与应用身份分开。', '数据库账号由本机所有者选择，可同时用于迁移与运行；独立低权限账号是可选方案。密码只通过交互或受保护配置输入。')
replace(p, '`migrate --application-role NAME` 通过 SQLx 和 tower-sessions 原生迁移创建域表及会话存储，授权应用 DML；`serve` 不执行迁移，并拒绝高权限/owner 数据库连接。', '`migrate` 通过 SQLx 和 tower-sessions 原生迁移创建域表及会话存储；仅当使用独立运行角色时提供 `--application-role NAME`。`serve` 不执行迁移，也不因连接拥有数据库而拒绝启动。')
s=read(p); s=re.sub(r'^机器 capability 的原生 Argon2 校验前[^\n]*', '机器 capability 仍由原生 Argon2 校验，使用独立两槽限制计算并发；槽满返回 CRYPTO_BUSY。没有数据库失败次数窗口、全局尝试计数或历史失败锁定。错误 Bearer 始终拒绝，不回退成本机会话；研究预算超限仍单独处理。',s,flags=re.M)
s=s.replace('与API相同的非owner应用账号', '与API相同的数据库账号'); write(p,s)
p='CLI.md'
replace(p, '不得使用数据库所有者账号启动。', '可使用同一数据库所有者账号启动。')
s=read(p); a=s.index('# DATABASE_URL 此时是独立的新库迁移身份。'); b=s.index('# PUBLIC_URL 必须是实际同源 HTTPS 入口。',a)
s=s[:a]+'''# DATABASE_URL 指向已准备的本机数据库，迁移与服务可使用同一账号。
cargo run --locked -p server -- migrate
# 可选：使用独立运行角色时，先创建角色再执行 migrate --application-role NAME。

'''+s[b:]
s=s.replace('默认启动拒绝具有 schema CREATE、表 TRUNCATE 或超级用户权限的应用角色。', '不强制拆分迁移与运行角色；数据库账号由本机所有者选择。')
s=s.replace('Origin 或数据库角色校验', 'Origin 校验')
s=s.replace('429须区分错误码：AUTH_RATE_LIMITED按原生Retry-After等待；BUDGET_EXHAUSTED的', '429须区分错误码：CRYPTO_BUSY表示当前原生验证槽已满，可稍后重试；BUDGET_EXHAUSTED的')
s=s.replace('并另测非 owner 角色与 loopback TCP', '并验证 owner 数据库连接与 loopback TCP'); write(p,s)
p='docs/user-guide.md'
s=read(p).replace('an application database identity distinct from the migration owner', 'a database account chosen by the local owner (a separate runtime role is optional)').replace('for the database roles, explicit migration', 'for database access, explicit migration').replace('with the non-owner application identity', 'with the chosen database identity; the migration owner is supported, or optionally use a separate runtime role,')
write(p,s)
p='docs/research/reuse.md'
replace(p, '''明确 superuser 绕过权限：运行服务必须使用非owner/non-superuser角色；migration
在独立本机运维命令中执行，不能每次服务器启动自动以管理员建表。''', '''说明 superuser 的原生权限；本机所有者可自行选择数据库角色，不再由应用扫描或拒绝。
migration 仍由显式运维命令执行，不在服务器启动时自动建表。''')
s=read(p); a=s.index('## PostgreSQL ADMIN OPTION'); b=s.index('## Run 生命周期',a)
keep=s[s.index('新证据绑定复用 PostgreSQL',a):s.index('[PostgreSQL18预定义角色]',a)].strip()
s=s[:a]+'## PostgreSQL 证据来源绑定\n\n'+keep+'\n\n'+s[b:]; write(p,s)
write(p,read(p)+'''
## Task-local computation reuse

Use the already pinned [Wasmi 2.0.0](https://docs.rs/wasmi/2.0.0/wasmi/) Engine/Module for compiled code and a new Store for every instrument/fold/block. No global cache or content hash is introduced. The optional extra-checks feature duplicates executor checks and is disabled by default upstream; disabling it here does not remove module validation, deterministic execution, memory limits or fuel.

Use existing [ndarray 0.17.1](https://docs.rs/ndarray/0.17.1/ndarray/) borrowed slices before the unchanged dot operation and consume its owned result buffer rather than cloning it. Portfolio preparation borrows its already selected immutable catalog for all members. No BLAS, Rayon, JIT, Python bridge or new scientific dependency is needed for these changes. Parallelizing folds is deliberately not mixed into this change because fuel is one ordered task budget.

The native benchmark example compares compile-per-instance with compile-once, includes first compilation in both measurements and checks identical predictions and fuel. Its workload is a tiny synthetic Wasm model, not an end-to-end performance claim. Native run logs, not upstream overhead estimates, are the measurement source.
''')
p='THIRD_PARTY_NOTICES.md'
s=read(p).replace('The generated source inventory below supports that work but does not replace it.', 'The committed lockfiles support that work but do not replace it.')
a=s.index('## Generated source inventory'); b=s.index('## Native PostgreSQL',a)
s=s[:a]+'''## Dependency inventory

The committed Cargo/npm lockfiles identify resolved dependency versions. Use
`cargo tree --locked` and the relevant npm lockfile to inspect a build's graph;
verify upstream license texts and redistribution obligations before distribution.
CI no longer generates duplicate Syft/SPDX inventories on every change. Existing
license notices remain applicable; optional/development dependencies in a lockfile
are not proof that they are linked into a particular executable.

'''+s[b:]; write(p,s)
for q in ROOT.rglob('*.md'):
    if '.git' in q.parts: continue
    s=q.read_text(); old=s
    for removed in ['.github/CODEOWNERS', '.github/workflows/codeql.yml', 'docs/research/source-dependencies.md']:
        pattern=r'\]\((?:\.\./)*'+re.escape(removed)+r'\)'
        s=re.sub(pattern, ']('+ 'https://github.com/zhengui666/QuaZonai/blob/57878e5fe1cf012582c0f038466bc6fbe1ee1ca0/'+removed+')', s)
    if s!=old: q.write_text(s)
write('.opensdlc/tasks/personal-lean-20260922/task.md', '''# Personal simplification and native computation

## Intent
The sole owner requested removal/compression of unnecessary documentation, tests, CI, code and security gates, plus performance optimization through native reuse. Base: main 57878e5fe1cf012582c0f038466bc6fbe1ee1ca0. Scope is source maintenance, not deployment or closing Issue #62.

## Implementation
Task-local Wasmi Module reuse with fresh mutable stores; one selected catalog shared by portfolio members; borrowed ensemble members with unchanged ndarray::dot identity; consume owned result buffers. Remove optional extra-checks, not Wasmi validation or resource budgets.

Delete runtime role/ACL scanner and its dedicated tests. Allow the database owner in API/Worker; optional separate-role migration grants remain. Remove distributed machine-auth rate windows from request paths and their dedicated tests. Retain token verification, scopes/revocation, bounded Argon2, loopback/Origin, scientific isolation, idempotency and data recovery. Existing migrations and stored data are unchanged.

Remove CodeQL workflow, per-change dual SBOM generation and redundant CI aggregation; run the real PGMQ contract once in the Store job. Cancel superseded branch runs. Compress duplicated README/Agent/OpenSDLC guidance; use existing tempfile instead of a custom test-directory allocator. Keep license notices and useful native regressions.

## Verification
The shared-module regression covers globals, linear memory, failure isolation, exact predictions/fuel and invalid budgets. Existing native forecast/validation/portfolio, HTTP, database and recovery tests remain applicable. The owner-connection TCP regression and repeated invalid-token test replace removed policy assertions.

Run native cargo fmt, Clippy and all applicable workflows on the candidate Head. Benchmark: `cargo run --locked --release -p job --example benchmark_signals`; CI also records its debug-profile output without timing thresholds. Actual command outcomes, profile, CI and review references belong in the PR. Source edits and configured checks are not evidence of successful execution.

## Delivery
Web ChatGPT authors all changes; GitHub Actions executes native formatting/verification. No Codex implementation, user-host modification, production credentials, migration execution or deployment. Merge only after current-Head applicable CI and explicit clean read-only Codex review. Whole-product acceptance and whole-workload speedups are not claimed.
''')
