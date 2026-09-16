# Fresh-context onboarding evaluation

<a id="run"></a>
## Run scope

Date: 2026-09-17. Runner: native Codex fresh-context subagent `onboarding_acceptance`, with no inherited conversation, using the task's configured model without an override. Candidate: the open-source-foundation branch in [PR #84](https://github.com/zhengui666/QuaZonai/pull/84). No accepted prior model baseline exists, so this is an adoption observation rather than a measured improvement.

The verifier received only the checkout, read-only boundaries and the three task inputs from the [suite](../suite.md#cases); it was explicitly told not to read the expected answers. Tools were limited by instruction to repository reads and harmless checks, with no edits, database/deployment work, accounts, secret access or Git writes. This describes actual procedure, not a claim of a new host-enforced sandbox.

<a id="results"></a>
## Actual results

| Case | Observed answer / evidence | Result |
| --- | --- | --- |
| First contribution | Located README preview command, port 4179, Ctrl+C and fixture lifetime; selected docs/architecture/unit and affected native checks from CONTRIBUTING/Make | Expected navigation and limits identified |
| Architectural ownership | Traced Web → cycle route → Store admission → queue → Worker/Attempt; cited the real atomic-cycle regression and prohibited domain → store production/build dependency | Correct; additionally found two documentation defects below |
| Scoped acceptance | Rejected earlier-Head emoji review as approval; distinguished current task merge from full Issue #62 acceptance and named required next steps | Expected authority/evidence boundary identified |

The verifier independently ran fixed Rust 1.98.1 `cargo metadata --locked --offline --no-deps --format-version=1` (seven workspace members), Make target dry-runs and `git diff --check`, all successfully. Its first bare-rustup attempt failed because the host PATH lacked rustup; the existing isolated toolchain was then used. It did not count the author's browser/build tests as independently executed.

<a id="decision"></a>
## Comparison and review

The verifier found that the architecture trace incorrectly placed Attempt creation in admission, and that the lifecycle link pointed to the directory rather than the actual transaction file. The author corrected both; the verifier reread the corrections, including a final on-disk read of the link. No blocking architecture-test implementation defect was reported. The test's dev/transitive/semantic exclusions were recognized as documented limits.

All three bounded reading cases produced the expected source-backed behavior after those fixes. This is not a human approval, a full agent benchmark, a production run, or proof of an unattended runner. The owner-defined acceptance remains latest-Head GitHub CI and explicit clean Codex review in PR #84. Preserve this case set for workflow changes; an unattended model release still needs an actual selected runner, access/budget and comparative baseline.
