# Personal production

<a id="task"></a>
## Task

Task ID: `personal-production`. The owner requested a genuinely usable personal QuaZonai across code, architecture, documentation, deployment and UX, removal of obsolete development traces, and complete delivery against [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62).

[DESIGN](../../../DESIGN.md) owns W0–W8/T01–T42. [The acceptance index](../../../docs/architecture/issue-62-execution.md#acceptance) owns coverage; PR discussions own per-turn execution logs, failures, reviews and merge evidence. Do not duplicate those histories in product manuals.

<a id="intent"></a>
## Intent

Provide persistent services, recoverable failures and understandable personal operation using existing native components. Retain single-user Rust, official Ant Design and target-only ownership. Remove stale narration and temporary tooling, not user data, credentials, immutable evidence, migrations or licenses. Reuse [maintenance](../production-maintenance/task.md) and [onboarding](../open-source-foundation/task.md).

<a id="spec"></a>
## Scope and boundaries

| Area | Implemented or current acceptance work | Limit |
|---|---|---|
| Hosted application | Packaged Rust/dist through real Caddy routes; TOTP/session/project/first receipt across API restart | Loopback is not public TLS, systemd boot or owner-host deployment |
| Recovery | Original Runtime cold/UID cases and a joint PostgreSQL/control/Runtime/catalog checkpoint through the production Worker | Quiescent same-host/path evidence, not arbitrary rollback or the owner's measured recovery objective |
| Dependencies | Source-lock SBOM generated with pinned upstream Syft in the existing native CI job | Not binary/image inventory, complete license clearance or vulnerability scanning |
| Documentation | Current commands, ownership, prerequisites and actual evidence links | A listed command or configured check is not a pass |

QZ does not own broker credentials, real orders, positions, account/NAV or downstream trading controls. Preview samples and FIXTURE/PIT-UNVERIFIED data do not qualify real research. Missing account/data/legacy inputs remain unverified requirements.

<a id="plan"></a>
## Implementation and reuse

Merged baselines: [#85](https://github.com/zhengui666/QuaZonai/pull/85) (`ff8bee7`, hosting/preview), [#86](https://github.com/zhengui666/QuaZonai/pull/86) (`45033f4`, cold recovery), [#87](https://github.com/zhengui666/QuaZonai/pull/87) (`e426fc3`, SQLite/SQLx and Mission stack repair), [#88](https://github.com/zhengui666/QuaZonai/pull/88) (`08c0af7`, hosted restart/ownership/deadline) and [#89](https://github.com/zhengui666/QuaZonai/pull/89) (`35a1625aa7a3349fc0d3f3c384a79b6d3b3e4699`, joint recovery). Their PRs retain exact accepted checks and failures, not approval of later source.

[PR #90](https://github.com/zhengui666/QuaZonai/pull/90), branch `codex/source-dependency-inventory-20260918`, begins at main35a1625. Reconcile actual Head before writing. The [B9 plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5724780821), [minimal placement](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5724801190) and [observed coverage correction](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5725063439) precede their implementation. OpenSDLC has no language override; preserve this English task and ID.

### Source dependency evidence

Extend only the existing Rust native CI job, current third-party notices, this task and one focused [upstream reuse note](../../../docs/research/source-dependencies.md). Git exports the committed Cargo manifest/lock, actual apps/web manifest/lock and native Codex manifest/lock. Syft scans only that directory once, producing native JSON plus SPDX JSON in the existing native-evidence artifact. Preserve original input files, source commit and actual tool version.

The official download action is pinned to e22c389904149dbc22b58101806040fa8d37a610 and Syft to v1.52.0. Explicit native JavaScript development-dependency inclusion is required: metadata must confirm it, and standard jq/sort/diff compares each npm lock's versioned non-link name/version set, including project roots and aliases, with findings from that same source. This is an output-completeness assertion, not another dependency resolver or generated SBOM. No fixed package-count constant is introduced.

No application dependency, scanner policy, hosted service, new workflow/job, permission increase, release upload or dependency-snapshot submission is added. Existing source/build/database/Web/Runtime/review checks and timeouts stay unchanged. Unknown licenses are not relabeled as safe; optional/development/platform entries and Git source metadata remain distinguishable. A source inventory is not a deployed-binary or image inventory. The selected notice table is only integration/attribution navigation, with cap-std and both resolved Arrow versions corrected against the original lock.

### Retained recovery behavior

The joint target uses real Parquet and Runtime output while the original control Attempt remains SENT_UNKNOWN, with no publication/ACK. Stop all relevant writers, restore a fresh PostgreSQL database and same-path new-inode control/Runtime/catalog copies with originals retained and master key separate, then use actual recover-access. A real Worker must defer while an original local parameter is withheld, and a new owner must adopt original bytes once after that same file returns and the actual lease expires. Original Attempt/spec/container/queue identity is preserved; a distinct new task needs fresh capabilities and reads the restored catalog. Trusted Store authentication setup is not real model/TOTP acceptance. The [runbook](../../../docs/runtime-recovery.md) owns procedure and limits.

Hosted tests retain both real Playwright phases, first-response capture, the same keys/database/session, normal old-process exit and a new PID. Caddy502 is not replaced by an SPA success. No reseeding, Vite fallback or re-enrollment. Native cold/ownership controls, bounded resource observation and actual database deadline checks remain unchanged. No temporary source-materialization machinery survives in the delivered tree.

<a id="verification"></a>
## Verification

PR #89's final2236a0f97aa69d891d2de57422616aece30513b0 passed all five applicable workflows and explicit independent review5724323156 with both findings resolved. Runtime artifact10529466181 executed17OCI+2cold/ownership+1joint cases; Store artifact10529831989 has681 passes; Web artifact10529411623 passed both real hosted phases. The expected-Head merge35a1625 has no source diff. [The delivery record](https://github.com/zhengui666/QuaZonai/pull/89#issuecomment-5724912215) confirms all five post-merge workflows separately succeeded.

PR #90's initial934bc9e actually generated both formats in CI35304319663. Artifact10530763734 matched all six original input files, and all641Cargo identities/sources were present. However, its native metadata explicitly disabled JavaScript development dependencies: only75frontend records including the root appeared instead of524, omitting449. The Codex lock's8records were complete. The earlier nonempty/three-source checks passed despite that defect. The documentation-only1061 revision retained the same configuration; neither result is a complete source inventory. This observed artifact led to the native option and record-set checks above, not a narrowed scope or handwritten repair of generated output.

Require the corrected final Head's actual generated output, effective option, all three lockfile sources, exact retained input bytes and complete npm comparisons. The original missed449records are pre-repair evidence, not a hardcoded future count. Inspect unknown licenses and raw Git source metadata rather than claiming SPDX conversion proves license clearance. Failed download/scan/parse/comparison is a failed native job. All existing CI and a fresh independent review remain applicable; older green source does not approve the correction.

Public/owner deployment, protected account/data research, legacy migration and complete T40/T41/T42 acceptance require their original evidence. Neither maintenance nor an SBOM fills those requirements automatically.

<a id="review"></a>
## Independent review

Inspect all summary and inline findings. After every source change request fresh read-only `@codex review` for the exact final Head. Resolve actionable findings with source and executed evidence; author inspection and older clean feedback cannot substitute for independent current approval.

<a id="delivery"></a>
## Delivery boundary

1. Complete the declared PR scope with committed source and no temporary transfer machinery.
2. Require every applicable final-Head CI success, all actionable findings resolved and explicit clean independent Codex feedback for that Head.
3. Only then mark ready and merge with expected-Head verification; inspect main/source identity and post-merge checks, recording the actual merge.
4. **Never ask GitHub Codex to fix, implement, edit, commit or push.** It reviews only; the web assistant authors changes.

Issue #62's closed metadata is not an acceptance certificate or scope waiver. Do not enlarge the contract by treating every test's stated limitation as an additional product feature, or narrow it by calling missing protected evidence a pass.

<a id="handoff"></a>
## Continuation

Keep each turn's public execution summary in the active PR. Read AGENTS, [CONTRIBUTING](../../../CONTRIBUTING.md) (`DEVELOPMENT.md` is absent), relevant DESIGN, OpenSDLC configuration and actual checks before continuing. Protect concurrent changes and user data.

CodexPro discovery did not expose an owner-workspace action in this session. Connected file tools and existing isolated GitHub CI are available; no local executor, owner-host shell, production database, private model account or user snapshot has been accessed. Continue the full contract through the canonical acceptance index without fabricating absent results.
