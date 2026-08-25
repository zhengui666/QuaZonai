---
name: quazonai
description: Operate a running QuaZonai instance through the `quazonai` CLI. Use when the user asks to check QuaZonai health or readiness; preview, start, inspect, pause, resume, archive, or restore research; inspect Alpha qualifications, Portfolio mandates/programs/candidates, approvals, or handoffs; approve or reject a Candidate; revoke a Handoff; create or inspect data sources; list datasets, universes, or downstream systems; or mentions the QuaZonai CLI or a `quazonai` command. Do not use for broker orders, fills, positions, accounts, NAV, live-trading runtime control, or downstream stop/undeploy actions.
license: AGPL-3.0-only
compatibility: Requires the `quazonai` executable from QuaZonai 0.1.x on PATH and a running QuaZonai Core API reachable on the local loopback host.
metadata:
  author: zhengui666
  version: "1.0"
---

# QuaZonai CLI Operator

Translate the user's operating objective into the smallest safe sequence of `quazonai` commands, execute it, inspect the JSON response, and verify any state change.

This skill is self-contained. Do not require a QuaZonai source checkout or ask the user to provide `AGENTS.md`, `DESIGN.md`, `OPERATIONS.md`, or `CLI.md`. When the installed CLI differs from this skill, stop assuming and use the installed command's `--help` output as the runtime authority.

## Product boundary

QuaZonai is a quantitative **research and portfolio-construction** workbench. It owns research programs, Alpha qualifications, portfolio candidates, approvals, candidate packages, handoffs, and feedback evidence.

It does not own broker credentials, orders, fills, positions, accounts, NAV, execution risk, or a Paper/Live trading runtime. Never invent or invoke commands for order placement, position management, deployment, runtime stop, undeploy, cancellation, or forced liquidation.

## Start here

1. Verify that the CLI is installed:

   ```bash
   quazonai --help
   ```

2. Resolve the Core API endpoint in this order:

   1. an explicit global `--endpoint URL`;
   2. `QUAZONAI_API_ENDPOINT`;
   3. `http://127.0.0.1:8000`.

   The endpoint must use `http` or `https`, have no credentials/query/fragment/path, and use `127.0.0.1`, `localhost`, or `::1`. Remote hosts are intentionally rejected.

3. Place the global option before the resource command:

   ```bash
   quazonai --endpoint http://127.0.0.1:8000 status
   ```

   Do not write `quazonai status --endpoint ...`.

4. Check only what the task needs:

   ```bash
   quazonai status
   quazonai readiness
   ```

   Use `status` for service health. Use `readiness` before a mutation when the user needs to know whether required capabilities are ready.

5. Read [references/cli-reference.md](references/cli-reference.md) before constructing an unfamiliar command. Read [references/workflows.md](references/workflows.md) for multi-step operating recipes.

## Route the request

| User objective | Start with | Access |
|---|---|---|
| Check service health | `quazonai status` | Read |
| Check operational readiness | `quazonai readiness` | Read |
| Preview an Idea without creating a Program | `quazonai idea preview --text "..."` | Preview |
| Start research | preview, then `quazonai research start --idea "..."` | Write |
| Inspect or follow research | `research list/show/activity/missions` | Read |
| Pause, resume, archive, or restore research | fresh `research show`, then the requested lifecycle command | Write |
| Inspect Alpha qualifications | `alpha list/show` | Read |
| Inspect portfolio state | `portfolio mandates/programs/candidate` | Read |
| Inspect approval snapshots | `approval list/show` | Read |
| Approve or reject a Candidate | fresh `approval show`, then `approval approve/reject` | Capital decision |
| Inspect or revoke Handoffs | `handoff list`; revoke only on explicit request | Read / Write |
| Inspect or create data sources | `data-source list/create` | Read / Write |
| Inspect Administration inventories | `datasets`, `universes`, `downstreams` | Read |

Use only commands listed in [references/cli-reference.md](references/cli-reference.md). In particular, no implemented CLI command exists for `handoff show`, `feedback show`, manual Alpha activation, manual portfolio weight editing, broker execution, downstream runtime stop, or plugin administration.

## Read operations

Execute read operations directly when they answer the user's request. Use IDs returned by the CLI; never guess an ID from a name or reuse an ID from old conversation state without re-reading the relevant list.

For broad requests, narrow progressively:

```bash
quazonai research list
quazonai research show <PROGRAM_ID>
quazonai research activity <PROGRAM_ID>
quazonai research missions <PROGRAM_ID>
```

Do not dump unrelated resources merely because they are available.

## Mutation protocol

Treat these as state-changing commands:

```text
research start
research create
research pause
research resume
research archive
research restore
approval approve
approval reject
handoff revoke
data-source create
```

For every mutation:

1. Require an explicit user request for that action. Do not turn “show me” or “what would happen” into a write.
2. For an update, decision, or revoke, re-read the target immediately before changing it. For a creation, inspect the relevant preview/list first to avoid duplicates.
3. Check that the current state and identifiers still match the user's instruction.
4. Execute the command once. The CLI generates a new `Idempotency-Key` for each invocation.
5. Re-read the affected resource after success and report the observed state, not the intended state.
6. When a request times out or the result is ambiguous, re-read state before considering a retry. A blind retry uses a different idempotency key and can duplicate an operation.
7. On exit code `20` / HTTP `409`, never retry unchanged. Re-read current state and explain the conflict.

### Capital decisions

`approval approve`, `approval reject`, and `handoff revoke` require explicit authorization for the specific resource in the current interaction.

Before approval, show or confirm:

- Approval ID and current state;
- Candidate identity exposed by the Approval Snapshot;
- target downstream system ID;
- Paper versus Live scope when present;
- validity/freshness information and material warnings returned by the API.

Never substitute a different Candidate or downstream system. Never approve a stale/expired snapshot. Paper approval is not Live authorization.

Before rejection or revocation, use only a reason code supplied by the user or exposed by QuaZonai. Put explanatory prose in `--note` only for Approval rejection; `handoff revoke` has no note argument. Do not fabricate a reason code.

## Idea and research workflow

Always preview a new Idea first unless the user explicitly instructs you to skip the preview and immediately start it:

```bash
quazonai idea preview --text "<RESEARCH_IDEA>"
```

Summarize the returned Charter/clarifications/overlap recommendation. Do not introduce manual optimizer, model, parameter, Alpha-selection, or portfolio-weight questions that QuaZonai did not request.

Start only with the user's intended Idea text and overlap choice:

```bash
quazonai research start \
  --idea "<RESEARCH_IDEA>" \
  --overlap-action recommended
```

Accepted overlap actions are:

```text
recommended
new-program
independent-program
```

After creation, follow the returned Program ID with `research show`, `research activity`, and `research missions`. Do not submit duplicate Programs merely because autonomous work is still running.

`research create "<IDEA>"` is an implemented alternate form of `research start --idea "<IDEA>"`; prefer `start` for new automation because its required option is visually explicit.

## Output and failures

Successful commands print the Core API response as indented JSON to stdout. Errors are written to stderr.

| Exit code | Meaning | Agent action |
|---:|---|---|
| `0` | Success | Parse the response and continue |
| `1` | Other CLI/API/network failure | Report the error; inspect state before retry |
| `2` | Invalid command usage/input | Correct the command from `--help`; do not retry unchanged |
| `10` | Core API returned a server-side `5xx` response | Check `status`; retry only after evidence of recovery |
| `20` | Core API returned `409 Conflict` | Re-read the target; rebuild the action from current state |

Do not parse human prose from stdout. Consume the JSON value. Preserve API error codes and messages verbatim enough to remain actionable, but never expose secrets or credentials.

## Report back

After operating QuaZonai, report:

```text
Objective
Commands executed
Resources read or changed
Current observed state
Automatic work still running
Human decision still required
Failures or unverified items
```

Omit empty sections. Never report hidden model reasoning.
