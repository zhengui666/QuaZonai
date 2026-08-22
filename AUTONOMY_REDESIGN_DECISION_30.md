# Grill-Me Decision 30: Research Program Lifecycle Controls

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai allows the single human operator to pause, resume, archive and restore a Research Program. A formally started Program cannot be physically deleted through the business product.

These controls are low-frequency management operations; they do not change the normal product promise that the operator's recurring responsibilities are limited to proposing ideas and deciding Paper/Live candidate approvals.

## State semantics

### Pause

`ACTIVE` or `COOLING` may enter `PAUSED` by explicit human action.

- Stop scheduling new Research Missions.
- Allow a running Mission to reach a defined safe checkpoint, or interrupt it through the Mission lifecycle when immediate suspension is requested.
- Do not reactivate for new data, Forward Evidence, Sealed Episodes or other wake events.
- Preserve the Research Charter, Program repository, Branches, Search Ledger, Evidence Exposure Graph, Evaluation Episodes, Alpha versions, Candidate versions and prior decisions.
- Do not modify or revoke already published downstream Candidate Packages.

### Resume

A human may resume a `PAUSED` Program. QuaZonai then re-evaluates current facts rather than forcing the Program into a predetermined state:

- enter `ACTIVE` when a valid, non-duplicate and information-bearing next Mission exists;
- enter `COOLING` when no current Mission passes the novelty and information-gain gates;
- enter `BLOCKED` when required data or system capability is unavailable.

Resume never resets the Search Ledger, Evidence Exposure, consumed Sealed Episodes, lineage or prior Approval history.

### Archive

A human may archive any non-draft Program.

- Remove it from the active Research Pool and normal dashboard defaults.
- Stop all automatic Mission creation and wake processing.
- Stop producing new candidate Approvals.
- Preserve all business facts and development history.
- Keep Alpha Library qualifications, Candidate Packages, Handoff records and downstream feedback referentially intact.

Archive does not stop, pause or control any independent downstream Paper or Live system that has already claimed a Candidate Package.

### Restore

A human may restore an archived Program. The restored Program inherits all prior lineage, search burden, evidence exposure and consumed Evaluation Episodes. It is never treated as a fresh independent research effort.

### Delete

- An unsubmitted Idea Draft may be deleted.
- Once the Research Charter is frozen and the Program starts, business-level physical deletion is unavailable.
- Historical failures, rejected candidates, Search Ledger entries, Evidence Exposure and Handoff lineage cannot be removed to manufacture fresh evidence or reset research history.

## Product presentation

The UI must clearly distinguish:

- `COOLING`: system-controlled waiting for information;
- `PAUSED`: explicit human override that disables automatic wake-up;
- `ARCHIVED`: removed from active operation but retained as permanent research history;
- downstream runtime status: external to QuaZonai and unaffected by Program lifecycle controls.

Every Pause, Resume, Archive and Restore action is recorded as a durable Program event with actor type `HUMAN_OPERATOR`, reason and timestamp.

## Prohibited interpretations

- Pause or Archive must not be described as stopping Paper or Live trading.
- Restore must not create a new Research Charter or reset evidence history.
- Archive must not delete Git history, Candidate Packages, Approvals or downstream feedback.
- No lifecycle operation may use source-code hashes, checksums, fingerprints or digest gates.
