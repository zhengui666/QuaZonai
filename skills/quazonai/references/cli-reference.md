# QuaZonai CLI reference

`backend/src/cli/main.py` is the syntax source for this reference. In a source
checkout, `DESIGN.md` remains the product authority and `AGENTS.md` remains the
governance authority. `--help` confirms syntax only; it never overrides product, ownership, authorization, or safety rules.

## Install and endpoint

```bash
python -m pip install ./backend
quazonai --help
quazonai --endpoint http://localhost:8000 readiness
```

```text
quazonai [--endpoint LOOPBACK_URL] <resource> [<action>] [arguments]
```

Only `127.0.0.1`, `localhost`, and `::1` endpoints are accepted. Use global
`--endpoint`, then `QUAZONAI_API_ENDPOINT`, then `http://127.0.0.1:8000`.
When authentication is enabled, `QUAZONAI_API_TOKEN` is a separate machine
credential, never a downstream Handoff service token.

```bash
test -n "${QUAZONAI_API_TOKEN:-}"
quazonai readiness
```

## Implemented command inventory

This table is contract-tested against argparse. Approval commands remain
human-only even though their syntax is implemented.

<!-- cli-command-paths:start -->
| Command path | Access | Purpose |
|---|---|---|
| `status` | Read | Read Core API health |
| `readiness` | Read | Read capability/readiness status |
| `idea create` | Write | Create an Idea Draft |
| `idea show` | Read | Read one Idea Draft |
| `idea answer` | Write | Answer Draft clarifications |
| `idea start` | Write | Start a complete Draft |
| `research list` | Read | List Programs |
| `research show` | Read | Read one Program |
| `research pause` | Write | Pause a Program |
| `research resume` | Write | Resume a Program |
| `research archive` | Write | Archive a Program |
| `research wake` | Write | Submit a bounded Wake |
| `research cycles` | Read | List Research Cycles |
| `research graph` | Read | Read the Mission graph |
| `mission show` | Read | Read one Mission |
| `mission turns` | Read | Read durable Mission turns |
| `mission artifacts` | Read | List Mission artifacts |
| `alpha list` | Read | List Alpha qualifications |
| `alpha show` | Read | Read one Alpha qualification |
| `portfolio mandates` | Read | List Portfolio Mandates |
| `portfolio programs` | Read | List Portfolio Programs |
| `portfolio candidate` | Read | Read one Portfolio Candidate |
| `approval list` | Read | List Approval Snapshots |
| `approval show` | Read | Read one Approval Snapshot |
| `approval approve` | Human-only decision | Human approval syntax |
| `approval reject` | Human-only decision | Human rejection syntax |
| `handoff list` | Read | List Handoffs |
| `handoff revoke` | Write | Revoke an eligible Handoff |
| `universe create` | Write | Create a governed Universe Version |
| `universe version` | Write | Create the next immutable Universe Version |
| `data-source list` | Read | List Data Sources |
| `data-source create` | Write | Register a governed Data Source |
| `data-source preflight` | Write | Request governed Data Source preflight |
| `dataset materialize` | Write | Request Dataset materialization |
| `dataset status` | Read | Read a configuration operation |
| `evaluation-dataset-selection create` | Write | Create immutable Evaluation Dataset Selection |
| `evaluation-dataset-selection list` | Read | List Evaluation Dataset Selections |
| `evaluation-design-version create` | Write | Create immutable Evaluation Design Version |
| `evaluation-design-version list` | Read | List Evaluation Design Versions |
| `promotion-policy-version create` | Write | Create immutable Promotion Policy Version |
| `promotion-policy-version list` | Read | List Promotion Policy Versions |
| `mandate create` | Write | Create a Portfolio Mandate and first Version |
| `mandate version` | Write | Create the next immutable Mandate Version |
| `downstream register` | Write | Register a logical Paper or Live downstream |
| `datasets` | Read | List Datasets |
| `universes` | Read | List Universes |
| `downstreams` | Read | List Downstream Systems |
<!-- cli-command-paths:end -->

## Draft and Program

`IdeaDraft → answers → start` is the only Program creation path. There is no
preview, direct Program creation, restore, activity, or legacy Mission-list
command.

```text
quazonai idea create --text "<RESEARCH_IDEA>"
quazonai idea show <DRAFT_ID>
quazonai idea answer <DRAFT_ID> \
  --expected-revision <REVISION> \
  --answer KEY=VALUE [--answer KEY=VALUE ...]
quazonai idea start <DRAFT_ID> \
  --expected-revision <REVISION> \
  [--title "<TITLE>"]
quazonai research list
quazonai research show <PROGRAM_ID>
quazonai research cycles <PROGRAM_ID>
quazonai research graph <PROGRAM_ID>
```

The server returns Draft question keys and freezes the Charter only after all
required answers exist. Use the current returned revision for every Draft or
Program write:

```text
quazonai research pause <PROGRAM_ID> --expected-revision <REVISION> [--reason "<TEXT>"]
quazonai research resume <PROGRAM_ID> --expected-revision <REVISION> [--reason "<TEXT>"]
quazonai research archive <PROGRAM_ID> --expected-revision <REVISION> [--reason "<TEXT>"]
quazonai research wake <PROGRAM_ID> --expected-revision <REVISION> [--reason "<TEXT>"]
quazonai mission show <MISSION_ID>
quazonai mission turns <MISSION_ID>
quazonai mission artifacts <MISSION_ID>
```

## Alpha and Portfolio reads

```text
quazonai alpha list
quazonai alpha show <QUALIFICATION_ID>
quazonai portfolio mandates
quazonai portfolio programs
quazonai portfolio candidate <CANDIDATE_ID>
```

There is no manual Alpha activation, Candidate patching, Alpha selection, or
target-weight command. Fewer than two eligible Alphas is `INFEASIBLE`, never a
single-Alpha fallback.

## Approval and Handoff

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

`--downstream` and `--reason` are required named options. Do not rewrite them as positional arguments. An AI Agent must never execute `approval approve` or `approval reject`.

```text
quazonai handoff list
quazonai handoff revoke <HANDOFF_ID> --reason <REASON_CODE>
```

There is no downstream runtime-control command. Revocation never stops a
claimed downstream.

## Fresh-install configuration

```text
quazonai data-source list

quazonai universe create --json '<UNIVERSE_CREATE_JSON>'
quazonai universe version <UNIVERSE_VERSION_ID> --json '<UNIVERSE_VERSION_JSON>'
quazonai data-source create --json '<DATA_SOURCE_JSON>'
quazonai data-source preflight <DATA_SOURCE_ID>
quazonai dataset materialize --json '<DATASET_MATERIALIZATION_JSON>'
quazonai dataset status <OPERATION_ID>
quazonai evaluation-dataset-selection create --json '<EVALUATION_DATASET_SELECTION_JSON>'
quazonai evaluation-design-version create --json '<EVALUATION_DESIGN_VERSION_JSON>'
quazonai promotion-policy-version create --json '<PROMOTION_POLICY_VERSION_JSON>'
quazonai mandate create --json '<MANDATE_CREATE_JSON>'
quazonai mandate version <MANDATE_ID> --json '<MANDATE_VERSION_JSON>'
quazonai downstream register --json '<DOWNSTREAM_JSON>'

quazonai datasets
quazonai universes
quazonai downstreams
quazonai evaluation-dataset-selection list
quazonai evaluation-design-version list
quazonai promotion-policy-version list
```

Each `--json` is one complete JSON object validated by the canonical
`/api/v1/*` resource API. It must contain every field required by that
resource's schema; public configuration must not contain credentials.
`data-source preflight` sends `{}` only and returns an operation ID for
`dataset status`; it accepts no URL, endpoint, plugin path, or credential.
`dataset materialize` also returns an operation ID for `dataset status`, and
both the operation's Dataset Revision and its quality/PIT checks begin `PENDING`.
Trusted Alpha configuration creates forward complete JSON unchanged to Core;
they do not choose latest Dataset Revisions or synthesize statistical thresholds,
gates, downstreams, modes, or activation state.
Registration may issue a downstream service token, but the CLI redacts it from
stdout; do not put a token in the request or terminal transcript.

## Output and mutation mechanics

Success prints one JSON response on stdout. Errors print to stderr.

| Code | Meaning |
|---:|---|
| `0` | Success |
| `1` | CLI, client, network, or non-conflict API failure |
| `2` | Command syntax/input failure |
| `10` | Core API `5xx` failure |
| `20` | Core API conflict |

Each mutation uses a fresh `Idempotency-Key`. After a timeout or `409`, re-read
state before a new write. The CLI never reads PostgreSQL, Dataset volumes,
worktrees, Sealed data, secrets, broker accounts, or downstream runtimes.
Configuration creation alone is not fresh-install E2E evidence, and it does
not prove Package-before-Approval, Auto Live, or automatic Wake/Replan.
