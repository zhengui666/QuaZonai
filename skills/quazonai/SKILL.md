---
name: quazonai
description: Operate an existing QuaZonai research workbench through its installed HTTP CLI. Define original goals, testable research questions and evidence-backed rationales; create or continue finite factor studies, inspect trials and independent evidence, build optional shared-account portfolios/backtests, delete requested inactive investment decisions/studies/factors/portfolios, and observe target-only downstream/account bindings. Covers both command usage and agent decision, writing, recovery and handoff standards. Not a development or deployment workflow.
compatibility: Requires the installed quazonai CLI, Node 22.19+ and a saved owner-device login to a compatible /api/v3 service.
---

# Operate QuaZonai

Use the installed `quazonai` executable and the user's requested scope. No repository checkout, service startup or model inference probe is needed to connect.

## Working agreement for agents

1. Establish the requested endpoint: inspect, prepare a local research draft, start a finite study, interpret results, or perform a specifically requested consumer operation. Use existing context and delegated choices; ask only for missing decisions that materially change the question, data, budget or endpoint. Writing a draft does not imply starting computation.
2. Before authoring or revising a study, read [research authoring](references/research-authoring.md) and [research operations](references/research.md). Preserve the user's original intent, distinguish facts from hypotheses, and align the question, registered data, metric, search scope and trial budget. Valid JSON alone is not a usable study specification.
3. Inspect existing studies before creating another. Continue the original study when that meets the request; record why a different question needs new work. Retain failed trials, exposed data and original criteria. Do not use a fresh request ID or relaxed target to erase an inconvenient result.
4. Give each logical write one responsible agent. Helpers can inspect evidence or draft independent parts; the writer reconciles the request and submits once. Handoffs include the original request/file reference, IDs, observed state/time, evidence limits and next action, without credentials.
5. Follow the requested completion condition and report the actual result. A negative or inconclusive finding can complete a study. Factor completion does not authorize portfolio construction, account observation, settings changes or capital actions.

## Connect

Run `quazonai identity`. Without a login, have the user run `quazonai login --origin THEIR_ORIGIN` in their own terminal; password input is hidden. Never collect passwords in chat or read saved tokens. New-instance password setup belongs in the authentication UI. See [connection](references/connection.md).

## Idea → trials → optional consumers

| Intent | Commands / reference |
| --- | --- |
| Choose an actual configured model and dataset | `research models`, `datasets` |
| Write the original goal, question, rationale, exclusions and acceptance criteria | [Research authoring](references/research-authoring.md) |
| Submit a finite goal or an original request file | [Research](references/research.md) |
| Read trials, selection, failure and independent result | `research get ID`; [evidence and portfolios](references/results.md) |
| Observe or stop the original study | [Bounded observation](references/runs.md) |
| Delete specifically requested investment decisions, studies, atomic factors or portfolios | `pm delete ID`, `research delete ID`, `factor delete ID`, `portfolio delete ID`; [deletion and limits](references/deletion.md) |
| Inspect original atomic factors and conditional capacity | `factor list`, `factor get ID`, `factor capacity ID` |
| Build a requested shared-account portfolio/backtest | `portfolio build --file FILE`, `portfolio list`, `portfolio get ID` |
| Inspect a queued/running portfolio, progress and current native curve | `portfolio list`, `portfolio get ID`, `portfolio watch ID [--timeout SECONDS]`, `portfolio wait ID [--timeout SECONDS]`; [progress and partial-result limits](references/results.md) |
| Reassess original portfolio capacity without rebuilding | `portfolio capacity ID --file FILE`; [assumptions and limits](references/results.md) |
| Bind Paper/Live account observation and target receipts | [Observations](references/account-transport.md) |
| Requested account sizing or capital reservation | `observation automatic ID --file FILE`, `capital status/preview/command ID`; [account transport](references/account-transport.md) |
| Inspect/update requested settings or installed extensions | `settings get`, `settings set --file FILE`, `extensions list`, `extensions enable NAME`, `extensions disable NAME` |
| Unknown outcome, conflict or failure | [Recovery](references/recovery.md) |

Use `quazonai --help` and offline `schema list` / `schema show NAME` for unfamiliar inputs; `--domain` selects native scientific schemas when installed. `client` is only a spelling alias, not the retired command surface. Unsupported adapters and missing evidence are explicit failures; never invent routes or commands.

## Operation notes

- Retain the original request file and `requestId`. Replaying the same body/ID obtains original identities; changing the body conflicts. Never make a new key after an unknown outcome.
- Native research IDs are numbers. Portfolio/observation IDs are opaque strings; large clocks, quantities and money remain exact decimal strings.
- HTTP 202 is admission, not settled science or proof of cancellation. `wait` uses the native submission projection; empty tasks, IDLE or a final record alone do not establish settlement. Detaching a watcher leaves admitted work running.
- Requested deletion hides the object from normal lists/details and new selection; it preserves original ledgers, evidence and receipts. It never cancels work, resets a budget/exposure, retires an investment strategy or liquidates positions. Read [deletion](references/deletion.md) before acting.
- Keep all failed/rejected trials, the compared set, selection and original criteria. Computation is not independently evaluated evidence, qualification or profitability. Never tune against final held-out values or relabel exposed samples as unseen.
- Missing/stale values stay unavailable, not zero. Real portfolio equity must come from a shared native account, never summed factor curves. A configured source alone proves no licensing, PIT, data quality or usable account.
- Model credentials stay in the service's private native credential store or configured host environment. Use Settings for official provider login; never ask for credentials in chat. Model availability is metadata/auth configuration, not a billed test. Never disclose secrets, hidden reasoning or raw Sealed values.
- Paper/Live describe observed source bindings. QuaZonai is target-only: no broker orders, trading launch or funds transfer. Optional original-V1 capital controls require advertised downstream capability and the user's specific request; a preview or generic development request does not authorize initiating a reservation or external withdrawal. A target ACK is not a fill; local exit or a stop receipt does not prove remote cancellation.
- For requested settings or extension changes, read the current values/state first, change only the requested scope and read back the result. A configured or enabled extension is not necessarily loaded; report `pending_restart` or `error` as observed, and do not restart the service merely to clear it.

Return the original IDs, observed state, actual evidence/limitations and next required action. For research, say which part of the original goal was answered, whether the evidence supports, rejects or leaves the question unresolved, and how the conclusion depends on the recorded data and criteria. Keep proposed follow-up work distinct from work already authorized or started.
