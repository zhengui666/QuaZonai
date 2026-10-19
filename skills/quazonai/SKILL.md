---
name: quazonai
description: Operate an existing QuaZonai research workbench through its installed HTTP CLI. Submit finite factor studies, inspect trials and independent evidence, build optional shared-account portfolios/backtests, and observe target-only downstream/account bindings. Not a development or deployment workflow.
compatibility: Requires the installed quazonai CLI, Node 22.19+ and a saved owner-device login to a compatible /api/v3 service.
---

# Operate QuaZonai

Use the installed `quazonai` executable and the user's requested scope. No repository checkout, service startup or model inference probe is needed to connect.

## Connect

Run `quazonai identity`. Without a login, have the user run `quazonai login --origin THEIR_ORIGIN` in their own terminal; password input is hidden. Never collect passwords in chat or read saved tokens. New-instance password setup belongs in the authentication UI. See [connection](references/connection.md).

## Idea → trials → optional consumers

| Intent | Commands / reference |
| --- | --- |
| Choose an actual configured model and dataset | `research models`, `datasets` |
| Submit a finite goal or an original request file | [Research](references/research.md) |
| Read trials, selection, failure and independent result | `research get ID`; [evidence and portfolios](references/results.md) |
| Observe or stop the original study | [Bounded observation](references/runs.md) |
| Inspect original atomic factors and conditional capacity | `factor list`, `factor get ID`, `factor capacity ID` |
| Build a requested shared-account portfolio/backtest | `portfolio build --file FILE`, `portfolio list`, `portfolio get ID` |
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
- Keep all failed/rejected trials, the compared set, selection and original criteria. Computation is not independently evaluated evidence, qualification or profitability. Never tune against final held-out values or relabel exposed samples as unseen.
- Missing/stale values stay unavailable, not zero. Real portfolio equity must come from a shared native account, never summed factor curves. A configured source alone proves no licensing, PIT, data quality or usable account.
- Model credentials stay in the service's private native credential store or configured host environment. Use Settings for official provider login; never ask for credentials in chat. Model availability is metadata/auth configuration, not a billed test. Never disclose secrets, hidden reasoning or raw Sealed values.
- Paper/Live describe observed source bindings. QuaZonai is target-only: no broker orders, trading launch or funds transfer. Optional original-V1 capital controls require advertised downstream capability and the user's specific request; a preview or generic development request does not authorize initiating a reservation or external withdrawal. A target ACK is not a fill; local exit or a stop receipt does not prove remote cancellation.

Return the original IDs, observed state, actual evidence/limitations and next required action. Do not expand authority or start optional portfolio/observation work merely because factor research completed.
