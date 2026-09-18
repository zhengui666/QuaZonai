# Personal production

<a id="task"></a>
## Task

Task ID: `personal-production`. Deliver the owner's personal QuaZonai requirements against [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62), using [DESIGN](../../../DESIGN.md) and its [authorized acceptance scope](../../../DESIGN.md#acceptance-scope). Code, architecture, documentation, deployment and UX remain in scope. Keep per-turn execution logs and failed iterations in the active PR, not product manuals.

<a id="intent"></a>
## Intent

Continue without CodexPro or a dedicated live model account. The web assistant authors source through connected file tools; GitHub Actions executes native checks and generators. GitHub Codex is an independent read-only reviewer, never the implementation agent. Retain single-user Rust, official Ant Design, existing native components and target-only delivery.

<a id="spec"></a>
## Accepted scope

[PR #91](https://github.com/zhengui666/QuaZonai/pull/91) merged as `1368e1254b3b2ace4777709e07e9bf305291e096`. DESIGN0.4 records the owner's 2026-09-18 authorization: dedicated-account work is `COMPLETED_BY_OWNER_WAIVER`, execution `NOT_RUN`, not `PASSED`. T07 and only account-dependent live-provider portions of T03–T05/T08/T42 are closed. Do not request a dedicated account or CodexPro reconnection. Actual product research still requires usable native authentication; an empty profile does not become ready.

T08's actual scientific computation, same-Thread result consumption, independent Reviewer, Web/CLI, TOTP, migration, data and recovery obligations remain. Controlled provider responses do not constitute live model reasoning; controlled scientific outputs do not establish native computation. FIXTURE/PIT-UNVERIFIED data cannot be relabeled as REAL or granted production qualification.

<a id="plan"></a>
## Implementation

Current [PR #92](https://github.com/zhengui666/QuaZonai/pull/92), branch `codex/native-science-feedback-20260918`, starts at the #91 merge. [The scientific-loop plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5727011023) defines the full non-account target. [The observed-failure repair](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5730177796) addresses the initial compilation-stage defects. Preserve concurrent work and reconcile actual Head before publishing.

### Native compilation checkpoint

The standalone `native_research_loop` case covers original-source experiment compilation, not the complete loop. It reuses the existing Cycle/Experiment preparation, actual Runtime/OCI and production Worker. Cycle/data-quality preparation is explicitly controlled; the compilation itself must be real. The shared Cycle helper accepts an explicitly built image when creating assumptions, before freeze. Original callers keep the original fixture behavior; frozen assumptions are not mutated.

After Runtime configuration changes, prepare a Store probe ticket before I/O, resolve its credential reference through the existing SecretVault, and use production RuntimeTransport with that exact snapshot. Publish the actual observation before freezing. The proposal helper's controlled probe is replaced by another real configured observation before native compilation admission.

After the Worker publishes and acknowledges the original message, inspect its exact Attempt/spec, real OCI container, qz.model_compilation and qz.wasm_model mapping. The accepted model must execute the submitted close-minus-previous source on four varied observations through the production Wasmi adapter. Check source ID, model reference/size, original raw manifest and unique terminal receipt/ACK. A non-fixture version label or Wasm header alone is not compilation proof.

Server and UUID are test-only Runtime dependencies. Retain every accepted lockfile package/version/source and existing dependency list; only Runtime gains those two test edges. Native Cargo owns lockfile resolution and rustfmt owns formatting. The temporary exact-patch preparation entry and compressed patch are removed before formal-source validation. No permanent generator, dependency upgrade or changed product protocol is introduced. Restore the original Native Runtime concurrency group and preserve all original checks and limits.

### Connected scientific loop

The [implementation detail](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5730552855) extends the same target with one connected scenario and two focused test-support modules. `scientific_catalog.rs` writes actual native Parquet partitions, derives metadata from observed bars/instruments and registers source/grant/datasets/inputs/assumptions/policy/Brief through existing Store APIs. Sibling catalog roots prevent one authorized partition mount exposing another. Provenance remains FIXTURE/PIT-UNVERIFIED; control identity setup is explicit test infrastructure, not real TOTP or market licensing.

`scientific_feedback.rs` uses production API/MissionLauncher/Worker, the official App Server and real Runtime. Initial data quality is executed, not synthesized. The existing Responses fixture scripts native tool discovery and `experiment.propose` using actual uploaded artifact IDs. It does not run an Agent loop. Actual compile, forecast and validation output is published by the Worker, then the original persistent Thread receives its bounded Evaluation and returns a controlled public conclusion quoting the actual IDs/decision. Normal Mission calls close and reopen the native process; canonical chat files and hidden reasoning are not read.

The test compares original Run/Attempt/spec/manifest/ACK identities, Wasm behavior, each forecast point against observed Parquet prices, validation sample/metric provenance, exact feedback and two original-Thread reservations. It expects no qualification for FIXTURE input, not a forced scientific PASS. Existing standalone compilation and protocol regressions stay. The original Runtime job reuses locked Codex and existing native user-manager prerequisites; no new dependency, control service, scientific engine or permanent workflow is added.

The connected scenario has executed but has not passed. Its source and configuration are not acceptance evidence. Read its actual final-Head result before closing the supported T08 non-account portion; T09/T42 and remaining original obligations are not automatically fulfilled.

### Noninteractive MCP delegation

The [native approval repair](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5734040403) changes only the trusted Mission configuration, not the API's authority model. The pinned [Codex configuration types](https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/config/src/mcp_types.rs) expose per-tool `approval_mode`; the [call path](https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/src/mcp_tool_call.rs) applies it before invoking MCP. Without an explicit override, native tool approval can require an interactive request which the noninteractive QZ Wire intentionally rejects.

With a valid Mission token, preauthorize only `research.get_brief`, `run.get`, `artifact.submit` and `experiment.propose` under the launcher's own `quazonai_mission` server using native `approve`. Do not set a server/global default, approve unknown tools or handle arbitrary interactive requests. Keep `approvalPolicy=never`, disabled ambient MCP servers, bounded filesystem/network configuration and all API scopes, fences and expiry checks. This does not grant a Reviewer write authority or grant qualification/approval/delivery. Token-free cancellation/reconciliation keeps that server disabled with no credential or approval map. Start/resume unit assertions cover the four exact entries and disabled path; real connected MCP/science execution remains the functional acceptance.

<a id="verification"></a>
## Verification

At `4f82a9c33621975f2791025f466da9e88a689a2a`, [Runtime35336430591](https://github.com/zhengui666/QuaZonai/actions/runs/35336430591), job105572222463, passed formatting then failed Clippy at six undeclared UUID references in shared test helpers. No new scientific execution occurred. Independent review4046067745/751/757/762 identified missing pre-freeze probing, a controlled image mismatch, ticket/transport binding and inadequate model assertions.

The earlier generate-lockfile invocation also changed unrelated transitive packages. That drift was rejected: baseline lock bc316acc was restored, and native metadata must retain all641 original packages and all dependency lists except the two Runtime test edges. [Preparation35347141805](https://github.com/zhengui666/QuaZonai/actions/runs/35347141805), artifact10546888516, applied the exact authored patch, resolved only those edges and formatted source. The four returned files were byte/blob-verified against the actual artifact; the source differences beyond the authored text were native formatting only. Preparation is not a compilation or test pass.

At `dd8f63a8454b16993851a5140ab7c90830d99b79`, [Runtime35347533422](https://github.com/zhengui666/QuaZonai/actions/runs/35347533422), artifact10549140604, passed17OCI,2cold/ownership,1joint recovery and1original-source compilation test. The compilation case executed the real model at four inputs and checked original manifest/publication/ACK. These are executed prerequisite results, not a passed connected forecast/validation/Thread scenario.

At `61d68ed96ab1703477a19930b6514f42423d0e58`, [Runtime35375341178](https://github.com/zhengui666/QuaZonai/actions/runs/35375341178), job105698586627/artifact10560362278, passed17OCI,2restore and1joint case. The research target had3passes/1failure: standalone compilation and two output-wrapper cases passed; the connected scenario still failed before obtaining a CODEX experiment receipt. Its bounded diagnostic found a short native text result without the expected Cycle or QZ error code. The preceding d8 candidate had stopped at formatting; 61d applied that exact format change. The subsequent per-tool native approval correction requires its own actual execution; no result is inferred from the diagnostic length or configured policy.

Require final committed-source CI, Web, Native Runtime, hosting and CodeQL. Read the real compilation report/module behavior, original identity and ACK, then the connected scientific/Thread results. Earlier main or preparation results do not approve new source. Missing, failed, skipped, ignored, cancelled and old-Head checks are not passes. Account waivers contribute no test passes.

<a id="review"></a>
## Independent review

Request explicit read-only `@codex review` for the exact final Head. Address all actionable summary/inline findings and execute affected tests after repairs. Author inspection, a temporary generated tree and older clean review are not independent current approval.

<a id="delivery"></a>
## Delivery boundary

1. Complete the PR's declared scope and publish actual source without temporary transfer machinery.
2. Require every applicable final-Head CI success, all actionable findings resolved and explicit clean independent review.
3. Only then mark ready, merge with expected-Head verification and inspect main/post-merge checks.
4. **Never ask GitHub Codex to fix, implement, edit, commit or push.** It reviews only.

Issue #62's closed metadata is not full acceptance. Its explicit account waiver closes only that portion, not the remaining business, scientific, migration or deployment requirements.

<a id="handoff"></a>
## Continuation

Use the [acceptance index](../../../docs/architecture/issue-62-execution.md#acceptance), actual PR Head, checks and original DESIGN. Keep this English task and ID; CONTRIBUTING is the development entry because DEVELOPMENT.md is absent. Never solicit secrets or run repository shell commands in the web sandbox. Actions execution needs no CodexPro reconnection or dedicated account. Do not replace the actual research goal with unrelated security/supply-chain platforms.
