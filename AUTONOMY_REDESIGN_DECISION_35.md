# Grill-Me Decision 35: Staged Product Readiness

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai uses staged readiness. The first-run experience requires only enough configuration to reach `RESEARCH_READY`; Paper and Live downstream handoff readiness are configured independently and may be completed later.

This preserves the product boundary that research, Paper handoff and Live handoff are separate capabilities. A missing downstream connection must not prevent autonomous research from starting and must not be interpreted as research failure.

## Readiness levels

```text
SYSTEM_READY
→ RESEARCH_READY
→ PAPER_HANDOFF_READY
→ LIVE_HANDOFF_READY
```

The levels are capability states, not one mandatory linear wizard. `PAPER_HANDOFF_READY` and `LIVE_HANDOFF_READY` may be achieved independently after `RESEARCH_READY`, subject to their own prerequisites.

### SYSTEM_READY

The installation can run its control plane and persist business state.

Required conditions include:

- PostgreSQL and schema are available;
- required persistent storage is writable;
- background orchestration services are healthy;
- application configuration is valid;
- evidence and artifact storage have sufficient basic capacity.

`SYSTEM_READY` alone does not allow a Research Program to start.

### RESEARCH_READY

The operator may submit an Idea and start an autonomous Research Program.

Required conditions include:

- `SYSTEM_READY`;
- Codex App Server is reachable through the supported local integration;
- Codex authentication is valid;
- a supported model is available;
- Program repositories and temporary Mission worktrees can be created;
- at least one Discovery Dataset or approved data-ingestion capability is available;
- the canonical Research Engine passes a minimal execution preflight;
- the scheduler has at least one admissible Mission execution slot;
- the Sealed Evaluator boundary is configured sufficiently to prevent Codex access to protected evaluation data, even when no sealed episode is currently available.

Once `RESEARCH_READY` is reached, the user may propose an Idea and the system may create Programs, Missions, Alpha candidates and Portfolio research.

### PAPER_HANDOFF_READY

QuaZonai may generate a `PAPER_HANDOFF_APPROVAL` for a compatible candidate and selected downstream system.

Required conditions include:

- at least one enabled Paper downstream system;
- supported Candidate Package contract version;
- supported Feedback Contract version;
- successful downstream compatibility and claim preflight;
- valid downstream identity and handoff registry configuration;
- ability to receive and validate Paper feedback packages.

A candidate that otherwise reaches the Paper promotion gate while this readiness is absent enters a configuration-blocked promotion state rather than failing research.

### LIVE_HANDOFF_READY

QuaZonai may generate a `LIVE_HANDOFF_APPROVAL` for a compatible candidate and selected downstream system.

Required conditions include:

- at least one explicitly enabled Live downstream system;
- supported Live Candidate Package and Feedback Contract versions;
- successful Live compatibility and claim preflight;
- administrator acknowledgement that the downstream connection is intended for Live handoff;
- complete and valid Paper Forward Evidence satisfying the approved Paper feedback contract;
- all Live promotion dependencies and approval-validity policies are current.

Live readiness never grants QuaZonai execution, credential, deployment, order or runtime-control authority.

## First-run experience

The initial setup flow must prioritize reaching `RESEARCH_READY`:

```text
Storage and database
→ Codex App Server and authentication
→ Research dataset or ingestion capability
→ Research Engine preflight
→ Ready to propose an Idea
```

Paper and Live integrations are offered as later setup stages. The user is not forced to configure a trading or simulation system before evaluating QuaZonai's research value.

The product must not present downstream configuration as part of the normal Idea workflow. It remains an Administration responsibility.

## Candidate behavior when downstream readiness is absent

Research continues normally. A qualified candidate may be retained as an immutable internal candidate, but QuaZonai must not create an approval that cannot be acted upon.

Recommended promotion states include:

```text
PAPER_CONFIGURATION_REQUIRED
LIVE_CONFIGURATION_REQUIRED
```

These states mean:

- the candidate has not failed;
- the candidate is not yet in the Approval Inbox;
- the relevant Promotion Gate is blocked by product configuration rather than research evidence;
- Administration receives a clear readiness task;
- the Research Program may continue exploring other non-duplicate Branches;
- global approval throttling and the Material Improvement Gate still apply after readiness is restored.

Restoring readiness does not automatically publish an old candidate. QuaZonai must re-check evidence freshness, approval validity dependencies, downstream compatibility and material improvement before creating a new Approval snapshot.

## User experience

The home page and Administration area expose readiness separately:

```text
Research: Ready
Paper handoff: Not configured
Live handoff: Not configured
```

The product may state:

```text
You can start autonomous research now.
Configure a Paper downstream system before a candidate can be submitted for Paper handoff approval.
```

Missing downstream configuration must not be displayed as a system-wide failure when research remains available.

## Failure isolation

- Loss of Paper or Live readiness does not stop active Research Programs.
- Loss of Codex or Research Engine readiness blocks new Missions and may move affected Programs to `BLOCKED`, but does not alter existing evidence or candidates.
- Loss of a downstream connection does not classify a candidate as failed.
- Restoring any readiness level requires fresh preflight evidence rather than a manual boolean toggle.
- QuaZonai does not start, stop or inspect downstream runtime processes while establishing readiness.

## Evidence and identity rules

Readiness uses explicit component states, supported contract versions, preflight records, business IDs, timestamps and field-level capability validation.

No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.
