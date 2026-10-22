# Native replay memory and exit diagnostics

<a id="task"></a>
## Task

**Task ID:** native-replay-memory

**Source and endpoint:** The owner authorized required repair/deployment and continuation of the original research. Research 1208 / submission 1216 / trial 1347 failed during native task 1349. Repair the existing execution path, deliver through `dev`, and continue the same study and ledger.

<a id="intent"></a>
## Intent

At 2026-10-22 15:39:59 UTC, the host kernel recorded a global OOM kill of native PID 1092213 (`job`, anonymous RSS 10,357,840 KiB). Trial 1347 has no account or metrics; its owner error only says `scientific_execution_failed / unknown`. Trial 1365 / task 1367 is already running and must not be duplicated.

Preserve Astra/max, the original 100000-trial budget, 3000 pUSD, full registered history, dynamic whole-catalog selection, all failed trials, and unrestricted trading frequency. No real orders or new study identity. Source trade/BAR proxies retain their existing execution and PIT limitations.

<a id="spec"></a>
## Requirements and design

- Release replay-only catalog state before final account analysis, retain full scope validation, and remove redundant logging analysis and canonical serialization buffers through official Nautilus APIs. Preserve full canonical output, account statistics, daily returns, fees, clocks and settlement behavior.
- Keep the official process exit code through native runners, child outcomes, trials and owner projection. Recognize fixed error codes after progress lines. Code 137 alone does not prove SIGKILL or OOM; this incident's OOM diagnosis comes from the target PID's kernel record.
- Do not modify third-party implementations, read private databases/authentication stores, reduce the research scope, or invent successful results.

<a id="plan"></a>
## Implementation plan

1. Reuse `catalog_ensemble::replay` and `catalog_simulation_result`: capture final counters, drop no-longer-needed policy/scope collections, disable only log analysis, and preserve the canonical value without a full bytes/parse round trip. Check safe final disposal after all required native results have been captured.
2. Reuse `ExecutionFailure`, the three native runners, existing child error propagation and owner classification. Add optional validated `exitCode`; retain specific native errors and existing abort/replay/cleanup behavior.
3. Add focused regressions for actual native result equivalence and exit/error propagation. Run small TypeScript checks locally; run native compilation and tests in existing remote CI while the live replay occupies host memory.
4. Obtain independent and final GitHub Codex review, merge into `dev`, publish the official image, and reconcile a suitable deployment point against the active original study. No service replacement while an unreconciled native computation is running.

<a id="verification"></a>
## Verification

The kernel kill, original failed trial and new running trial have been read directly. Source inspection confirms redundant native result clones and catalog state retained through report construction; the exact OOM allocation stack is unobserved.

Three focused exit/error regressions failed before the TypeScript fix. After it, the relevant research tests passed 31/31, service projection tests passed 4/4, and research/service type checks passed. Native changes were formatted and `git diff --check` passed. Existing native tests now compare logging-analysis modes and retain complete canonical histories, multi-day returns, PnL/general statistics, fees and scope rejection. Native compilation/tests run through the remote CI linked below; no local native compilation was attempted alongside the live replay. Full-scale memory reduction remains unverified at source preparation. The host has 14 GiB RAM and heavily used swap, so no competing large local build or replay is planned.

The first Node 22 CI run exposed an extensionless test child relying on newer Node module detection: it exited 1 before the requested exit 137. Its path now explicitly ends in `.mjs`, matching the existing native-child tests; the affected local test file passes 4/4. Independent source review found no issues. Current-head CI and GitHub Codex review remain recorded in PR 232.

<a id="delivery"></a>
## Delivery

[PR 232](https://github.com/zhengui666/QuaZonai/pull/232) is authoritative for current CI, review and merge state. Work uses `codex/native-replay-memory`, initially based on `origin/dev` at dd74bf6b. At preparation time the installed service is the earlier official release with trial 1365 running; deployment and original-study continuation still require reconciliation. No scientific performance result was available when this repair was prepared.
