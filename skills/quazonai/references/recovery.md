# Errors and uncertain outcomes

Use the CLI's actual nonzero exit, native HTTP status/payload and persisted state. JSON errors, HTTP 202 and a closed observer are not completion. Do not retry indiscriminately.

| Observed result | Next action |
| --- | --- |
| Local `CLI_INPUT_INVALID` or missing/unknown command/schema | Inspect the selected native help/schema and repair the request before sending. Unknown DTO names are not permission errors. Do not guess another endpoint. |
| `CLI_LOGIN_REQUIRED`, `CLI_LOGIN_REQUIRES_TERMINAL`, or revoked owner device | Stop writes; have the user run `quazonai client login` in their own terminal and enter the frontend address and hidden password there. Never collect the password through the Agent or chat. |
| `CLI_LOGIN_REPLACE_REQUIRED` | An existing login points to another instance or device name. Have the user intentionally run `quazonai client login --replace` in their terminal; the previous device remains managed in settings. |
| `PASSWORD_RATE_LIMITED` | The browser and CLI share a one-minute failed-password limit. Stop attempts and let the user retry login in their private terminal after one minute; do not retry concurrently, request the password, or replace credentials to bypass the limit. |
| `CLI_CREDENTIAL_INVALID`, or unauthorized/expired/revoked scoped capability | Stop writes; ask the trusted provisioner to repair the original connection. Never read token contents, reuse browser cookies or switch a restricted identity to owner login. |
| Wrong project, insufficient scope, stale historical Attempt or missing exact grant | Report the original binding and required capability without acquiring or expanding it yourself. Restricted identities do not gain durable owner authority. |
| `CLI_SERVER_UNAVAILABLE_OR_RESULT_UNKNOWN`, or a submission timeout | Outcome may already be committed. Retain the same key, original body and exact authorization. Inspect known resource/Run IDs; if an authorized retry is necessary, replay only that original request to obtain its receipt. Do not automatically create a new ID/key or claim rollback. |
| Conflict / revision mismatch | Read the current record, compare it with the original intent and explain the conflict. Do not silently rewrite the request revision or replace an immutable object. Same key plus changed body is not a valid retry. |
| Explicit retryable auth rate limit | Honor a Retry-After value when exposed by the native surface and avoid concurrent retries. If the surface does not provide the retry policy, stop and report rather than guessing a delay or falling back to raw HTTP. |
| `BUDGET_EXHAUSTED`, `retryable=false`, or unclassified HTTP 429 | Do not auto-retry, change keys, create another Attempt or claim this is transient. Report the budget/unknown classification to the task owner or launcher. |
| `CLI_EVENT_CURSOR_RESET_REQUIRED` | Preserve the old cursor, read the current Run snapshot and disclose the event gap. Resume a new observation only explicitly; never claim gap-free history. |
| `CLI_RESPONSE_CONTRACT_INVALID`, incompatible event/version or response limit | Stop interpreting it as valid evidence. Report the client version and code; narrow a supported read or have the host update the client, not bypass validation. |
| Remote outcome unknown during cancellation | Reconcile the original Run. Neither a timeout, 404, disconnect nor local process exit proves remote cancellation. |
| Durable timeout, `request_id_conflict`, `trial_budget_exhausted`, or `heldout_already_exposed` | Keep the original file and `requestId`, inspect `research get` and native tasks. Never replace the ID/body, clear trials or rename exposed data to make the operation pass. |

Preserve only operational context: resource IDs, exact nonsecret requests, keys, returned receipts and event cursors in the host's approved task workspace. Do not write directly to QuaZonai's database, clear ledgers or budgets, alter policy, expose secrets or create a second workflow engine.

A useful blocked response says what was requested, which exact resource/Run is involved, what was observed, whether anything may have committed, and the single missing action needed to proceed. It never substitutes a reassuring success message for missing evidence.
