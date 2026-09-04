# QuaZonai CLI workflows

Use these recipes after [the operating rules](../SKILL.md). In a source
checkout, its AGENTS.md, DESIGN.md, OPERATIONS.md and CLI.md remain authoritative.

## 1. Diagnose connectivity, authentication, and readiness

When authentication may be enabled, verify only that the machine token exists;
never print it:

```bash
test -n "${QUAZONAI_API_TOKEN:-}"
quazonai readiness
quazonai status
```

`readiness` proves the machine credential is accepted when enabled; `status`
is public health and does not prove authentication. `AUTH_REQUIRED` means the
process lacks the current machine token. Never fall back to a TOTP setup secret,
browser cookie, or downstream Handoff service token.

For a non-default local endpoint, put the global option first:

```bash
quazonai --endpoint http://localhost:8000 status
```

## 2. Create, answer, and start an Idea Draft

Program creation is a three-step write path. First create and read the Draft:

```bash
quazonai idea create --text "<RESEARCH_IDEA>"
quazonai idea show <DRAFT_ID>
```

Use only question keys returned by that Draft. Submit all needed answers using
the revision just read:

```bash
quazonai idea answer <DRAFT_ID> \
  --expected-revision 1 \
  --answer market_scope="<SCOPE>" \
  --answer horizon="<HORIZON>" \
  --answer data_scope="<DATA_SCOPE>"
quazonai idea show <DRAFT_ID>
```

When the Draft is ready, start it with its current revision and inspect the
persisted Program rather than submitting another Idea:

```bash
quazonai idea start <DRAFT_ID> --expected-revision 2
quazonai research show <PROGRAM_ID>
quazonai research cycles <PROGRAM_ID>
quazonai research graph <PROGRAM_ID>
```

## 3. Inspect autonomous work

Start from the list when the Program ID is not fresh:

```bash
quazonai research list
quazonai research show <PROGRAM_ID>
quazonai research cycles <PROGRAM_ID>
quazonai research graph <PROGRAM_ID>
quazonai mission show <MISSION_ID>
quazonai mission turns <MISSION_ID>
quazonai mission artifacts <MISSION_ID>
```

Report Program state separately from data quality, Mission/runtime, Sealed, or
negative research evidence. A failed Mission alone does not prove a failed
Alpha.

## 4. Change Program lifecycle

Read first and take only the explicit requested action with the current
revision:

```bash
quazonai research show <PROGRAM_ID>
quazonai research pause <PROGRAM_ID> --expected-revision 1 --reason "<TEXT>"
quazonai research resume <PROGRAM_ID> --expected-revision 2
quazonai research archive <PROGRAM_ID> --expected-revision 3 --reason "<TEXT>"
quazonai research wake <PROGRAM_ID> --expected-revision 4 --reason "<TEXT>"
quazonai research show <PROGRAM_ID>
```

Pause/archive affect QZ research only. They do not stop an independent
downstream runtime. Wake cannot bypass a Paused or Archived human state.

## 5. Review an Approval Snapshot

Read current state and summarize only returned facts:

```bash
quazonai approval list
quazonai approval show <APPROVAL_ID>
quazonai downstreams
```

Confirm Candidate identity, downstream ID, Paper/Live scope, freshness and
warnings. No AI Agent may execute an approval or rejection. It may render this
for the human operator in a response, not run it:

```text
quazonai approval approve \
  <APPROVAL_ID> \
  --downstream <DOWNSTREAM_SYSTEM_ID> \
  --expected-state PENDING
```

After the human acts, the Agent may re-read the snapshot and Handoff list.

## 6. Inspect or revoke a Handoff

```bash
quazonai handoff list
```

Revoke only with explicit authorization for a specific Handoff and reason.
Then re-read the list:

```bash
quazonai handoff revoke <HANDOFF_ID> --reason <REASON_CODE>
quazonai handoff list
```

Do not translate revoke into stop, undeploy, cancellation, or position control.

## 7. Inspect or configure a Data Source

```bash
quazonai data-source list
quazonai data-source create --json '{}'
quazonai data-source preflight <DATA_SOURCE_ID>
quazonai dataset status <OPERATION_ID>
quazonai data-source list
quazonai datasets
quazonai universes
```

Replace `{}` with the complete canonical Data Source JSON object after explicit
administrator authorization. Never ask a user to paste provider credentials
into chat; public configuration rejects them. Preflight sends no configuration
and is not ready until its returned operation completes. A materialization
request remains `PENDING` until its worker and quality/PIT checks complete.

## 7.1 Configure trusted Alpha facts

```bash
quazonai evaluation-dataset-selection create --json '{}'
quazonai evaluation-dataset-selection list
quazonai evaluation-design-version create --json '{}'
quazonai evaluation-design-version list
quazonai promotion-policy-version create --json '{}'
quazonai promotion-policy-version list
```

The `{}` values are syntax placeholders, not valid payloads. Replace each with
exact, complete governed identifiers and policy fields returned or approved by
Core. Do not choose latest Dataset Revisions or make up thresholds, gates,
downstreams, modes, or activation state; Core validates each immutable version.

## 8. Recover from an ambiguous mutation

When a permitted write times out or exits ambiguously:

1. Do not blindly repeat it.
2. Check `quazonai status` if service availability is uncertain.
3. Re-read the affected Draft, Program, or Handoff.
4. If the desired state is present, report that evidence.
5. If it is not, confirm the request still applies and submit a new mutation.

Each invocation creates a new `Idempotency-Key`; shell repetition is not request
replay. This never authorizes an Agent to make a human Approval decision.
