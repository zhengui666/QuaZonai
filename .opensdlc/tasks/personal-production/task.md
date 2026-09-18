# Personal production

<a id="task"></a>
## Task

Task ID: `personal-production`. The owner requested a genuinely usable personal QuaZonai across code, architecture, documentation, deployment and UX, removal of obsolete development traces, and complete delivery against [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62).

[DESIGN](../../../DESIGN.md) owns W0–W8/T01–T42. [The acceptance index](../../../docs/architecture/issue-62-execution.md#acceptance) owns remaining coverage. This entry routes continuation; PR discussions own per-turn execution logs, failures, reviews and merge evidence. Do not duplicate historical development narration in product manuals.

<a id="intent"></a>
## Intent

Provide persistent real services, recoverable failures and understandable personal operation using existing native capabilities. Retain single-user Rust, official Ant Design and target-only ownership. Remove stale narration and temporary tooling, not credentials, user data, immutable evidence, migration history or licenses. Reuse [maintenance](../production-maintenance/task.md) and [onboarding](../open-source-foundation/task.md).

<a id="spec"></a>
## Scope and boundaries

| Area | Implementation / acceptance work | Limit |
|---|---|---|
| Hosted application | Packaged Rust/dist through production Caddy routes; original TOTP/session/project/first receipt across clean API restart | Loopback is not public TLS, systemd boot or owner-host deployment |
| Browser recovery | Confirmed reload, original-key recovery, mobile layout and logout | Does not cancel or recreate an uncertain command |
| Runtime checkpoint | Complete SQLite/WAL archive with numeric ownership, retained original, new process and original inputs | Same-host/same-path terminal checkpoint, not active-job snapshot |
| Joint checkpoint | PostgreSQL, control artifacts/secrets, real Runtime and catalog restored together; original uncertain Attempt reconciled by real Worker | Explicit FIXTURE/PIT-UNVERIFIED data; no account, scientific qualification or production RPO/RTO claim |
| Documentation | Current commands, prerequisites and canonical evidence links | Written commands and listed tests are not executed results |

QZ does not own broker credentials, real orders, positions, account/NAV or downstream trading controls. Preview and historical samples cannot qualify new research. Real accounts, authorized data and owner-selected snapshots remain separate protected acceptance inputs.

<a id="plan"></a>
## Implementation and reuse

Merged baselines: [#85](https://github.com/zhengui666/QuaZonai/pull/85) (`ff8bee7`, hosting/preview), [#86](https://github.com/zhengui666/QuaZonai/pull/86) (`45033f4`, isolated cold recovery), [#87](https://github.com/zhengui666/QuaZonai/pull/87) (`e426fc3`, SQLite/SQLx and Mission stack repair), and [#88](https://github.com/zhengui666/QuaZonai/pull/88) (`08c0af7`, actual hosted restart, mixed-owner archive and observed Mission deadline). Their exact accepted checks and failed iterations remain in those PRs; they do not approve later commits.

Current [PR #89](https://github.com/zhengui666/QuaZonai/pull/89), branch `codex/native-control-restore-20260918`, starts from main `08c0af7ddb8a62375959a74cf24dad851113d017`. Reconcile actual Head before any write. The [joint-checkpoint plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5723177996) and [field-level plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5723535628) precede the implementation.

### Joint control and Runtime recovery

`apps/runtime/tests/native_control_restore.rs` reuses the actual Runtime/Docker fixture, native Parquet market fixture, Store data/authority helpers, PostgreSQL client helper and built `server worker` CLI. Test-only Store/PostgreSQL/UUID/url dependencies do not create a production Runtime→Store boundary. The original archive procedure moves to `tests/support/archive.rs`; existing cold/ownership assertions are not replaced or weakened.

Read actual catalog instrument/bars/time/count, retain FIXTURE/PIT UNVERIFIED provenance, and get metadata/capability bytes from the real Runtime. Existing Store commands freeze DATA_VALIDATE and its Attempt/spec. Submit that exact spec, observe terminal native output, but leave the control Attempt SENT_UNKNOWN with no terminal/publication/ACK. No control Worker is running when the terminal Runtime is stopped and the common checkpoint is captured.

Native pg_dump/pg_restore restore a fresh database. Whole control state, Runtime directory/config and actual catalog restore into new-inode copies at their original paths, with originals retained. Keep the master key separate; actual recover-access invalidates old browser authority. Store's controlled verified-step setup is explicitly not real TOTP/model-account acceptance.

Withhold and retain the original local parameter artifact. A real Worker must defer with no terminal/publication/ACK. Restore the same file, wait for the real database lease to expire, and start a new Worker. It must preserve the original Attempt/external identity/spec while advancing owner epoch, adopt exact raw manifest/output bytes once, then archive the original queue message. Original container/start/end/restart identity and the retained original state must stay unchanged. A distinct next DATA_VALIDATE must actually read the restored catalog and produce native output without re-uploading/re-registering the original data. Do not claim network request counts that were not observed.

The existing Native Runtime workflow adds its established PostgreSQL18/PGMQ service and built Worker. Keep the original17OCI, cold and ownership tests, limits, locked builds and unchanged-source assertions. Private database/archive/Worker logs and keys are not uploaded. Only test-owned fresh resources are cleaned after child guards end; failures must remain visible.

### Retained native behavior

Hosted recovery retains both original Playwright phases, first-response capture, same keys/database/session, normal old-process exit and a different new PID. Caddy502 during API downtime must not become a successful SPA response. No reseeding, new enrollment or Vite fallback. [CONTRIBUTING](../../../CONTRIBUTING.md#verify-the-change) and [hosting](../../../docs/user-guide.md) own the commands.

The existing archive helper retains privileged numeric-owner GNU tar only; Runtime/Cargo stay unprivileged. Metadata controls prove ownership preservation, not a scientific result. The existing memory probe still uses its original limits/deadline and requires both stopped and OOM facts. The Mission deadline test observes actual persisted database time, not a fake clock or edited reservation. Earlier ambiguous failures and the optional CodeQL database-copy correction remain in #88; no query, SARIF upload or actual product backup is waived.

<a id="verification"></a>
## Verification

At initial Head `7e8daed215021dee2556dd512d40ed1167c4b19e`, [Native Runtime35295820052](https://github.com/zhengui666/QuaZonai/actions/runs/35295820052) had one successful temporary format/native-lock job105448012165. Artifact10528501276 contains five byte/blob-verified entries, compiler build-finished=true with no diagnostics, and identical before/after check diffs. The only lock change adds the already-resolved Store and url test dependencies to Runtime. The archive helper is an exact move plus native formatting. This proves candidate compilation only; the original committed-source runtime job failed before execution.

Publish the actual formatted source and native lock, removing `.github/joint-archive.patch` and the temporary authored-source job. No permanent transfer/generator framework is added. New committed-source CI must execute the joint scenario, not reuse the temporary compile as a pass.

Use the actual final-Head CI/artifacts in #89: complete Rust/Store/Server and Web, original17OCI, cold/metadata restore and joint checkpoint, hosting and CodeQL. Inspect source identity, phase outcomes, original Attempt/fence/spec, exact bytes/publications, original queue ACK, fresh native job and owned cleanup. The new joint scenario has no executed success at this record. Update the existing recovery guide and acceptance index with the supported boundary and actual results before delivery. Missing, skipped, ignored, cancelled, failed or old-Head checks are not passes.

Independent prior database or Runtime restores cannot substitute for the new combined result. Even a passing quiescent fixture does not establish active-job power-loss recovery, cross-host relocation, real licensing/accounts, production RPO/RTO or full T42.

<a id="review"></a>
## Independent review

Inspect summary and inline findings. After every source change request fresh read-only `@codex review` for the exact final Head. Resolve actionable threads with source and executed evidence; prior clean feedback cannot override a later finding. Author inspection is not independent approval.

<a id="delivery"></a>
## Delivery boundary

1. Complete the declared PR scope and publish actual source without temporary transfer machinery.
2. Require every applicable final-Head CI to pass, all actionable findings resolved and explicit clean independent Codex review for that Head.
3. Only then mark ready and merge with expected-Head verification; inspect main and post-merge checks and record the actual merge.
4. **Never ask GitHub Codex to fix, implement, edit, commit or push.** It is the read-only reviewer; the web assistant authors changes.

Issue #62's closed metadata is not an acceptance certificate or scope waiver. Preserve all remaining contractual evidence requirements; a maintenance PR merge is not full personal-product production readiness.

<a id="handoff"></a>
## Continuation

Keep each turn's public execution summaries in the active PR, not product manuals. Read AGENTS, [CONTRIBUTING](../../../CONTRIBUTING.md) (`DEVELOPMENT.md` is absent), relevant DESIGN, OpenSDLC configuration and actual Head/checks. Preserve this English task and its ID; no language override was found.

CodexPro discovery exposed no owner-workspace action. Connected file tools and isolated native GitHub CI are available, not an observed owner-host shell or local executor. No production database, private model account, owner snapshot or deployment was accessed. Continue account/data/migration/coordinated recovery/runbook/research acceptance through the canonical index; unavailable inputs remain unverified, never fabricated passes.
