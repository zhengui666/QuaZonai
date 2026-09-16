# Production maintenance

<a id="intent"></a>
## Intent

The owner requested a project whose code and documentation stay maintainable and can reach production acceptance. They explicitly authorized local source/document edits and removal of the web-only authorship restriction on 2026-09-16.

<a id="spec"></a>
## Requirements and design

Keep [DESIGN](../../../DESIGN.md) authoritative. Preserve user data, credentials, licenses and unrelated work. Remove obsolete author restrictions and delivery claims, consolidate duplicated guidance, and reuse native checks to detect drift. Complete the cleanup scope in a reviewed PR; production acceptance remains the full [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62) contract and must not be inferred from maintenance CI.

<a id="plan"></a>
## Implementation plan

1. Start from merged main `fae61dd9e130613ef63e1ddaaaefddca5e55b2bb` in an isolated worktree. Keep the original dirty checkout and its other worktree intact; remove the explicitly revoked author restriction from the original checkout as well.
2. Update AGENTS and DESIGN to distinguish follow-up PR delivery from full product acceptance. Replace README/Skill repetition and the historical execution diary with navigation and a current evidence index; preserve detailed contracts in DESIGN and command/operation instructions in their canonical documents.
3. Add the omitted frontend Dependabot entry. Reuse a pinned native Markdown link checker and the existing CLI help traversal. Align Make and CI validation rather than building another workflow engine.
4. Run affected checks and verify negative cases. Review the final diff, request independent GitHub Codex review, and run exact-Head CI before merging. Do not deploy, close #62 or claim real-account/data/restore acceptance without evidence.

<a id="verification"></a>
## Verification

Before edits, the merged main CI, CodeQL, Web and Native Runtime workflows passed. Local Rust 1.98.1 formatting and frontend typechecking passed; a limited relative-file-link inventory found 32 existing targets. These are baseline observations, not validation of this change. The changed-source local checks passed:

- `make check-docs`: pinned lychee 0.24.2 checked the local file/anchor links; the native server help traversal passed without credentials or a database, including migration, recovery and MCP commands.
- Temporary negative fixtures: a valid Chinese anchor returned 0; missing file and missing anchor each returned 2. No exclusions were added to hide either failure. An unquoted illustrative file URI in DESIGN was corrected to code formatting.
- `make check-web`: locked offline npm install, unchanged regenerated client, typecheck, 517 Vitest cases, 5 PWA file cases and the production/PWA build passed. Existing large-chunk warnings remain; the native precache per-file limit is unchanged.
- Rust 1.98.1 `cargo fmt --all -- --check`, `git diff --check`, YAML parsing and the Skill validator passed.

Remote review and exact-Head CI are required after publication; the PR is their authoritative record. No real-account, authorized market-data or user-backup acceptance was run.

<a id="handoff"></a>
## Handoff

Branch: `codex/production-maintenance-20260916`, based on merged main. The original `bug/clean` checkout contains unrelated uncommitted work; the prior delivery worktree also contains an uncommitted DESIGN addition for complete Demo acceptance. Both remain intact. Real accounts, authorized market data and a selected legacy backup are not available as acceptance inputs for this task.

<a id="review"></a>
## Review and delivery

Publish this scoped maintenance branch as a follow-up PR. GitHub owns the current review threads, run results and merge state; no permanent passing status is copied into this document. Production acceptance remains incomplete and Issue #62 must stay open.
