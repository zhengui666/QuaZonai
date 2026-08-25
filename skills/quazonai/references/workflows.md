# QuaZonai CLI workflows

Use these recipes after reading the operating rules in `../SKILL.md`. Replace placeholders only with values returned by the user or by a fresh CLI read.

## 1. Diagnose connectivity and readiness

Use when the CLI cannot connect, the user asks whether QuaZonai is operational, or a mutation is about to start and readiness matters.

```bash
quazonai status
quazonai readiness
```

Interpret them separately:

- `status` answers whether the Core API and reported services are healthy;
- `readiness` answers whether QuaZonai reports the capabilities required for work as ready.

If the CLI executable is missing and the current directory is a QuaZonai checkout containing `backend/pyproject.toml`:

```bash
python -m pip install ./backend
quazonai --help
```

Without a checkout, report the missing CLI prerequisite instead of guessing an installation source.

If the endpoint is not the default, put it before the command:

```bash
quazonai --endpoint http://localhost:8000 status
```

Do not work around `REMOTE_API_ENDPOINT_FORBIDDEN` with an arbitrary hostname. The local CLI intentionally accepts only loopback endpoints.

## 2. Preview and start a Research Program

### Preview

```bash
quazonai idea preview --text "<RESEARCH_IDEA>"
```

Read the returned preview. Surface:

- the interpreted research objective;
- material clarifications;
- overlap/reuse recommendations;
- any stated data/capability limitation.

Do not create a Program while the preview materially differs from the user's intent.

### Start

Use the overlap action returned or selected by the user:

```bash
quazonai research start \
  --idea "<RESEARCH_IDEA>" \
  --overlap-action recommended
```

Other accepted values:

```text
new-program
independent-program
```

Capture the Program ID from the JSON response, then verify:

```bash
quazonai research show <PROGRAM_ID>
quazonai research activity <PROGRAM_ID>
quazonai research missions <PROGRAM_ID>
```

A Program that has automatic work pending or running should be followed, not submitted again.

## 3. Inspect a Research Program

Start from the list when the user did not provide a freshly verified ID:

```bash
quazonai research list
```

Select only an unambiguous Program from the returned data, then:

```bash
quazonai research show <PROGRAM_ID>
quazonai research activity <PROGRAM_ID>
quazonai research missions <PROGRAM_ID>
```

Report domain state separately from Mission/job activity. A failed Mission does not by itself prove that the research hypothesis failed; preserve the API's failure category and evidence.

## 4. Change a Research Program lifecycle state

Read the Program first:

```bash
quazonai research show <PROGRAM_ID>
```

Run only the action explicitly requested:

```bash
quazonai research pause <PROGRAM_ID> --reason "<TEXT>"
quazonai research resume <PROGRAM_ID>
quazonai research archive <PROGRAM_ID> --reason "<TEXT>"
quazonai research restore <PROGRAM_ID>
```

Add `--reason "<TEXT>"` to `resume` or `restore` only when the user supplied a reason.

Then verify:

```bash
quazonai research show <PROGRAM_ID>
```

Pause/archive affect QuaZonai research only. Never claim that they stop an independent Paper or Live trading runtime.

## 5. Inspect Alpha and Portfolio state

### Alpha qualification

```bash
quazonai alpha list
quazonai alpha show <QUALIFICATION_ID>
```

### Portfolio inventories and Candidate

```bash
quazonai portfolio mandates
quazonai portfolio programs
quazonai portfolio candidate <CANDIDATE_ID>
```

Treat returned Candidate data as immutable evidence. Do not offer unsupported manual commands for selecting Alphas, changing weights, or patching a Candidate.

## 6. Review an Approval Snapshot

List and fetch the current snapshot:

```bash
quazonai approval list
quazonai approval show <APPROVAL_ID>
```

Summarize only fields actually returned by the API, including:

- current Approval state;
- Candidate identity and recommendation;
- Paper/Live scope when present;
- available downstream choices or the requested downstream ID;
- freshness/validity window;
- material warnings and evidence.

Do not infer permission to approve from a request to inspect.

## 7. Approve a Candidate

Proceed only after the user explicitly authorizes the specific Approval ID and downstream system.

Re-read immediately before the decision:

```bash
quazonai approval show <APPROVAL_ID>
quazonai downstreams
```

Execute once:

```bash
quazonai approval approve \
  <APPROVAL_ID> \
  <DOWNSTREAM_SYSTEM_ID> \
  --expected-state PENDING
```

Verify:

```bash
quazonai approval show <APPROVAL_ID>
quazonai handoff list
```

Report the actual observed Approval/Handoff state. `DOWNSTREAM_ACCEPTED`, when present, means the downstream accepted a package contract; it does not prove that a trading runtime is running.

On exit `20`, re-read the Approval. Do not retry with a different Candidate or downstream system.

## 8. Reject a Candidate

Proceed only after explicit authorization and a user-supplied or API-supported reason code.

```bash
quazonai approval show <APPROVAL_ID>

quazonai approval reject \
  <APPROVAL_ID> \
  <REASON_CODE> \
  --expected-state PENDING

quazonai approval show <APPROVAL_ID>
```

Add `--note "<TEXT>"` only when the user supplied explanatory text. Do not invent a reason code from prose when the valid code set is unknown; inspect the API response or ask the user to choose from codes already exposed by QuaZonai.

## 9. Inspect or revoke a Handoff

Inspect:

```bash
quazonai handoff list
```

Locate the Handoff ID and current state in the returned JSON.

Revoke only on explicit authorization:

```bash
quazonai handoff revoke <HANDOFF_ID> <REASON_CODE>
quazonai handoff list
```

There is no `handoff show` command. Use the post-write list to verify.

Do not translate revoke into downstream stop/undeploy. Once a downstream owns runtime activity, QuaZonai is not an execution control plane.

## 10. Inspect or create a Data Source

Read existing registrations first:

```bash
quazonai data-source list
```

Create only from explicit configuration values:

```bash
quazonai data-source create \
  "<NAME>" \
  --provider "<PROVIDER>" \
  --fields "field_a,field_b,field_c"
```

Then verify:

```bash
quazonai data-source list
```

Omit optional flags rather than inventing values. Never request that the user paste provider credentials into chat; this CLI command accepts only name, provider, and field metadata.

## 11. List Administration inventories

```bash
quazonai datasets
quazonai universes
quazonai downstreams
```

These are list operations. The implemented CLI does not provide per-item `show` commands for these resources.

## 12. Recover from an ambiguous mutation

When the command times out, the network disconnects, or exit `1` does not establish whether the Core API committed the mutation:

1. Do not immediately repeat the write.
2. Run `quazonai status` if service availability is uncertain.
3. Re-read the affected resource with the corresponding read command.
4. If the desired state is already visible, report success with the verification evidence.
5. If the prior state is still visible, confirm that the original request still applies, then issue one new mutation.
6. If state is conflicting or advanced, stop and report the current state.

Each CLI invocation creates a fresh idempotency key. Shell-command repetition is not request replay.

## 13. Response template

Use only relevant sections:

```text
Objective
- <what the user asked>

Commands executed
- <exact command with secrets omitted>

Resources read or changed
- <resource type and ID>

Current observed state
- <verified state from the final read>

Automatic work still running
- <Program/Mission/Handoff activity, when present>

Human decision still required
- <specific unresolved decision>

Failures or unverified items
- <exit code, API error code/message, and what was not proven>
```

Never include credentials, tokens, hidden reasoning, or guessed fields.
