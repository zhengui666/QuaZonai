# Agent research guidance in the service Skill

**Source:** User request to improve the QuaZonai Skill beyond CLI usage, including how agents create research and write questions, rationales and original goals; follow-up explicitly requests merging into remote `dev`.
**Task ID:** `skill-agent-research-guidance`.

## Scope and acceptance

Improve the portable [service Skill](../../../skills/quazonai/SKILL.md) and its procedures: native field mapping, writing templates/examples, create/reuse decisions, registered data/model/budget consistency, evidence interpretation, recovery and agent handoff. Keep current commands and operational boundaries. Do not change runtime code or submit live research.

The delivery branch starts at remote `dev` `a233b966`. The initial draft was prepared against older `main` `d7da6bff` and retained locally at `90b629d8`; its Rust CLI tests and field mappings do not establish acceptance for the current TypeScript workbench. This version adapts the intended guidance to `ResearchStart`, `specification`, `requestId` and current study/trial semantics. It does not restore the retired Project/Brief/Cycle/Mission command surface or a nonexistent research preview.

Acceptance: a new agent can prepare a supported request, preserve the original goal without inventing fields/evidence, identify current prerequisites, submit once within scope and distinguish execution, independent evidence and optional consumer operations. Documentation and applicable current checks must pass, with independent final-Head review and GitHub Codex review before merge to `dev`.

## Verification and delivery

Current local verification on the `dev` baseline:

- `make check-ts`: three type checks and 249 tests passed (Service 114, Research 128, CLI 7), none skipped. Runtime source is unchanged by this documentation patch. Local Node is 24.19.0; remote CI uses 22.19.0.
- The complete ResearchStart example passed the current `researchSpecification` parser, exact question/goal equality and request-field/budget checks. CLI help and five referenced schema discoveries passed offline; three Shell blocks passed `bash -n`.
- All 31 links in the ten changed Markdown files passed. An isolated copy of the Skill resolved all 25 internal links across nine Markdown files without a checkout. `git diff --check` passed.
- Full-repository `make check-links` already fails on baseline `a233b966`: 59 errors in 18 unchanged historical documents, referring to deleted Rust/Web files or obsolete deployment anchors. This is not reported as a full-repository pass. The changed-file and standalone checks isolate this patch's result.
- Independent review checked the current TS CLI, contracts, service/research implementation and every Skill reference. It identified incomplete capital-command discovery after removing a repository-only link; the reference now includes the actual camelCase wrapper fields for preview/start/pause/cancel/resume/reconciliation and distinguishes missing native policy schemas. The retired claim that no capital commands exist was corrected.

No live service writes, paid model evaluation, scientific experiment or account operation was performed. Deterministic/source review is not model-quality evidence. Final-Head independent confirmation, applicable GitHub CI, Codex review and merge receipts belong to the PR; merge remains pending until those gates pass.
