# Accounts and capital

## Create and read an account observation

Read `observation list` and `schema show ObservationStart` first. An observation requires an existing `connectionId`, with a matching mode. Read connection identity from an existing observation; otherwise ask the user or configure it through the instance UI. The CLI currently has no connection creation/listing command.

Form the request:

```json
{
  "requestId": "REPLACE_NEW_UNIQUE_REQUEST_ID",
  "purpose": "ACCOUNT_ONLY",
  "mode": "paper",
  "connectionId": "REPLACE_EXISTING_CONNECTION_ID"
}
```

This example uses paper. Choose paper/live only according to the user's actual goal; switching modes is not ordinary failure recovery.

```sh
quazonai observation start --file observation-request.json
quazonai observation get "$observation_id"
quazonai observation refresh "$observation_id"
```

Read `status`, `updatedAt`, `positions`, `equity`, `cash`, `receipt`, `error`, and `provenance`. Preserve client/account/sequence/nativeClock identity and timing basis; do not replace `null` numbers with zero. `observation stop ID` stops observation, not orders, positions, or the brokerage account.

## Explicit control of a configured native strategy

```sh
quazonai account-strategy status "$observation_id"
quazonai account-strategy start "$observation_id" --request-id "$request_id" --account-source-id "$account_source_id" --expected-revision "$strategy_revision"
quazonai account-strategy stop "$observation_id" --request-id "$stop_request_id"
quazonai account-strategy receipt "$observation_id" "$request_id"
```

START's accountSourceId/strategyRevision must come from the status read when the user decided to start. Report a stale-revision rejection first; do not automatically refresh the revision and reauthorize the old command. STOP does not require a revision but still requires explicit user intent. Account execution currently wires only the official EMA_CROSS strategy. ORDER_BOOK_IMBALANCE, GRID_MARKET_MAKER, and HURST_VPIN_DIRECTIONAL currently support research backtests, not account execution. Historical backtests do not deploy strategy configuration into connections. The CLI is not a strategy source uploader or connection configurator.

Starting the default host does not start trading. paper/live use separate connections. A live START may place actual orders and must satisfy explicit user authorization and applicable confirmation requirements; a favorable backtest rating is not authorization. Recovery warms up again and does not promise seamless restoration of internal indicator state. STOP follows upstream configuration semantics and does not guarantee that orders or positions remain untouched. Stopping observation is not strategy STOP.

Command receipts distinguish REJECTED, UNCERTAIN, and actual native outcomes. persisted:false means the receipt was not persisted; even STOPPED is not proof of durable recording or replay protection across restarts. When the network outcome is uncertain, query the original requestId rather than automatically resending. Current STATUS is not proof that an old command completed. Describe cancellation protection, order previews, and sizing according to actual capabilities; a preview is not an order.

## Capital exit: status → preview → command → reconciliation

1. Run `capital status OBSERVATION_ID`. Inspect `capability`, `reason`, `controlRevision`, `exits`, and `commands`. Stop that operation and report the reason for `UNSUPPORTED` / `BLOCKED`.
2. Save the user-specified amount and observation currency in a preview request with a new requestId:

```json
{"requestId":"REPLACE_PREVIEW_REQUEST_ID","amount":"1000","currency":"USD"}
```

Omitting policy uses `CASH_ONLY`. Do not switch to position reduction merely to complete a withdrawal. Replace the example amount and currency with the user's actual values. More complex policies must come from an existing valid request or an explicit downstream contract.

3. Run `capital preview OBSERVATION_ID --file preview-request.json`. Read `id`, `capability`, `valid_until`, `plan_artifact_id`, previewed funds, proposed cancellations, and reduction legs. A preview does not automatically become an exit command.
4. Within the user's authorization, construct a separate START request referencing the preview ID:

```json
{"requestId":"REPLACE_COMMAND_REQUEST_ID","action":"START","previewId":"REPLACE_PREVIEW_ID"}
```

5. Run `capital command OBSERVATION_ID --file capital-command.json`, then query the same requestId through `capital status OBSERVATION_ID`. Preserve the distinct meanings of `PENDING`, `UNCERTAIN`, `ACCEPTED`, and `REJECTED`; `ACCEPTED` is not a completed withdrawal.

If a preview expires or the account revision changes, read status and obtain a new preview as a new operation. Preserve the original file and requestId for a command with an unknown outcome; do not redirect it to another preview.

## Recovery and external withdrawal reconciliation

Non-START actions bind the actual `exitId` and `exit.revision` returned by `capital status`. This `expectedRevision` is neither the PM mandate revision nor account controlRevision.

- `RESUME`: submit `{requestId,action:"RESUME",exitId,expectedRevision,previewId}` with a valid preview.
- `RECONCILE_WITHDRAWAL`: only with actual external transfer facts supplied by the user, submit `{requestId,action:"RECONCILE_WITHDRAWAL",exitId,expectedRevision,amount,currency,externalTransferRef}`. Amount is a string; externalTransferRef is the actual transfer reference.
- Construct other downstream actions only from confirmed action contracts; do not infer `withdraw`, `sell`, or `cancel` CLI subcommands from their names.

Report requested, reserved, released cash, verified withdrawable, and reconciled withdrawal separately. Without withdrawable-funds or transfer evidence, a successful exit code cannot establish that funds arrived.
