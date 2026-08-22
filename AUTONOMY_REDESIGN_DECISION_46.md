# Grill-Me Decision 46: Degradation Monitoring Boundary

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai operates an independent Degradation Monitoring subsystem that evaluates research validity and portfolio health using Forward Evidence and Downstream Feedback Packages. It does not control downstream execution runtimes.

The boundary is:

- QuaZonai owns research validity monitoring;
- downstream systems own execution runtime, orders, positions and operational safety.

## Monitoring scope

### Alpha health

Monitors signals including:

- predictive quality drift;
- ranking or directional performance degradation;
- calibration drift;
- feature/data distribution drift;
- market regime compatibility changes;
- validity-condition violations.

### Portfolio health

Monitors signals including:

- risk behavior deviation;
- drawdown characteristics;
- correlation structure changes;
- cost assumption deterioration;
- capacity pressure;
- marginal contribution degradation;
- Mandate objective deviation.

## Health states

```text
HEALTHY
WATCH
DEGRADING
INVALIDATED
```

Transitions create research events and may trigger:

- Research Program wake-up;
- new Evaluation Episodes;
- Candidate Revision research;
- Withdrawal or degradation advisories.

They do not directly modify deployed Candidates.

## Prohibited behavior

QuaZonai must not:

- stop downstream runtimes;
- cancel orders;
- close positions;
- modify live Portfolio Candidates in place;
- automatically redeploy a new Candidate;
- interpret infrastructure failures as model degradation;
- use monitoring results to bypass Approval requirements.

## Feedback sources

Monitoring consumes validated:

- ForwardEvidenceEpisodes;
- Downstream Feedback Packages;
- approved market and data revisions.

Only contract-complete and validated feedback may enter formal degradation evidence.

## Product boundary

A degradation event means:

> The research validity or portfolio assumptions require review.

It does not mean:

> QuaZonai has execution authority.

Execution action remains the responsibility of the selected downstream system and operator.

## Identity rules

Monitoring decisions use explicit business versions, evidence records, metrics and lifecycle states.

No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.
