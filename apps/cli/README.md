# QuaZonai CLI

A compact `/api/v3` HTTP client. Node **22.19+** strips TypeScript natively; parsing uses Node `parseArgs`, transport uses official pinned Undici. This package never opens SQLite or owns a Harness/task loop.

```sh
quazonai login --origin https://YOUR_INSTANCE
quazonai identity
quazonai research models
quazonai datasets
quazonai research start --file original-request.json
quazonai research get CONVERSATION_ID
quazonai research wait CONVERSATION_ID --timeout 30
quazonai research diagnostics CONVERSATION_ID
quazonai research resume CONVERSATION_ID --submission-id PREVIOUS_TERMINAL_SUBMISSION
quazonai portfolio build --file original-portfolio.json
quazonai observation start --file original-observation.json
```

Run `quazonai --help` for all compact commands. `client` is an optional spelling alias; `backtest build/list/get` aliases the portfolio consumer. Source DTOs are `packages/contracts/src/index.ts`; native scientific input schemas remain Rust-owned. Offline `schema list/show NAME [--domain]` reads live source/native schemas in source-tree distributions. `npm pack` includes original license and contract assets generated directly from those sources, so installed packages can inspect them offline too. Unavailable assets report `installed_schema_unavailable` without inventing schemas. The npm launcher uses public Node `registerHooks/stripTypeScriptTypes` for its own source because Node does not auto-strip TypeScript under `node_modules`.

Login accepts a password only through hidden input in the user's own terminal, never argv/environment/stdin pipes. Setup remains in the auth UI. A saved login is reused until `login --replace` deliberately creates a replacement; that does not revoke the old device. The profile retains `{schema_version:1,origin,token,development_http,ca_certificate}` under XDG/HOME or APPDATA. Unix writes are atomic/private (0700 directory, 0600 file); Windows uses a private current-owner DACL. Token bytes, including `qzc`, are preserved; legacy acceptance depends on the service. No provider-credential discovery runs.

HTTPS verifies certificates, with an optional explicit private CA. Explicit HTTP sends credentials unencrypted; there are no redirects or implicit scheme changes. `logout` revokes the current device before forgetting it; failed/unknown revocation retains the profile. `forget` is local only. Login/identity output is nonsecret ID/status, never token/password.

All starts require an **explicit original requestId**. File requests are sent with original bytes, preserving decimal strings and safe numeric IDs; `--request-id` asserts the file's ID rather than replacing it. Goal flags require `--dataset`, `--model provider:modelId`, `--max-trials` and `--request-id`. There are no automatic write retries or generated replacement keys. After an unknown outcome, replay only that same original body/ID.

Watch/wait poll only the existing research-detail endpoint. The service must expose the original native `submission:{id,status}` projection; there is no invented submission endpoint/status machine. HTTP 202, IDLE, final facts and empty tasks cannot establish settlement. No default observation or HTTP deadline is imposed; Ctrl-C and an explicit `--timeout` detach observation without cancelling admitted work. Wait exits 0 for native `done`, 2 for `unanswered`, 124 for timeout, 130 for detach. Done does not mean independently evaluated evidence passed. Watch produces NDJSON snapshots/results.

`diagnostics` reads original native identity/status and a safe model-error category, never raw provider text or model reasoning. After owner authorization and reconciliation, `resume ID --submission-id PREVIOUS_ID` admits one new input to the **same** original conversation after a latest `unanswered/model_error` or a latest `done` submission with an unselected `STOPPED` result and no final assessment. It retains input/model/thinking level, trials, original budget and exposure; complete prior stops remain in `previousStops` for owner reads. The stable request key binds those exact IDs; replay returns the same linked receipt without another history entry or model round, including after a response was lost. A saved intent repairs a missing admission after restart, unless a later owner stop takes precedence. Selected/assessed finals, aborted/active inputs, mismatched predecessors and exhausted budgets cannot continue. Old submissions/tasks are never reopened or rewritten. `submission` tracks the latest input while `originalSubmission` preserves original admission;202 establishes neither completed recovery nor scientific qualification. No automatic write retry, replacement study or counter reset occurs.

Portfolios are optional real shared-account backtest consumers, not summed factor curves. Observation `paper/live` is an existing source binding, never trading launch; `get` uses list/select where the server get route is optional. No broker orders, retired envelope relay, capital-exit control or withdrawal commands are advertised. Missing values/unsupported adapters fail truthfully.

Development checks:

```sh
npm ci --prefix apps/cli --ignore-scripts --no-audit --no-fund
npm --prefix apps/cli run typecheck
npm --prefix apps/cli test
```

Tests concentrate on private synthetic auth/profile reuse, exact HTTP requests and explicit replay/conflict/error, and bounded native-status observation. They establish client protocol behavior, not real provider/account authorization or model quality.
