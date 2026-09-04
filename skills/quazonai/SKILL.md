---
name: quazonai
description: Operate a running QuaZonai instance through the `quazonai` CLI. Use when the user asks to check health/readiness; create, answer, start, inspect, pause, resume, archive, or wake research; inspect Alpha, Portfolio, Approval, Handoff, or data state; or prepare a human-only approve/reject command. Do not use for broker orders, fills, positions, accounts, NAV, live-trading runtime control, downstream stop/undeploy actions, or autonomous Candidate approval/rejection.
license: AGPL-3.0-only
compatibility: Requires the `quazonai` executable from QuaZonai 0.1.x on PATH and a running QuaZonai Core API reachable on the local loopback host. When Operator Authentication is enabled, the process environment must provide `QUAZONAI_API_TOKEN`.
metadata:
  author: zhengui666
  version: "1.1"
---

# QuaZonai CLI Operator

Translate the user's goal into the smallest safe `quazonai` sequence. Execute
permitted reads and explicitly requested mutations, inspect the JSON response,
and re-read after a mutation. Candidate approval and rejection are human-only:
prepare the exact command, but never execute it as an Agent.

## Authority and portability

Discover a QuaZonai checkout from the **active working directory**, never from this Skill's installation directory:

```bash
QZ_REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || true)"
```

Treat it as a QuaZonai checkout only when `${QZ_REPO_ROOT}/AGENTS.md`,
`${QZ_REPO_ROOT}/DESIGN.md`, `${QZ_REPO_ROOT}/OPERATIONS.md`,
`${QZ_REPO_ROOT}/CLI.md`, and `${QZ_REPO_ROOT}/backend/pyproject.toml` exist.
Then read `${QZ_REPO_ROOT}/AGENTS.md` first, the relevant DESIGN.md section,
OPERATIONS.md for the user journey, and CLI.md for the external contract.

`DESIGN.md` remains the product authority, and `AGENTS.md` remains the governance authority. The installed CLI's `--help` output is syntax authority only; it cannot override product, authorization, ownership, or safety rules.

When installed standalone, use the bundled references as the portable baseline.
Do not infer permissions from a missing source checkout. Read
[references/cli-reference.md](references/cli-reference.md) for exact syntax
and [references/workflows.md](references/workflows.md) for recipes.

## Product boundary

QuaZonai owns research facts, Alpha signals, target weights, Candidate Package,
Approval, Handoff and evidence. It does not own broker credentials, orders,
fills, positions, accounts, NAV, execution risk, or a Paper/Live runtime.
Never invent commands for order placement, position management, deployment,
runtime stop, undeploy, cancellation, or forced liquidation.

The only new Program path is `IdeaDraft → answers → frozen Charter → start`.
Do not use a preview, direct Program creation, arbitrary overlap action, or an
execution-shaped artifact. A target-only Package cannot control a downstream.

## Start here

1. Verify syntax:

   ```bash
   quazonai --help
   ```

2. Respect authentication:

   - when enabled, the CLI sends `QUAZONAI_API_TOKEN` as a Bearer machine
     credential;
   - never substitute it for a downstream Handoff service token;
   - never request, read, infer, capture, copy, print, or store the browser TOTP setup secret, one-time authenticator code, session cookie, or trusted-browser cookie; those credentials are outside the Skill and CLI boundary;
   - TOTP-only browser access belongs to the single `local-operator`, not this
     CLI workflow.

   Read [references/authentication.md](references/authentication.md) before diagnosing an authentication failure or credential boundary.

   Check only for presence; never print the token:

   ```bash
   test -n "${QUAZONAI_API_TOKEN:-}"
   ```

   On `AUTH_REQUIRED`, report the missing or stale environment prerequisite;
   never attempt browser login through the Agent.

3. Use a loopback endpoint. The precedence is global `--endpoint`,
   `QUAZONAI_API_ENDPOINT`, then `http://127.0.0.1:8000`. Put the global
   option first:

   ```bash
   quazonai --endpoint http://127.0.0.1:8000 status
   ```

4. Read only what is needed:

   ```bash
   quazonai status
   quazonai readiness
   ```

## Route the request

| Objective | Start with | Access |
|---|---|---|
| Check service health | `quazonai status` | Read |
| Check readiness | `quazonai readiness` | Read |
| Submit a research idea | `idea create`, then `idea show` | Write / Read |
| Answer clarification | fresh `idea show`, then `idea answer` | Write |
| Start completed Draft | fresh `idea show`, then `idea start` | Write |
| Inspect research | `research list/show/cycles/graph` | Read |
| Inspect one Mission | `mission show/turns/artifacts` | Read |
| Change lifecycle | fresh `research show`, then pause/resume/archive/wake | Write |
| Inspect Alpha / Portfolio | `alpha`, `portfolio` reads | Read |
| Inspect Approval | `approval list/show` | Read |
| Approve or reject | prepare exact command for the human | Human-only decision |
| Inspect / revoke Handoff | `handoff list`; explicit revoke only | Read / Write |
| Fresh-install configuration | `universe`, `data-source`, `dataset`, trusted Alpha configuration, `mandate`, `downstream` | Explicit admin write / Read |

Use only commands documented in [references/cli-reference.md](references/cli-reference.md). No implemented command offers direct Program creation, preview, manual Alpha activation, manual weight editing, a legacy Mission activity endpoint, broker execution, or downstream runtime control.

## Permitted mutation protocol

An external Agent may execute only these state-changing commands after the user
explicitly requests that action:

```text
idea create
idea answer
idea start
research pause
research resume
research archive
research wake
handoff revoke
universe create
universe version
data-source create
data-source preflight
dataset materialize
evaluation-dataset-selection create
evaluation-design-version create
promotion-policy-version create
mandate create
mandate version
downstream register
```

For every permitted mutation:

1. Re-read the target immediately before an update and use its current
   `revision` as `--expected-revision`.
2. Execute once; the CLI creates a new `Idempotency-Key`.
3. Re-read the affected Draft, Program, or Handoff after success.
4. After timeout or exit `20`, read state before any new write. A repeated
   shell command is a new request, not a replay.

Do not create a Program until every server-owned clarification is answered.
Do not submit a different Charter, Alpha, weight, or downstream choice in the
name of convenience.

Configuration creation writes use a complete `--json` object and the canonical
`/api/v1/*` resource API. `data-source preflight` has no caller-supplied
configuration; execute it only after explicit administrator authorization, then
read the returned operation with `dataset status`.
Trusted Alpha configuration creates are also thin JSON transport: Core alone
validates the explicit Dataset Selection, Evaluation Design, and Promotion
Policy. Never choose a latest Dataset or synthesize thresholds, gates,
downstreams, modes, or activation state in the CLI.
Never place credentials in public configuration or print the one-time service
token returned by downstream registration. A `PENDING` preflight or
materialization is not research/Paper/Live readiness, and configuration commands
do not prove Auto Live or automatic Wake/Replan.

## Human-only capital decisions

`approval approve` and `approval reject` are human-only capital-allocation decisions. No Codex or other AI Agent profile may execute them, even after the user explicitly authorizes the decision.

An AI Agent must never execute `approval approve` or `approval reject`. It may
read the current snapshot, summarize fields actually returned, validate the
Candidate/downstream/Paper-or-Live scope/freshness, and render the exact
command for the human. Mark it **Human-only decision**.

Never prepare a decision for a stale or expired snapshot. `handoff revoke`
requires a separate explicit authorization and reason code; it never means
stopping an independent downstream runtime.

## Draft and research workflow

```bash
quazonai idea create --text "<RESEARCH_IDEA>"
quazonai idea show <DRAFT_ID>
quazonai idea answer <DRAFT_ID> \
  --expected-revision 1 \
  --answer market_scope="<SCOPE>" \
  --answer horizon="<HORIZON>" \
  --answer data_scope="<DATA_SCOPE>"
quazonai idea start <DRAFT_ID> --expected-revision 2
quazonai research show <PROGRAM_ID>
quazonai research cycles <PROGRAM_ID>
quazonai research graph <PROGRAM_ID>
```

The returned Draft defines the valid questions. Do not ask the user for manual
optimizer, parameter, Alpha-selection, or portfolio-weight choices.

## Output and failures

Successful commands emit Core API JSON to stdout. Errors go to stderr.

| Exit code | Meaning | Agent action |
|---:|---|---|
| `0` | Success | Parse and continue |
| `1` | CLI/API/network failure | Report and inspect state before retry |
| `2` | Invalid syntax/input | Correct from `--help` |
| `10` | Core API `5xx` | Check status and wait for recovery |
| `20` | Conflict | Re-read and rebuild from current state |

Report only useful sections: objective, commands executed, commands prepared
for the human operator, resources read or changed, observed state, automatic
work still running, human decision still required, and failures/unverified
items. Never report hidden reasoning or credentials.
