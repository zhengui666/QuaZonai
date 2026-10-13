# QuaZonai CLI

A compact `/api/v3` HTTP client, released and tested only on **Linux x86_64**. The release archive includes official Node **22.19**, so no host Node/npm is required; source development needs Node **22.19+** to strip TypeScript natively. Parsing uses Node `parseArgs`, transport uses official pinned Undici. This package never opens SQLite or owns a Harness/task loop.

```sh
quazonai login --origin https://YOUR_INSTANCE
quazonai identity
quazonai research models
quazonai datasets
quazonai research start --file original-request.json
quazonai research get CONVERSATION_ID
quazonai research wait CONVERSATION_ID --timeout 30
quazonai portfolio build --file original-portfolio.json
quazonai observation start --file original-observation.json
```

Run `quazonai --help` for all compact commands. `client` is an optional spelling alias; `backtest build/list/get` aliases the portfolio consumer. Source DTOs are `packages/contracts/src/index.ts`; native scientific input schemas remain Rust-owned. Offline `schema list/show NAME [--domain]` reads live source/native schemas in source-tree distributions. `npm pack` includes original license and contract assets generated directly from those sources, so installed packages can inspect them offline too. Unavailable assets report `installed_schema_unavailable` without inventing schemas. The npm launcher uses public Node `registerHooks/stripTypeScriptTypes` for its own source because Node does not auto-strip TypeScript under `node_modules`.

Login accepts a password only through hidden input in the user's own terminal, never argv/environment/stdin pipes. Setup remains in the auth UI. A saved login is reused until `login --replace` deliberately creates a replacement; that does not revoke the old device. The profile retains `{schema_version:1,origin,token,development_http,ca_certificate}` under XDG/HOME. Linux writes are atomic/private (0700 directory, 0600 file). Token bytes, including `qzc`, are preserved; legacy acceptance depends on the service. No provider-credential discovery runs.

HTTPS verifies certificates, with an optional explicit private CA. Explicit HTTP sends credentials unencrypted; there are no redirects or implicit scheme changes. `logout` revokes the current device before forgetting it; failed/unknown revocation retains the profile. `forget` is local only. Login/identity output is nonsecret ID/status, never token/password.

All starts require an **explicit original requestId**. File requests are sent with original bytes, preserving decimal strings and safe numeric IDs; `--request-id` asserts the file's ID rather than replacing it. Goal flags require `--dataset`, `--model provider:modelId`, `--max-trials` and `--request-id`. There are no automatic write retries or generated replacement keys. After an unknown outcome, replay only that same original body/ID.

Watch/wait poll only the existing research-detail endpoint. The service must expose the original native `submission:{id,status}` projection; there is no invented submission endpoint/status machine. HTTP 202, IDLE, final facts and empty tasks cannot establish settlement. Default watch budget is 30 seconds; Ctrl-C and timeouts detach observation without cancelling admitted work. Wait exits 0 for native `done`, 2 for `unanswered`, 124 for timeout, 130 for detach. Done does not mean independently evaluated evidence passed. Watch produces NDJSON snapshots/results.

Portfolios are optional real shared-account backtest consumers, not summed factor curves. Observation `paper/live` is an existing source binding, never trading launch; `get` uses list/select where the server get route is optional. No broker orders, retired envelope relay, capital-exit control or withdrawal commands are advertised. Missing values/unsupported adapters fail truthfully.

Development checks:

```sh
npm ci --prefix apps/cli --ignore-scripts --no-audit --no-fund
npm --prefix apps/cli run typecheck
npm --prefix apps/cli test
```

Tests concentrate on private synthetic auth/profile reuse, exact HTTP requests and explicit replay/conflict/error, and bounded native-status observation. They establish client protocol behavior, not real provider/account authorization or model quality.
