# Personal production

<a id="task"></a>
## Task

Task ID: `personal-production`. Deliver the owner's personal QuaZonai requirements against [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62), using [DESIGN](../../../DESIGN.md) and its [authorized acceptance scope](../../../DESIGN.md#acceptance-scope). Code, architecture, documentation, deployment and UX remain in scope. The active PR owns per-turn execution logs, failed iterations and delivery evidence; product manuals do not duplicate those histories.

<a id="intent"></a>
## Intent

The web assistant authors through connected file tools; GitHub Actions runs native checks and generators. Do not use CodexPro or a local coding agent. GitHub Codex is an independent read-only reviewer. Retain single-user Rust, official Ant Design, existing native components and target-only delivery.

<a id="spec"></a>
## Accepted scope

[PR #91](https://github.com/zhengui666/QuaZonai/pull/91) merged as `1368e1254b3b2ace4777709e07e9bf305291e096`. DESIGN0.4 closes dedicated-account work as `COMPLETED_BY_OWNER_WAIVER`, execution `NOT_RUN`, not `PASSED`. T07 and only account-dependent live-provider portions of T03–T05/T08/T42 are closed. Do not request a dedicated account or CodexPro reconnection. An absent runtime account still cannot execute genuine model research.

Actual scientific computation, same-Thread result consumption, independent Reviewer, Web/CLI, TOTP, migration, data and recovery obligations remain. A controlled Provider is not live model reasoning. FIXTURE/PIT-UNVERIFIED data cannot be relabeled REAL or granted production qualification. Do not force a scientific PASS to satisfy a delivery checklist.

<a id="plan"></a>
## Implementation

[PR #92](https://github.com/zhengui666/QuaZonai/pull/92), branch `codex/native-science-feedback-20260918`, begins at the #91 merge. Reconcile actual Head before writing. The [scientific-loop plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5727011023), [connected implementation](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5730552855), [native tool approval](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5734040403) and [feedback delivery correction](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5738454941) define the current work.

### Native science and original evidence

`native_research_loop.rs` retains the standalone original-source compilation case. The shared Cycle helper binds the actual built image while creating assumptions, not by modifying frozen rows. Prepare each Runtime ticket before I/O, resolve its credential through SecretVault and use production RuntimeTransport with the exact ticket snapshot. Re-probe before freeze and fresh compilation admission. Inspect the accepted Wasm module through production Wasmi on four varied observations; a non-fixture version marker or Wasm header alone is insufficient. Preserve original source/model/Attempt/manifest/terminal/ACK identity.

`scientific_catalog.rs` creates real Parquet partitions and derives metadata from actual bars/instruments. Register matching datasets, inputs, assumptions, policy and Brief through existing Store APIs. Sibling roots separate Discovery, Validation, Sealed and Forward. Control identity setup is explicitly test infrastructure, not real TOTP or market licensing.

`scientific_feedback.rs` connects the production API/MissionLauncher/Worker, official App Server and actual Runtime. Initial DATA_VALIDATE, compilation, forecast and Validation execute in OCI. The existing Responses fixture scripts only the waived model-provider boundary: tool discovery and real MCP proposal use the actual artifact IDs, and the final controlled summary cites returned Evaluation IDs and decision. It does not calculate scores, emulate the Agent tool loop or grant qualification.

Check each forecast point against observed Parquet prices, Validation counts/metric provenance, original native manifests and the same persistent Thread's two Turn reservations/receipts. Canonical chat files and hidden reasoning are not read. The supported outcome can be REJECT/INCONCLUSIVE; independent Reviewer completion and full T42 remain separate.

### Noninteractive native tool delegation

The pinned Codex [configuration](https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/config/src/mcp_types.rs) and [native call path](https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/src/mcp_tool_call.rs) expose per-tool approval. With a valid Mission token, preauthorize exactly `research.get_brief`, `run.get`, `artifact.submit` and `experiment.propose` under `quazonai_mission`. Do not approve unknown tools, set a global/server default or add an interactive approval handler. API roles, fences, expiry and qualification remain authoritative. Token-free cancellation keeps the server disabled. Existing start/resume unit assertions and actual connected execution cover the change.

### Feedback uses a genuine redelivery

Within one controlled serial delivery, business-stage ticks retain the same owner derived from its native message read count. The first App Server process closes before feedback. Reopening with another secret under that same unexpired Attempt/owner epoch is correctly rejected by the existing credential Store; do not weaken it to make the test pass.

Before consuming the reserved feedback Turn, read the original Attempt and reservation/receipt/credential counts. Wait with native PostgreSQL `pg_sleep` until both the actual lease and PGMQ visibility expire, then verify eligibility using `clock_timestamp()` and obtain the same message through `read_mission_messages`. A higher native read count selects a new owner, matching the daemon's separate claimant for each delivery. Do not edit timestamps, shorten leases, increase the frozen budget, create another Attempt/Thread or regenerate the feedback artifact.

The original production Worker must advance owner_epoch once while retaining Attempt ID/number and Thread, issue one credential for the new epoch, consume actual Evaluation feedback and produce exactly two settled Turns and one terminal/ACK. One principal and two credentials are expected. All scientific and zero-qualification assertions remain. This exercises queue redelivery at the process-reopen boundary; direct serial ticks for intermediate business stages are not claimed as a complete daemon scheduling test.

### Reuse and source publication

Only test-only Runtime edges to existing Server/UUID were added; the accepted original package graph is retained. Native Cargo/rustfmt own generated lock/formatting. Earlier broad lock drift was rejected. All temporary source materialization patches/jobs are removed. Existing concurrency, normal-stack tests, deadlines, scientific limits, source-drift checks and full CI remain unchanged. No credential cache, new scheduler, backup framework or security platform is introduced.

<a id="verification"></a>
## Verification

[PR #92](https://github.com/zhengui666/QuaZonai/pull/92) owns current exact-Head results and merge status. These historical observations do not approve later source:

| Exact Head | Actual evidence |
|---|---|
| `dd8f63a8454b16993851a5140ab7c90830d99b79` | [Runtime35347533422](https://github.com/zhengui666/QuaZonai/actions/runs/35347533422), artifact10549140604:17OCI,2cold/ownership,1joint and standalone original-source compilation passed. Missing pre-freeze probe, image mismatch, ticket/transport and weak model assertions were corrected. |
| `61d68ed96ab1703477a19930b6514f42423d0e58` | [Runtime35375341178](https://github.com/zhengui666/QuaZonai/actions/runs/35375341178), artifact10560362278:17OCI+2restore+1joint passed; research3passed/1failed at actual MCP proposal. Later per-tool configuration required fresh native execution. |
| `00ad66cb2e594e82870badc1a214bffd637709c8` | [Runtime35411144350](https://github.com/zhengui666/QuaZonai/actions/runs/35411144350), job105810886852/artifact10573533366: prerequisites and real proposal/compile/forecast/Validation/feedback preparation passed; research3passed/1failed at consume-feedback with Store::Conflict. Source inspection identified same-fence credential reissuance in the test driver. CI35411144399, Web35411144383, hosting35411144340 and CodeQL35411144339 passed for this Head only. |

The redelivery correction is authored, not presumed executed. Require the published Head's full Native Runtime scenario, actual lease/read-count/owner-epoch observations, original Thread/result and all other applicable CI. Record each real result in the PR; successful configuration, source inspection, preparation or an old green commit is not execution evidence. Account waivers contribute no test passes.

<a id="review"></a>
## Independent review

Request explicit read-only `@codex review` for final Head. Address all actionable summary/inline findings with source and executed evidence. Author inspection, temporary generated trees and older clean feedback are not independent current approval.

<a id="delivery"></a>
## Delivery boundary

1. Complete this PR's declared connected scientific/Thread scope with committed source and no temporary transfer machinery.
2. Require every applicable final-Head CI success, all actionable findings resolved and explicit clean independent review.
3. Only then mark ready, merge with expected-Head verification and inspect main/source identity/post-merge checks.
4. **Never ask GitHub Codex to fix, implement, edit, commit or push.** It reviews only.

Issue #62's closed metadata is not full acceptance. Its owner waiver closes only dedicated-account execution, not the remaining business, scientific, migration or deployment work.

<a id="handoff"></a>
## Continuation

Use the [acceptance index](../../../docs/architecture/issue-62-execution.md#acceptance), actual PR Head and amended DESIGN. Keep this English task/ID and its stable anchors; CONTRIBUTING is the development entry because DEVELOPMENT.md is absent. Never solicit secrets or execute project shell in the web sandbox. Actions requires neither CodexPro nor a dedicated account. Continue the original research/product goal, not unrelated security or supply-chain platforms.
