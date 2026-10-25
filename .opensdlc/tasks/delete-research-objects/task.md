# Delete investment decisions, research, factors and portfolios

The owner requests deletion through the Web interface and CLI Skill, developed on a new branch from current remote `dev` and merged back after applicable verification and explicit clean Codex review.

PR: [#239](https://github.com/zhengui666/QuaZonai/pull/239).

Base: `1fedd67c` (remote `dev`). Branch: `feat/delete-research-objects`.

## Behavior and acceptance

- Bodyless owner-authenticated DELETE endpoints for mandates, research, atomic factors and portfolios return a persistent `{kind,id,deletedAt}` receipt. Repeating the same deletion returns its original receipt, including after restart.
- Deletion removes the normal owner projection, without erasing scientific trials, exposure, immutable input, requests, targets or account receipts. It does not reclaim scientific budget or cancel remote work.
- Deleted records disappear from ordinary lists/details and new selection/admission. Original command identities stay reserved; replay and resume cannot revive them.
- Active native work and live dependencies reject with explicit conflicts. Research removal hides its derived factors; a factor can be removed independently without changing the trial. Deleted portfolios no longer block deleting their research/factors, while their frozen evidence remains.
- PM removal requires retirement and resolved external obligations. A responsibility-free parked monitor may complete on its native wake; deletion never aborts target reconciliation.
- Final portfolio and observation admission transactions recheck deletion after catalog copies or remote source reads. Research deletion shares the existing native owner-command boundary with start/resume/stop.
- Pending deletion identities survive a same-tab refresh and remain explicitly reconcilable from a deleted detail or list. Only a matching receipt completes reconciliation; a 404 is not a deletion receipt.
- Legacy portfolio dependencies follow frozen selected trials; an unused trial factor can be removed without changing the portfolio evidence.
- Official Ant Design deletion controls remain reachable on mobile, retain conflicts and allow the same identity to reconcile unknown responses. CLI commands and the portable service Skill document these exact semantics.

## Verification and delivery

Real HTTP/Durable SQLite tests cover persistence, dependencies, immutable history, reserved request identities, ownership and recovery. Browser tests exercise production assets against the disposable actual service and controlled scientific protocol; these are software checks, not financial or user-instance evidence. Applicable exact-Head CI and explicit clean read-only Codex review are required before merging to remote `dev`. No user data is deleted by development or verification, and merge does not establish an installed-service deployment.
