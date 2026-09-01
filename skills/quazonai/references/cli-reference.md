# QuaZonai CLI reference

Use this file for exact implemented syntax. In a QuaZonai source checkout, `DESIGN.md` remains the product authority, `AGENTS.md` remains the governance authority, and `CLI.md` remains the external Skill/CLI contract. The installed CLI's `--help` output is syntax authority only if a later release differs; it never overrides product, ownership, authorization, or safety rules.

This reference documents the command tree implemented by `backend/src/cli/main.py` in QuaZonai 0.1.x.

## Install and verify

From a QuaZonai checkout with Python 3.14:

```bash
python -m pip install ./backend
quazonai --help
```

For an editable development install:

```bash
make install-dev
quazonai --help
```

The console entry point is `quazonai`.

## Invocation grammar

```text
quazonai [--endpoint LOOPBACK_URL] <resource> [<action>] [arguments]
```

Endpoint precedence:

1. global `--endpoint`;
2. `QUAZONAI_API_ENDPOINT`;
3. `http://127.0.0.1:8000`.

Valid endpoint examples:

```text
http://127.0.0.1:8000
http://localhost:8000
http://[::1]:8000
https://localhost:8443
```

The URL must not contain credentials, a query, a fragment, or a path other than `/`. The CLI rejects non-loopback hostnames with `REMOTE_API_ENDPOINT_FORBIDDEN`.

Because `--endpoint` is a global option, place it before the resource:

```bash
quazonai --endpoint http://localhost:8000 readiness
```

## Machine authentication

When QuaZonai Operator Authentication is enabled, the CLI reads `QUAZONAI_API_TOKEN` from its process environment and automatically sends:

```text
Authorization: Bearer <QUAZONAI_API_TOKEN>
```

The token must be 32–4096 RFC 6750 `b64token` ASCII characters; whitespace, CR/LF, control characters, non-ASCII, and other punctuation are invalid. Do not put the token in the endpoint URL, command arguments, shell history, output, or documentation. Do not use the Operator TOTP setup secret, browser session cookie, or trusted-browser cookie from the CLI.

A safe shell prerequisite check verifies presence without printing the value:

```bash
test -n "${QUAZONAI_API_TOKEN:-}"
quazonai readiness
```

When authentication is disabled, no machine token is required. `AUTH_REQUIRED` means the protected API did not receive the current configured machine credential. A downstream Handoff service token is separate and must not be replaced by `QUAZONAI_API_TOKEN`.

## Implemented command inventory

The following table is contract-tested against the `argparse` command tree. Do not add design-stage or imagined commands to an invocation. An implemented command is not automatically authorized for an Agent: `approval approve` and `approval reject` are human-only decisions.

<!-- cli-command-paths:start -->
| Command path | Access | Purpose |
|---|---|---|
| `status` | Read | Read Core API health |
| `readiness` | Read | Read capability/readiness status |
| `idea preview` | Preview | Preview an Idea without creating a Research Program |
| `research list` | Read | List Research Programs |
| `research show` | Read | Read one Research Program |
| `research start` | Write | Create a Research Program from an Idea |
| `research create` | Write | Alternate implemented form for creating a Research Program |
| `research pause` | Write | Pause a Research Program |
| `research resume` | Write | Resume a Research Program |
| `research archive` | Write | Archive a Research Program |
| `research restore` | Write | Restore an archived Research Program |
| `research missions` | Read | List Missions for a Research Program |
| `research activity` | Read | Read activity for a Research Program |
| `alpha list` | Read | List Alpha qualifications |
| `alpha show` | Read | Read one Alpha qualification |
| `portfolio mandates` | Read | List Portfolio Mandates |
| `portfolio programs` | Read | List Portfolio Programs |
| `portfolio candidate` | Read | Read one Portfolio Candidate |
| `approval list` | Read | List Approval Snapshots |
| `approval show` | Read | Read one Approval Snapshot |
| `approval approve` | Human-only decision | Approve an Approval Snapshot for one downstream system; an Agent may prepare but never execute it |
| `approval reject` | Human-only decision | Reject an Approval Snapshot; an Agent may prepare but never execute it |
| `handoff list` | Read | List Handoffs |
| `handoff revoke` | Write | Revoke a Handoff when the domain preconditions permit it |
| `data-source list` | Read | List Data Sources |
| `data-source create` | Write | Create a Data Source registration |
| `datasets` | Read | List Datasets |
| `universes` | Read | List Universes |
| `downstreams` | Read | List Downstream Systems |
<!-- cli-command-paths:end -->

## System

```bash
quazonai status
quazonai readiness
```

## Idea

```bash
quazonai idea preview --text "<RESEARCH_IDEA>"
```

`idea preview` uses a POST endpoint but is a non-creating preview operation.

## Research

```text
quazonai research list
quazonai research show <PROGRAM_ID>

quazonai research start \
  --idea "<RESEARCH_IDEA>" \
  [--overlap-action recommended|new-program|independent-program]

quazonai research create "<RESEARCH_IDEA>" \
  [--overlap-action recommended|new-program|independent-program]

quazonai research pause <PROGRAM_ID> [--reason "<TEXT>"]
quazonai research resume <PROGRAM_ID> [--reason "<TEXT>"]
quazonai research archive <PROGRAM_ID> [--reason "<TEXT>"]
quazonai research restore <PROGRAM_ID> [--reason "<TEXT>"]

quazonai research missions <PROGRAM_ID>
quazonai research activity <PROGRAM_ID>
```

The default overlap action is `recommended`.

`start` and `create` call the same creation operation with the same Idea and overlap fields. Prefer `start` in new recipes; retain `create` only when the caller intentionally uses its positional Idea form.

## Alpha

```bash
quazonai alpha list
quazonai alpha show <QUALIFICATION_ID>
```

## Portfolio

```bash
quazonai portfolio mandates
quazonai portfolio programs
quazonai portfolio candidate <CANDIDATE_ID>
```

There is no implemented command for manual Alpha selection, Candidate patching, or weight editing.

## Approval

The CLI implements the following human commands so the Agent can explain and prepare their exact syntax:

```text
quazonai approval list
quazonai approval show <APPROVAL_ID>

quazonai approval approve \
  <APPROVAL_ID> \
  --downstream <DOWNSTREAM_SYSTEM_ID> \
  [--expected-state <STATE>]

quazonai approval reject \
  <APPROVAL_ID> \
  --reason <REASON_CODE> \
  [--note "<TEXT>"] \
  [--expected-state <STATE>]
```

The default expected state is `PENDING`.

`--downstream` and `--reason` are required named options, matching the canonical CLI contract. Do not rewrite them as positional arguments.

An AI Agent must never execute `approval approve` or `approval reject`. It may run `approval list/show`, validate the current snapshot, and render the exact decision command for the human operator.

## Handoff

```bash
quazonai handoff list
quazonai handoff revoke <HANDOFF_ID> --reason <REASON_CODE>
```

There is no implemented `handoff show` command. Verify a revoke by listing Handoffs again and locating the returned ID.

QuaZonai does not expose downstream runtime stop, undeploy, order cancellation, or position-closing commands.

## Data and Administration inventories

```text
quazonai data-source list

quazonai data-source create \
  "<NAME>" \
  [--provider "<PROVIDER>"] \
  [--fields "field_a,field_b,field_c"]

quazonai datasets
quazonai universes
quazonai downstreams
```

`--fields` is a comma-separated string. The CLI trims whitespace, removes empty entries, and sends the resulting list.

## Output contract

On success, the CLI prints one indented JSON value to stdout. It may be an object, array, string, or `null`, depending on the Core API response.

On failure, the CLI writes an actionable message to stderr. API failures are formatted as:

```text
<ERROR_CODE>: <MESSAGE>
```

Exit codes:

| Code | Meaning |
|---:|---|
| `0` | Success |
| `1` | Other CLI, client, network, or non-conflict API failure |
| `2` | Command-line usage/input failure |
| `10` | Core API `5xx` failure |
| `20` | Core API `409 Conflict` |

`AUTH_REQUIRED` is an exit-code `1` authentication/configuration failure. Verify that the current process has `QUAZONAI_API_TOKEN` without printing it. Never fall back to browser credentials or a downstream service token.

## Mutation mechanics

The CLI automatically creates a fresh UUID `Idempotency-Key` for each mutation invocation. It does not expose a flag for reusing that key. For Agent-permitted mutations:

- execute a requested mutation once;
- after timeout/connection ambiguity, read current state before retrying;
- never assume rerunning the same shell command is the same idempotent request;
- on `409`, read the target and rebuild the action from current state.

These mechanics describe the CLI; they do not authorize an Agent to execute the human-only Approval commands.

The CLI is a thin HTTP client. It does not read PostgreSQL, Dataset volumes, Program repositories, Codex state, plugin runtimes, broker accounts, or downstream trading runtimes directly.

## Discover release-specific help

```bash
quazonai --help
quazonai research --help
quazonai research start --help
quazonai approval approve --help
```

Use the narrowest relevant `--help` command when syntax fails. `--help` confirms syntax only. Do not use it to relax governance, probe the Core API with guessed paths, or replace the CLI with ad hoc `curl`.
