# QuaZonai CLI workflows

Use these recipes after reading the operating rules in `../SKILL.md`. Replace placeholders only with values returned by the user or by a fresh CLI read.

When the active working directory is inside a validated QuaZonai checkout, that checkout's `AGENTS.md`, `DESIGN.md`, `OPERATIONS.md`, and `CLI.md` remain authoritative over these portable recipes.

## 1. Diagnose connectivity, authentication, and readiness

Use when the CLI cannot connect, the API returns `AUTH_REQUIRED`, the user asks whether QuaZonai is operational, or a permitted mutation is about to start and readiness matters.

When Operator Authentication may be enabled, verify only that the machine token exists; do not print it:

```bash
test -n "${QUAZONAI_API_TOKEN:-}"
```

Then use a protected read to verify the credential, and query health separately when needed:

```bash
quazonai readiness
quazonai status
```

Interpret them separately:

- `readiness` proves the machine credential is accepted when authentication is enabled and reports whether QuaZonai has the capabilities required for work;
- `status` answers whether the Core API and reported services are healthy, but remains intentionally public and does not prove machine-token authentication.

Authentication interpretation:

- no `QUAZONAI_API_TOKEN` plus `AUTH_REQUIRED`: the CLI process lacks the required environment prerequisite;
- token present plus `AUTH_REQUIRED`: the CLI environment and API likely have different token revisions, or the API received no token;
- never fall back to the Operator TOTP setup secret, browser cookies, or a downstream Handoff service token;
- when authentication is disabled, direct loopback access works without a token.

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

Do not infer permission to approve from a request to inspect. No AI Agent may execute `approval approve` or `approval reject`; these remain human-only capital-allocation decisions.

## 7. Prepare a human Approval command

The Agent may inspect and prepare, but must not execute, the decision.

Re-read immediately before preparing the command:

```bash
quazonai approval show <APPROVAL_ID>
quazonai downstreams
```

Confirm that the Approval is current, the Candidate matches the user's decision, the downstream system ID is exact, the Paper/Live scope is understood, and the snapshot is not stale or expired.

Render this exact command for the human operator to run in their local terminal:

```text
quazonai approval approve \
  <APPROVAL_ID> \
  --downstream <DOWNSTREAM_SYSTEM_ID> \
  --expected-state PENDING
```

Do not execute it through Agent tools or a shell. Do not substitute another Candidate or downstream system.

After the human has run it, the Agent may verify through read-only commands:

```bash
quazonai approval show <APPROVAL_ID>
quazonai handoff list
```

Report the actual observed Approval/Handoff state. `DOWNSTREAM_ACCEPTED`, when present, means the downstream accepted a package contract; it does not prove that a trading runtime is running.

If the human reports exit `20`, re-read the Approval and prepare a new command only from current state.

## 8. Prepare a human Rejection command

The Agent may inspect and prepare, but must not execute, the decision. Use only a reason code supplied by the human or exposed by QuaZonai.

First re-read:

```bash
quazonai approval show <APPROVAL_ID>
```

Render this exact command for the human operator:

```text
quazonai approval reject \
  <APPROVAL_ID> \
  --reason <REASON_CODE> \
  --expected-state PENDING
```

Add `--note "<TEXT>"` only when the human supplied explanatory text. Do not invent a reason code from prose when the valid code set is unknown.

Do not execute the command. After the human has run it, verify read-only:

```bash
quazonai approval show <APPROVAL_ID>
```

## 9. Inspect or revoke a Handoff

Inspect:

```bash
quazonai handoff list
```

Locate the Handoff ID and current state in the returned JSON.

Revoke only on explicit authorization for the specific Handoff and reason code:

```bash
quazonai handoff revoke <HANDOFF_ID> --reason <REASON_CODE>
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

## 12. Recover from an ambiguous permitted mutation

When a permitted mutation times out, the network disconnects, or exit `1` does not establish whether the Core API committed the mutation:

1. Do not immediately repeat the write.
2. Run `quazonai status` if service availability is uncertain.
3. Re-read the affected resource with the corresponding read command.
4. If the desired state is already visible, report success with the verification evidence.
5. If the prior state is still visible, confirm that the original request still applies, then issue one new permitted mutation.
6. If state is conflicting or advanced, stop and report the current state.

Each CLI invocation creates a fresh idempotency key. Shell-command repetition is not request replay.

This retry workflow never authorizes an Agent to execute `approval approve` or `approval reject`.

## 13. Response template

Use only relevant sections:

```text
Objective
- <what the user asked>

Commands executed
- <exact permitted command with secrets omitted>

Commands prepared for the human operator
- <human-only approval/rejection command, if applicable>

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
