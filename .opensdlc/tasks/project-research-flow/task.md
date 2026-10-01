# Project research flow

## Scope

Continue the research workbench redesign from PR #160 without changing Rust or scientific contracts. Replace the static principle sidebar with a full-width project collection. Within a project, show its real current Brief and recorded cycle next actions, then connect Briefs, frozen inputs, cycles, runs and Agent evaluation through protected navigation.

## Behavior

- Only `current_brief_id` identifies the current Brief. Reject a mismatched project/Brief identity; do not substitute another available version.
- Reuse the existing project input component with an explicit project ID. The independent data-settings entry remains available. Do not enable SEALED creation or validation through this surface.
- Keep Brief edits, freeze/start requests, input creation and quality validation in their original command owners. Preserve captured bodies, idempotency keys, unknown-outcome retries, archive/active checks, dirty/pending guards and server action permissions.
- Present cycle state, outcome, next action and budget counters as recorded, with no invented completion percentages or scientific qualification.
- Readable full-width records replace narrow Brief and cycle tables. Record IDs and revisions remain inspectable.

## Font and acceptance

The web bundle uses existing system CJK sans-serif fonts with an explicit Noto Sans CJK SC fallback. No remote font request or large font file is added to the PWA. Linux browser acceptance installs the official `fonts-noto-cjk` distribution package through a bounded shared script; it uses SIL Open Font License 1.1. The existing local TTC regular/bold files are about 19.5/20.1 MB and are not copied into the product.

The real browser records `CSS.getPlatformFontsForNode` results for a Chinese heading, rather than claiming a CSS family list proves the selected glyph font. Native screenshots include overview, Brief, project inputs and cycle pages in both themes and three viewports. Synthetic cases cover reference identity, archive gating, server action availability and preventing project/tab changes while input authoring is active.

Run the [web checks](../../project.md#commands), the font script unit tests, and the complete hosted native browser acceptance before publication. Local fixture checks are not evidence of executed Rust requests or actual browser typography. Coordinate the shared font setup call with the parallel Web CI change; do not duplicate its workflow edits.
