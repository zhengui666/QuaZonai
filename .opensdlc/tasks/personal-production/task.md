# Personal production

<a id="task"></a>
## Task

Task ID: `personal-production`. Deliver the owner's personal QuaZonai requirements against [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62), using [DESIGN](../../../DESIGN.md) and its [owner-authorized scope](../../../DESIGN.md#acceptance-scope). Code, architecture, documentation, deployment and UX remain in scope. PR discussions hold each turn's public actions and evidence, not hidden reasoning or duplicated product history.

<a id="intent"></a>
## Intent

Continue core business acceptance without CodexPro or a dedicated live account. The web assistant authors through GitHub file tools; existing Actions run native verification. Account-dependent acceptance remains `COMPLETED_BY_OWNER_WAIVER` / `NOT_RUN`, not a test pass. Do not reopen it or request credentials.

Current work connects previously separate evidence: actual App Server tool execution, genuine numerical Job output, immutable Mission publication and result consumption after a same-Thread process restart. Controlled model responses must not replace computation, and a success result must not contaminate a later infeasible result.

<a id="spec"></a>
## Scope and limits

[PR #91](https://github.com/zhengui666/QuaZonai/pull/91) merged the owner amendment as `1368e1254b3b2ace4777709e07e9bf305291e096`. T07 and only dedicated-account portions of T03–T05/T08/T42 are closed by waiver. The original TOTP, data, science, independent Reviewer/Sealed, budget, persistence and business-flow requirements remain.

The [native science/Thread plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5728884901) adds a bounded integration, not full T08 or T42 acceptance. Provider decisions and the parent research records are explicit fixtures. The production Job binary really computes both results; native App Server owns shell/MCP dispatch and persistence; the actual API and PostgreSQL/ArtifactStore own publication. No qualification or delivery authority is granted.

<a id="plan"></a>
## Implementation and reuse

Branch `codex/native-science-thread-20260918` starts from main1368e125. Reconcile actual Head before any write. Preserve the existing task and English language; `.opensdlc/config.json` is absent.

`apps/server/tests/native_science_thread.rs` reuses the native Client/ThreadOptions, existing Mission MCP/HTTP fixture, allocation input and production `job allocate` CLI. Its explicit `native-science` test feature includes `native-codex`; the existing Native Runtime workflow builds the required Job and runs the target against its disposable PostgreSQL. No new dependency, lockfile, production API, permissions, model loop or numerical implementation.

The test copies the actually built Job executable into its private Mission workspace and uses native shell dispatch. The controlled provider receives the real tool output, validates it through the existing domain contract, discovers `artifact.submit` and submits the original output file through actual MCP/HTTP. Its public fixture conclusion must cite the returned Artifact ID and numerical status. The test checks the original bytes again through Store/ArtifactStore.

After a real native process close, a new process resumes the same Thread. The first allocation is optimal; the second has valid but infeasible constraints, so it must produce no weights and cite its distinct new artifact while retaining the first reference in native context. Polling an unfinished exec session continues that session rather than launching a second command. Public-summary reads must not trigger model calls or return the newest answer for an older Turn. Controlled usage counters remain labeled as such.

Reuse the already locked Codex install and existing Ubuntu native sandbox prerequisite, with scoped cleanup. All original OCI/cold/joint, Store, Web, hosting and CodeQL checks remain. No real account invocation, increased test limit, ignored prerequisite or automatic retry to hide failure is introduced. No project shell/compiler is executed in the web sandbox.

<a id="verification"></a>
## Verification

At authorship this new test has not executed. Test existence, source inspection and the earlier main's green checks are not a new pass. Read actual fmt/Clippy, native target and complete workflow outcomes for each candidate. On failure, repair the concrete source or fixture contract and preserve the failing evidence in the PR.

Require two genuine Job results (optimal and infeasible), two original HTTP publications, exact producer Run/Attempt identity and bytes, one persisted Thread across two native processes, actual result consumption and no fabricated weights on the negative result. No private request bodies, credentials or canonical model-history files are uploaded.

This connection does not verify production Worker's full experiment/evaluation orchestration, independently qualified Alphas, real market provenance, owner legacy migration or complete Web/CLI research-to-delivery. The [acceptance index](../../../docs/architecture/issue-62-execution.md#acceptance) keeps these remaining requirements separate from the closed account scope.

<a id="review"></a>
## Independent review

Request explicit read-only `@codex review` for the final Head, address every actionable summary and inline finding, and revalidate changed source. Author inspection and older clean results do not replace independent current review.

<a id="delivery"></a>
## Delivery boundary

1. Publish the PR and complete its declared scope with committed source and no temporary transfer machinery.
2. Require all applicable final-Head CI successes, all actionable findings resolved and explicit clean independent review for that Head.
3. Only then mark ready and merge using expected-Head verification, inspect main and post-merge checks, and record the actual merge.
4. **Never ask GitHub Codex to fix, implement, edit, commit or push.** It reviews only; the web assistant authors all changes.

Issue #62's closed metadata alone is not acceptance. Do not broaden the account waiver or count it as a passed test; do not use this focused connection to declare the entire product delivered.

<a id="handoff"></a>
## Continuation

Read AGENTS, CONTRIBUTING (`DEVELOPMENT.md` is absent), relevant DESIGN and the current PR/CI state before continuing. Do not request a CodexPro connection or dedicated account. Use file tools for authorship and Actions for execution, protect concurrent changes and user data, and record public turn stages in the active PR.

Complete the remaining non-account Thread/Worker, data/migration, runbook and full business entrypoint acceptance using existing native interfaces. Missing real data or owner snapshot evidence is not silently waived by the account decision.
