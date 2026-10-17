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
quazonai research resume CONVERSATION_ID --submission-id ORIGINAL_FAILED_SUBMISSION
quazonai portfolio build --file original-portfolio.json
quazonai observation start --file original-observation.json
```

Run `quazonai --help` for all compact commands. `client` is an optional spelling alias; `backtest build/list/get` aliases the portfolio consumer. Source DTOs are `packages/contracts/src/index.ts`; native scientific input schemas remain Rust-owned. Offline `schema list/show NAME [--domain]` reads live source/native schemas in source-tree distributions. `npm pack` includes original license and contract assets generated directly from those sources, so installed packages can inspect them offline too. Unavailable assets report `installed_schema_unavailable` without inventing schemas. The npm launcher uses public Node `registerHooks/stripTypeScriptTypes` for its own source because Node does not auto-strip TypeScript under `node_modules`.

Login accepts a password only through hidden input in the user's own terminal, never argv/environment/stdin pipes. Setup remains in the auth UI. A saved login is reused until `login --replace` deliberately creates a replacement; that does not revoke the old device. The profile retains `{schema_version:1,origin,token,development_http,ca_certificate}` under XDG/HOME or APPDATA. Unix writes are atomic/private (0700 directory, 0600 file); Windows uses a private current-owner DACL. Token bytes, including `qzc`, are preserved; legacy acceptance depends on the service. No provider-credential discovery runs.

HTTPS verifies certificates, with an optional explicit private CA. Explicit HTTP sends credentials unencrypted; there are no redirects or implicit scheme changes. `logout` revokes the current device before forgetting it; failed/unknown revocation retains the profile. `forget` is local only. Login/identity output is nonsecret ID/status, never token/password.

All starts require an **explicit original requestId**. File requests are sent with original bytes, preserving decimal strings and safe numeric IDs; `--request-id` asserts the file's ID rather than replacing it. Goal flags require `--dataset`, `--model provider:modelId`, `--max-trials` and `--request-id`. There are no automatic write retries or generated replacement keys. After an unknown outcome, replay only that same original body/ID.

Watch/wait poll only the existing research-detail endpoint. The service must expose the original native `submission:{id,status}` projection; there is no invented submission endpoint/status machine. HTTP 202, IDLE, final facts and empty tasks cannot establish settlement. No default observation or HTTP deadline is imposed; Ctrl-C and an explicit `--timeout` detach observation without cancelling admitted work. Wait exits 0 for native `done`, 2 for `unanswered`, 124 for timeout, 130 for detach. Done does not mean independently evaluated evidence passed. Watch produces NDJSON snapshots/results.

`diagnostics` reads original native identity/status and a safe model-error category, never raw provider text or model reasoning. After owner authorization and reconciliation of a model failure, `resume ID --submission-id FAILED_ID` admits one new input to the **same** original conversation, retaining input/model/thinking level, trials, budget and exposure. The service derives one stable request key from those exact IDs; replaying the same command returns the same linked continuation receipt without another model round. The old failed submission/task remains failed. No automatic write retry, replacement research or counter reset occurs. Final/stopped, busy/non-model-failed and exhausted studies cannot be resumed. Native `submission` in reads tracks the latest input while `originalSubmission` preserves admission identity; 202 still does not establish completed recovery or scientific qualification.

Portfolios are optional real shared-account backtest consumers, not summed factor curves. Observation `paper/live` is an existing source binding, never trading launch; `get` uses list/select where the server get route is optional. No broker orders, retired envelope relay, capital-exit control or withdrawal commands are advertised. Missing values/unsupported adapters fail truthfully.

Development checks:

```sh
npm ci --prefix apps/cli --ignore-scripts --no-audit --no-fund
npm --prefix apps/cli run typecheck
npm --prefix apps/cli test
```

Tests concentrate on private synthetic auth/profile reuse, exact HTTP requests and explicit replay/conflict/error, and bounded native-status observation. They establish client protocol behavior, not real provider/account authorization or model quality.
