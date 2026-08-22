# Grill-Me Decision 33: Handoff Offer Revocation Boundary

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai allows the human operator or the system to revoke a Handoff Offer only while it is still unclaimed. Once an independent downstream system has claimed the offer, QuaZonai no longer has revocation, stop, undeploy or runtime-control authority.

This preserves a strict boundary:

- QuaZonai owns research, approval, publication and handoff records;
- the downstream system owns deployment, runtime, orders, positions and execution safety after claim.

## Allowed revocation states

A Handoff Offer may be revoked from:

```text
PUBLISHING → REVOKED
AVAILABLE  → REVOKED
```

Revocation must be atomic with respect to downstream claim. Exactly one of the following may win:

```text
AVAILABLE → CLAIMED
AVAILABLE → REVOKED
```

A revoked offer cannot later be claimed.

## Revocation effects

Revocation:

- prevents the selected downstream system from claiming that offer;
- preserves the immutable Candidate Package;
- preserves the original `APPROVED` decision as a durable historical fact;
- records actor, timestamp, structured reason code and optional operator note;
- does not classify the Candidate as failed;
- does not become negative Alpha or Portfolio evidence;
- does not disclose additional Sealed Evaluation information to Codex;
- does not automatically redirect the Candidate Package to another downstream system;
- does not delete the package, Approval, Handoff history or lineage.

Suggested structured reasons include:

```text
OPERATOR_CHANGED_DECISION
DOWNSTREAM_SELECTION_ERROR
MARKET_CONDITIONS_CHANGED
COMPATIBILITY_CONCERN
CANDIDATE_VALIDITY_CONCERN
SYSTEM_INVALIDATION
OTHER
```

## Re-publication after revocation

A revoked offer is terminal. Re-publication creates a new Handoff Offer.

QuaZonai may create a new offer without a new Approval only when all of the following remain true:

- the original Approval is still `APPROVED` and current;
- the Candidate Package version is unchanged;
- the selected downstream system and connection version are unchanged;
- the Package Contract and Feedback Contract versions are unchanged;
- the compatibility preflight remains valid;
- no dependency has become stale;
- the Approval maximum validity period has not expired.

If any condition is false, QuaZonai must re-evaluate current facts and create a new immutable Approval snapshot before publication.

## Boundary after claim

Once the selected downstream has claimed the offer:

```text
CLAIMED
```

QuaZonai must not expose or imply any of the following operations:

```text
REVOKE_RUNTIME
STOP_DOWNSTREAM
UNDEPLOY
CANCEL_LIVE
CLOSE_POSITION
FORCE_RELOAD
```

QuaZonai may instead publish a non-authoritative `WithdrawalAdvisory` or `DegradationAdvisory` containing:

- the affected Candidate Package version;
- the new research, validity or degradation concern;
- severity and reason category;
- recommended operator action in the downstream system;
- creation time and supporting evidence references allowed by disclosure policy.

An advisory is informational. It does not assert that the downstream stopped, changed positions or acknowledged the recommendation.

## User experience

Before claim, the Handoff view may expose:

```text
Withdraw offer
```

The confirmation must state:

- the offer has not yet been claimed;
- withdrawal prevents future claim of this offer;
- the historical Approval and Candidate Package remain intact;
- no downstream runtime is being stopped.

After claim, the interface must replace withdrawal controls with a clear boundary message:

```text
This package has already been claimed by the selected downstream system.
QuaZonai cannot stop or revoke that independent runtime.
Use the downstream system for deployment or trading actions.
```

## Automatic revocation

QuaZonai may automatically revoke an unclaimed offer when:

- the Approval becomes `STALE`;
- the Candidate Package is invalidated or withdrawn;
- compatibility preflight becomes invalid;
- the selected downstream connection is disabled or materially changed;
- the offer reaches a policy-defined invalidation condition before claim.

Offer expiry remains distinct from revocation:

- `EXPIRED` means the claim deadline elapsed;
- `REVOKED` means an explicit human or system decision ended availability early.

## Evidence and identity rules

Revocation and claim arbitration use explicit offer IDs, business versions, states, database transactions and timestamps.

No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.