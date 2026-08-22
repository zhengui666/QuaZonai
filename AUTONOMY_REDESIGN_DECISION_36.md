# Grill-Me Decision 36: Research Continuation Without Downstream Readiness

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

When a Research or Portfolio Candidate reaches a Paper or Live promotion boundary while the corresponding downstream handoff capability is not ready, QuaZonai continues autonomous research. It does not freeze the entire Program, discard the candidate, or create an approval that the user cannot act upon.

For each Candidate Family, QuaZonai maintains exactly one current internal recommended candidate. A later candidate replaces that recommendation only after passing the versioned `MaterialImprovementPolicy` against the current recommendation.

## Promotion states while readiness is missing

A candidate may enter:

```text
PAPER_CONFIGURATION_REQUIRED
LIVE_CONFIGURATION_REQUIRED
```

These states mean:

- the candidate has not failed;
- the candidate is not yet eligible for the Approval Inbox because the selected capability cannot currently complete a handoff;
- the promotion block is operational or configuration-related, not negative research evidence;
- Administration receives a readiness task;
- the Research or Portfolio Program may continue to schedule non-duplicate, information-bearing Missions;
- no approval snapshot is created until the relevant readiness level is restored and current facts are revalidated.

## Single internal recommendation per Candidate Family

Each Candidate Family maintains a durable pointer to at most one current internal recommended candidate.

```text
Candidate A becomes internal recommendation

Candidate B arrives
├── no material improvement → A remains recommended
└── material improvement    → B replaces A
```

Replacement requires the new candidate to pass all applicable gates, including:

- candidate-family equivalence and lineage checks;
- novelty and duplicate-search checks;
- Search Ledger accounting;
- Evidence Exposure inheritance;
- discovery and independent evaluation requirements appropriate to the promotion stage;
- versioned Material Improvement Policy;
- candidate validity and degradation conditions.

A replaced candidate remains immutable historical evidence. It is never deleted, rewritten or presented as an approval backlog item.

## Continued research behavior

While downstream readiness is absent, QuaZonai may continue:

- exploring new Research Branches that pass the Novelty Gate;
- improving Feature Pipelines, Alpha Models and Calibration;
- performing robustness and replication work;
- qualifying Alpha Library entries;
- conducting Portfolio Assembly and marginal-contribution research;
- incorporating new market data and Forward Evidence that do not require the missing downstream connection;
- entering `COOLING` when no information-bearing next Mission exists.

QuaZonai must not:

- generate disabled or non-actionable approval cards;
- accumulate multiple pending approvals for the same Candidate Family;
- lower Promotion Policy thresholds because handoff configuration is missing;
- repeatedly consume Sealed Evaluation Episodes merely to maintain a queue of alternative candidates;
- treat downstream configuration as Alpha or Portfolio evidence;
- discard a valid candidate solely because no downstream is configured;
- automatically select a downstream system on behalf of the user.

## Sealed Evaluation discipline

The absence of downstream readiness does not create an unlimited justification for additional Promotion Evaluations.

QuaZonai may allocate a new Sealed Episode only when the candidate or evidence has materially changed and the evaluation would produce legitimate information value under the normal Promotion Policy. It must not use protected evidence simply to rank a backlog of candidates that cannot yet be handed off.

All disclosures and exposures continue to propagate through the Evidence Exposure Graph.

## Readiness restoration

When `PAPER_HANDOFF_READY` or `LIVE_HANDOFF_READY` is restored, QuaZonai does not automatically create an approval from an old blocked candidate.

It must re-check the then-current internal recommendation against:

- evidence freshness;
- Candidate and Alpha Library lifecycle state;
- current Material Improvement Policy;
- global approval throttling;
- current independent-evaluation requirements;
- approval validity policy;
- selected downstream compatibility and preflight;
- Candidate Package and Feedback Contract versions;
- any new Forward Evidence or invalidation event.

Only if the candidate remains qualified does QuaZonai create a new immutable Approval snapshot.

## User experience

Research Observatory may show:

```text
A Paper-ready internal candidate exists.
Paper handoff is not configured, so no approval has been created.
Autonomous research continues and the system will retain only the strongest materially improved recommendation for this candidate family.
```

Administration shows the missing readiness action. The Action Center must not display an approval until the user can make a valid decision and the selected downstream system can pass compatibility preflight.

If research produces no material improvement, the Program may enter `COOLING` normally. Missing downstream configuration does not force it to remain active.

## Product boundaries

- The normal user journey remains limited to proposing Ideas and deciding actionable Candidate approvals.
- Downstream configuration remains an Administration responsibility.
- QuaZonai does not start, stop, inspect or schedule downstream Paper or Live runtimes.
- No internal recommendation implies that a downstream deployment exists or is running.
- No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.
