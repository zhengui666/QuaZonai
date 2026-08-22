# Grill-Me Decision 29: Overlapping Research Ideas

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai detects semantic and domain overlap while the user is still in Idea Composer, recommends whether to reuse an existing Research Program or create a related new Program, and leaves the final choice to the user as part of the same “propose an idea” interaction.

## Decision rules

### Exact or effectively duplicate idea

- Do not create another Research Program by default.
- Record a new `IdeaContribution` against the existing Program.
- Wake the Program or raise its opportunity priority when the contribution provides new context.
- Preserve the existing Research Charter, Search Ledger and Evidence Exposure Graph.

### New testable angle inside the existing Charter

- Recommend extending the existing Program.
- Create a new Research Branch with explicit parent lineage and changed assumptions.
- Reuse applicable datasets, code and evidence.
- Inherit all applicable evidence exposure; a new Branch never resets independent-evidence history.

### Idea outside the existing Charter

- Create a related Research Program with a new immutable Charter.
- Record `related_program_id`, relationship type, reusable evidence and inherited exposure.
- Never widen or rewrite the older Charter.

### User chooses an independent Program despite overlap

- Allow the choice inside Idea Composer.
- Show that the new Program carries duplicate-search and multiple-testing consequences.
- Inherit applicable Evidence Exposure and related Search Ledger context.
- Apply stricter redundancy checks before Alpha Library admission and Portfolio Assembly.

## Product constraints

- Overlap resolution remains part of the Idea submission operation; it does not add a third normal user responsibility.
- QuaZonai must not silently merge two materially different Charters.
- QuaZonai must not let Program duplication manufacture fresh independent evidence.
- Semantic overlap analysis must not use source-code hashes, fingerprints, checksum gates or content-addressed identities.
