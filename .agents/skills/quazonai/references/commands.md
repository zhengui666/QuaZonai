# Command index

Every command below omits the common `quazonai` prefix. `ID`, filenames, and `NAME` are placeholders; use actual receipt values. Commands do not automatically resolve resource names: list resources first, then use their IDs.

## Sessions, contracts, and queries

| Command | Purpose |
| --- | --- |
| `login [--origin URL] [--name NAME] [--replace] [--ca-certificate FILE]` | Interactive login or session reuse |
| `identity` / `logout` / `forget` | Read identity, revoke this device, or only forget local login |
| `schema list` / `schema show NAME` | Read business TypeScript contracts |
| `schema list --domain` / `schema show NAME --domain` | Read native science JSON Schema |
| `datasets` | Read datasets, ready states, and research settings |
| `datasets registry` | Read the registration revision and current/retained dataset IDs |
| `datasets register --file REQUEST.json` | Append a new ID only; file contains requestId, expectedRevision, path. The path is a service-readable DatasetConfig JSON file, not a client upload |
| `native-runner status` | Read current environment revision without building or selecting |
| `native-runner import --file REQUEST.json` | Explicitly inspect a trusted executable; requestId, expectedRevision, environmentId, sourceExecutable, and optional expectedIdentity. Runs capabilities but neither connects accounts nor selects the global default |
| `native-runner build-status REQUEST_ID` | Read the original build/import task without repeating execution |
| `research models` | List available models, reasoning levels, and service tiers |
| `usage` | Read usage; `null` costs or token counts are not zero |

## Native research and historical portfolios

| Command | Request or note |
| --- | --- |
| `research list [--cursor JSON]` | Continue pagination with the returned `cursor` object |
| `research get ID` / `research diagnostics ID` | Details, original submission, and execution diagnostics |
| `research messages ID [--cursor CURSOR]` | User goals, continuation instructions, and agent replies for the same research; reuse nextCursor unchanged |
| `research native-runs ID --result-task-id TASK_ID [--offset OFFSET]` | Paginated native metadata for the current result; use final.nativeStrategyResult.resultTaskId from details and returned nextOffset |
| `research native-backtests ID [--cursor CURSOR]` / `research evidence ID TASK_ID` | Native child tasks and a selected task's original request, state, and evidence |
| `research implementation-sources ID --file REQUEST.json [--request-id ORIGINAL_ID]` | Register source text explicitly authorized for the current research model's independent internal logic assessment |
| `research start --file REQUEST.json [--request-id ORIGINAL_ID]` | `ResearchStart` |
| `research start --goal TEXT --dataset ID --model provider:modelId --request-id ORIGINAL_ID` | Simple research; cannot be combined with `--file` |
| `research watch ID [--timeout SECONDS]` / `research wait ID [--timeout SECONDS]` | State stream or wait for settlement; ID is conversationId |
| `research stop ID` | Request a stop, then read back state |
| `research resume ID --submission-id PREVIOUS_TERMINAL_SUBMISSION [--instruction TEXT]` | Use the original terminal submission confirmed by diagnostics |
| `research delete ID` | Hide eligible research while retaining original evidence |
| `portfolio list` / `portfolio get ID` / `portfolio delete ID` | List, read results, or hide portfolios |
| `portfolio watch ID [--timeout SECONDS]` / `portfolio wait ID [--timeout SECONDS]` | Wait for the same portfolio to settle |

The old `portfolio replay-ticks` / `tick-replay` service routes have been removed. Do not use the historical TickEconomicReplayStart type for new requests. Native replay runs through `research`.

## PM, Campaign, and Strategy

| Command | Request or ID type |
| --- | --- |
| `pm capabilities` / `pm list` / `pm get ID` | Capabilities, mandate list, and complete view |
| `pm decision ID` / `pm history ID` / `pm events ID` | Mandate ID; events is a single JSON query |
| `pm recompute ID --file COMMAND.json` | `PmCommand` |
| `pm refresh-data ID --file COMMAND.json` | `PmCommand`; capture a new revision of existing data |
| `pm targets ID --file COMMAND.json` | `PmCommand` plus `decisionId`; explicitly request target delivery |
| `pm retire ID --file COMMAND.json` | `PmCommand`, optional `strategyVersionId`; retire the strategy execution path |
| `pm delete ID` | Hide an eligible mandate |
| `pm cycle ID` | Cycle ID, not mandate ID |
| `pm capacity ID` | Capacity estimate ID, not mandate/portfolio ID |
| `campaign get ID` / `campaign trials ID` / `campaign compare ID` | Campaign ID |
| `strategy versions ID` / `strategy assessments ID` | Queryable strategy version or definition ID |
| `strategy pause ID --file COMMAND.json` / `strategy resume ID --file COMMAND.json` | Current strategy version ID; revision comes from the owning mandate |

## Accounts and capital

| Command | Request or meaning |
| --- | --- |
| `observation start --file REQUEST.json` | `ObservationStart`; bind an existing connection |
| `observation list` / `observation get ID` | All observations or a selected observation |
| `observation refresh ID` / `observation stop ID` | Refresh account facts or stop observation |
| `capital status OBSERVATION_ID` | Refresh and read capital exit and command states |
| `capital preview OBSERVATION_ID --file REQUEST.json` | Preview an exit plan; still requires the original `requestId` |
| `capital command OBSERVATION_ID --file REQUEST.json` | Submit a capital exit action; see the account reference for request fields |

## Settings, extensions, and maintenance

| Command | Request or check |
| --- | --- |
| `settings get` | Read language (`en` default or `zh-CN`), default model, thinkingLevel, serviceTier, and tickBookCores; use language for normal responses unless the user explicitly requests another language |
| `settings set --file SETTINGS.json` | PATCH required settings, then read back with `settings get` |
| `settings set --tick-book-cores COUNT` | Positive safe integer; cannot be combined with `--file` |
| `extensions list` | Read name, enabled, required, state, and error |
| `extensions enable NAME` / `extensions disable NAME` | Use actual names and read back loaded/disabled/pending_restart/error; required extensions may reject disabling |
| `maintenance status` | Read RUNNING/DRAINING/QUIESCED, epoch, and blockers |
| `maintenance enter --file REQUEST.json` | `MaintenanceEnter`: `requestId`, optional `reason` |
| `maintenance resume --file REQUEST.json` | `MaintenanceResume`: new operation's `requestId` and current `epoch` |

An enter receipt does not establish `QUIESCED`. Read `blockingTasks`, `blockingSubmissions`, `blockingExternal`, and `activeBusinessRequests`; do not stop work on your own merely to clear blockers. Resume with the epoch read from status; replay of the original maintenance request retains its epoch and ID. Maintenance commands do not update software or change research conclusions.

## Native grids and account strategies

Request new parameter grids through `research start` or the original research's `research resume`. Page original tasks with `research native-backtests ID [--cursor CURSOR]` and read each group's original request and result with `research evidence ID TASK_ID`. Legacy `parameter-scan list/get/stop/trials/watch/wait` handles historical records only; there are no new options/preview/create/surface interfaces. See [Native research](research-and-portfolios.md) for responsibilities and evidence limits.

Account commands: `account-strategy status OBSERVATION_ID`, `receipt OBSERVATION_ID REQUEST_ID`, `start OBSERVATION_ID --request-id ID --account-source-id SOURCE --expected-revision REV`, and `stop OBSERVATION_ID --request-id ID`. Do not automatically refresh START's source or revision. See [Accounts and capital](accounts.md).

`observation size ID --file SCENARIO.json` and `preview-order ID --file ORDER.json` are read-only capabilities, not order submission. `capital context ID` and `capital get OBSERVATION_ID EXIT_ID` read existing exit information. `observation cancel-protection ID` is explicit account control, not a backtest feature.
