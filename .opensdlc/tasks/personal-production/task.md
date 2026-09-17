# Personal production

<a id="task"></a>
## Task

Task ID: `personal-production`. The owner requested a genuinely usable personal QuaZonai across code, architecture, documentation, deployment and UX, removal of obsolete development traces, and complete delivery against [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62).

[DESIGN](../../../DESIGN.md) owns the full W0–W8/T01–T42 contract. [The acceptance index](../../../docs/architecture/issue-62-execution.md#acceptance) owns remaining coverage. This entry routes continuation; PR discussions own execution logs, failures, review and merge evidence. Historical source is retained by Git, not duplicated here.

<a id="intent"></a>
## Intent

Provide persistent real services, recoverable failures and understandable personal operation using existing native capabilities. Retain single-user Rust, official Ant Design and target-only ownership. Remove stale narration and temporary tooling, not credentials, user data, immutable evidence, migration history or licenses. Reuse [maintenance](../production-maintenance/task.md) and [onboarding](../open-source-foundation/task.md).

<a id="spec"></a>
## Scope and boundaries

| Area | Current implementation / acceptance work | Limit |
|---|---|---|
| Hosted application | Native Caddy, existing static policy, actual Rust API and production web assets; original TOTP/session/project/receipt across a clean API restart | Loopback evidence does not prove public TLS, systemd boot or an owner-host deployment |
| Browser recovery | Confirmed root reload, honest lost-ACK recovery, original-key replay, mobile layout and logout | Does not cancel or recreate an uncertain submitted command |
| Runtime recovery | Original journal/OCI fixture; full stopped SQLite/WAL archive, numeric ownership, retained original, new Runtime process and restored inputs | Same-host/same-path terminal checkpoint, not active-job or coordinated database/Runtime restoration |
| Native observation | Original resource limits and deadlines, waiting for required stopped/OOM facts; bounded native diagnostics | Observed facts only, no manufactured success or new production scheduler |
| Documentation | Current commands, prerequisites and canonical evidence links | A written command or listed test is not execution evidence |

QZ does not own broker credentials, real orders, positions, account/NAV or downstream trading controls. The synthetic preview and historical samples cannot qualify new research. Real accounts, authorized data and selected legacy snapshots remain separate protected acceptance inputs.

<a id="plan"></a>
## Implementation and reuse

Merged baselines are [#85](https://github.com/zhengui666/QuaZonai/pull/85) (`ff8bee7` hosting/preview), [#86](https://github.com/zhengui666/QuaZonai/pull/86) (`45033f4` isolated cold recovery) and [#87](https://github.com/zhengui666/QuaZonai/pull/87) (`e426fc3` SQLite/SQLx and Mission stack repair). Their failed iterations and exact accepted results remain in those PRs; they do not approve later commits.

[PR #88](https://github.com/zhengui666/QuaZonai/pull/88), branch `codex/native-hosted-restart-20260917`, continues from e426fc3. Reconcile its actual Head before publication and preserve concurrent work. Plans precede implementation: [hosted restart](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5715833744), [original receipt and native diagnostics](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5716320522), [archive ownership](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5717364040), and [real Mission deadline observation](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5717992629).

The existing `native-browser.mjs` copies the real binary/dist and runs the actual `deploy/Caddyfile`; only loopback certificate issuance and the administrator listener are disabled. Two native Playwright phases retain original enrollment/lost-ACK/CSRF/viewport coverage, privately save original storageState and the first committed receipt, then verify them after a new API PID with unchanged database/state/keys. Caddy must return502 for the stopped API while serving the real shell. No reseeding, re-enrollment, Vite fallback or test-control HTTP server. Existing cleanup fault regressions remain. Commands and prerequisites are in [CONTRIBUTING](../../../CONTRIBUTING.md#verify-the-change) and [the hosting guide](../../../docs/user-guide.md).

Native recovery reuses GNU tar rather than a backup framework. Whole stopped state can contain container-owned files until terminal cache cleanup; preserve numeric UID/GID and modes using privileged archive operations only. The Runtime remains unprivileged. An independently named metadata control demonstrates the old unprivileged ownership loss and the corrected round-trip without inventing a scientific result. Keep every existing WAL, identity, cancellation, original-byte and native-compile assertion. [The recovery runbook](../../../docs/runtime-recovery.md) owns the operator procedure.

The existing memory probe keeps its container and15-second deadline until both stopped and OOMKilled are observed; Docker processes these events separately. It retains finite terminal/resource scalars and the original failure/cleanup behavior. This avoids prematurely discarding required evidence; it is not a claim that every prior OOM failure has one proven cause.

The existing lost-send-ACK Mission regression must reach its intended checkpoint before expiry. Its original one-second request deadline could elapse inside the real preparation transaction, correctly triggering `Invalid("turn_deadline")` before the assertions. The test now allows ten seconds for setup, capped by the original Run deadline; this changes only its input, not a production TTL or test/CI timeout. It checks the persisted deadline and live database time after the first unresolved poll, verifies no early cancellation, waits with native PostgreSQL `pg_sleep_until` without domain row locks, then requires actual expiry and cancellation intent at or after that same deadline. The reservation remains identical, with no model request, fabricated native terminal or usage receipt. There is no retry loop, simulated clock, disabled trigger, edited immutable row or production admission change.

All temporary patch/materialization files and jobs are removed before acceptance. Keep original locked dependencies, generated contracts, production policies, resource bounds, test assertions and timeouts. No new persistent runner or compatibility layer.

<a id="verification"></a>
## Verification

Use the actual final-Head CI and artifacts linked in [PR #88](https://github.com/zhengui666/QuaZonai/pull/88), not a duplicate rolling result table. Required checks include the complete Web suite and both real hosted phases, original database cleanup faults, full Rust/Store/Server, native OCI and both cold/metadata restore cases, hosting and CodeQL. Inspect stage exits, original/new PID, actual engine results and screenshots. Absent prerequisites, ignored cases, cancelled jobs or old-Head checks are not passes.

Important unresolved provenance is explicit: #87's post-merge Native Runtime35231103914 passed17OCI but failed a generic tar assertion; the specific command was not captured. #88's initial35234274809 instead observed OOMKilled=false and did not reach cold restore. b2c2306's [35242181962](https://github.com/zhengui666/QuaZonai/actions/runs/35242181962) actually passed17OCI+1cold restore with stage diagnostics, but did not reproduce the older tar failure. The mixed-owner defect is established by source/native semantics and the separate deterministic control, not retroactively attributed as the uniquely captured cause of the older failure.

At `2855317578d286e1222462d8b5934cd9a333503a`, Web35244708746 and Native Runtime35244708892 passed, but [CI35244708806](https://github.com/zhengui666/QuaZonai/actions/runs/35244708806) failed in the Mission setup described above: artifact10507877172 records seventeen Mission passes and one `Invalid("turn_deadline")` failure. Later Server/Store tests did not all run. The corrected test was authored from that exact source; native Git/rustfmt transfer artifact10508875405 was inspected against its complete original and intended diff before publication. That transfer is not test execution. Fresh committed-source CI and review are still required.

The [original-receipt finding](https://github.com/zhengui666/QuaZonai/pull/88#discussion_r4038108076) requires first-response capture (`replayed=false`) and comparison of both later replies against it. A replay compared only with another replay is insufficient. Source, artifact inspection and independent review have separate roles; none substitutes for account/data-dependent T07/T08/T39/T40/T41/T42 acceptance.

<a id="review"></a>
## Independent review

Inspect summary and inline findings. After every source change request fresh read-only `@codex review` for the exact final Head. Resolve actionable threads with source and executed evidence; prior clean feedback cannot override a later finding. Author inspection is not independent approval.

<a id="delivery"></a>
## Delivery boundary

1. Complete the declared PR scope and publish actual source without temporary transfer machinery.
2. Require every applicable final-Head CI to pass, all actionable findings resolved and explicit clean independent Codex review for that Head.
3. Only then mark ready and merge with expected-Head verification; inspect main source and post-merge checks and record the actual merge.
4. **Never ask GitHub Codex to fix, implement, edit, commit or push.** It is the read-only reviewer; the web assistant authors changes.

Issue #62's closed metadata is not a product-acceptance certificate or scope waiver. Preserve all remaining contractual evidence requirements; do not declare the personal product production-ready merely because a scoped maintenance PR merged.

<a id="handoff"></a>
## Continuation

Keep per-turn public execution summaries in the active PR, not product manuals. Read AGENTS, the development entry [CONTRIBUTING](../../../CONTRIBUTING.md) (`DEVELOPMENT.md` is absent), relevant DESIGN sections and actual Head/checks before continuing. Preserve the existing English task and its ID.

CodexPro discovery in this session exposed no owner-workspace action. Connected file tools and isolated native GitHub CI were used, not owner-host shell or a local executor. No production database, private model account, owner snapshot or production deployment was accessed. Continue the complete research/account/data/migration/coordinated recovery/runbook acceptance through the canonical index; unavailable inputs remain unverified, never fabricated passes.
