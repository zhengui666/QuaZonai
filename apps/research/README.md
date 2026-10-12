# Durable factor research host

One official Pi Durable host owns model calls, conversations, submissions, tool execution, persistent tasks, children, recovery, cancellation, views and usage. QuaZonai adds original research inputs, trial/selection facts and small scientific extensions. It does not implement another Agent loop or session database.

## Run from source

Requires Node 22.19+ (including built-in SQLite), npm, Bash and the matching Rust `job` executable. The installed application packages these; this section is for development, not a prerequisite for end users.

```sh
npm ci --prefix apps/research --ignore-scripts --no-audit --no-fund
cp apps/research/research.example.json apps/research/research.json
QZ_RESEARCH_CONFIG="$PWD/apps/research/research.json" \
  QZ_JOB_BINARY="$PWD/target/debug/job" npm --prefix apps/research start
```

`research.json` is local configuration. `database` and module paths are resolved relative to its directory. Keep the official SQLite database and its sidecars together; never run two owners against it. Serve binds loopback port 8092. Rust exposes owner-authenticated `/api/v2/research*` through `QZ_RESEARCH_ORIGIN`; do not expose the raw local Node port publicly.

### Models and extensions

The host uses official pi-ai providers, catalogs and auth resolution. Configure provider credentials in the host's private environment or a trusted provider module, never in a research request, conversation, tool argument or browser form. No Codex CLI/App Server or project-specific account/profile discovery runs. `/models` returns the native configured availability, not a successful account test or fabricated fixed catalog.

- `modules`: trusted modules whose default export is an official `Extension` built with `defineExtension`/`defineTool`/`defineTask`.
- `providerModules`: optional trusted modules whose default export is an official pi-ai `Provider`. They register with native `Models.setProvider`; no alternate inference loop is added.
- Missing providers, native capability or input data are explicit failures. No fake response or alternate engine is substituted.

Default extension roles:

| Extension | Public behavior |
| --- | --- |
| `research.ts` | Original brief, complete trial disclosure and stop reason; no asset or Wasm ABI in workflow logic |
| `catalog-input.ts` | Configured native directory/iterative selection descriptor; explicitly not quality/PIT/permission evidence |
| `factor-wasm.ts` | Arbitrary signed factors via existing native Wasmi/Nautilus features and real Pearson IC, without positions |
| `heldout-wasm.ts` | One selected factor, original criteria and session-wide exposure reservation before independent final computation |
| `factor-volume.ts` | Additional volume-factor module, enabled by adding only its registration |
| `validation-compare.ts` | Additional evaluator that compares all disclosed trials under original `validation.pearson_ic` policy |

Enable the last two by adding their paths to `modules`. `factor-wasm.ts` registers the shared scientific child Task used by those adapters. Data/market acquisition can reuse the existing [source tools](../../runtimes/data/source-plugins.md), or use another official Extension. The core does not enumerate assets, providers, factors or data sources.

## Submit and observe

The Web research form and saved-owner CLI use the same Rust bridge. Login privately in the user terminal before operating the service:

```sh
quazonai client login
quazonai client research models
quazonai openapi --schema DurableResearchStartV1
quazonai openapi --domain --schema NativeFactorEvaluationRequestV1
quazonai client research start --file research-request.json
quazonai client research list
quazonai client research get 42
quazonai client research tasks
quazonai client research usage
quazonai client research stop 42
```

`42` illustrates a **native numeric conversation ID**, not a real installed study. A submission file contains `requestId` and `input`: original `goal`, native `model:{provider,modelId}`, extension-owned `data` and original `policy` objects. Select model IDs from actual availability. Input definitions are generated from [source contracts](../../crates/contracts/src/durable_research.rs).

HTTP 202 means admitted/replayed or a stop request received, not computation, qualification or proof of remote cancellation. Retain the exact original file and `requestId` after an unknown outcome; replay returns the original native identities. A changed body under the same ID is a conflict. Scientific failures and rejected proposals stay in the ledger. An explicit `policy.max_trials` is a nonnegative integer; zero allows no scientific attempt, and exhaustion terminates generation without claiming final evidence.

Native views contain conversations, transcript and built-in documents plus the scientific `research` document. Tasks expose native active inspection and a page of native stored records; usage comes directly from the Harness. These are observations, not a second authoritative Agent state. Stopping an observer does not stop research; the explicit stop calls native abort, including owned background children.

## Scientific input and isolation

The default Wasm adapter takes an original prepared, versioned catalog path in `data.catalog`, and a `NativeFactorEvaluationRequestV1` in `data.request`. Paths come from trusted owner configuration, not model tool arguments. Preserve source identity, licensing/PIT evidence, Instrument definitions and original availability clocks; path presence alone does not establish any of them. Exposure is keyed by native BAR identity and original event windows across the whole session, independent of caller-supplied `datasetId`, catalog path, copies or revisions. Those labels and paths cannot make observed data unseen.

`data.sealed`, when requested, contains the final evaluator's original catalog/request and same source identity. It is omitted from model-facing brief output. Final parameters must match the selected proposal's original criteria; only the end of its data range expands beyond validation. Iterative computation truncates the selected range at `validation_end_ns`, including the labels, so it cannot read the final suffix. Predictions use causal native features; label event horizon and availability must stay inside each shared half-open partition. All assets share time cuts rather than row-ordinal cuts.

`factor_finalize` records the selected trial and complete compared set before giving the evaluator read capability. Exposure is a native **session** document: crashes, new roots and backward forks cannot free that sample or erase past selection/trials. Final result/failure is owner-visible, and the tool terminates without returning metrics to the model. Exposed samples are not fresh independent evidence. No DSR/PBO/qualification/profit claim is inferred from finite IC or a successful process.

The adapter preserves scores of any finite sign/scale, missing masks and original clocks; unavailable metrics are null with a reason. It compiles WAT with official WABT, calls the matching `job evaluate-factor`, checks request association, and omits raw point/label rows from tool feedback. Native child execution receives no model-provider credentials. Portfolios, Nautilus simulation/accounting/fees and target-only delivery remain separate optional consumers.

Model-generated Wasm always receives finite native fuel: omitted/null `total_fuel` uses `"10000000"`, while an explicit decimal-string budget remains explicit. The original study input is unchanged and results echo the effective execution request and actual consumed fuel. The adapter uses the official environment's process timeout (120 seconds) and Bash address-space limit (2048 MiB); trusted owner configuration can set positive `QZ_FACTOR_TIMEOUT_SECONDS` and `QZ_FACTOR_MEMORY_MB`. Exhaustion, timeout or allocation failure remains a failed trial, not a free retry. Trusted direct native Forecast/factor CLI callers retain their optional unmetered semantics.

## Verification

```sh
make check-research
```

This checks types, real SQLite/HTTP/replay/abort/SIGKILL safe/interrupted behavior, and native factor calculations. A scientific `FAILED` fact survives a crash before the safe tool becomes terminal: reopening rethrows the original error without recomputation. The Rust factor target launches Node integration against actual native mixed CurrencyPair/Equity catalogs, registers additional data/factor/evaluator modules, runs multiple proposals, selects using real validation feedback and verifies final isolation and exposure persistence across renamed labels and copied catalogs. Iterative reads are recorded before computation, so validation feedback cannot become unseen final evidence in another root after SQLite reopen. Omitted/null fuel receives the disclosed finite model-code budget through finalization; infinite prediction and oversized memory fail finitely. Official Faux Provider controls model behavior **only**. Synthetic catalogs are not real-market data quality, model quality, account authorization, profitability or qualification evidence.

Full project, browser, installation and native OCI checks are listed in [.opensdlc/project.md](../../.opensdlc/project.md#commands); actual task outcomes remain in the [task record](../../.opensdlc/tasks/durable-factor-platform/task.md#verification).
