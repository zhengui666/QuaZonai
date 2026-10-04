# Fix native artifact export transport

<a id="task"></a>
## Task

**Task ID:** `fix-artifact-export-transport`

**Source and endpoint:** The user's request to continue small-contract research delivery requires readable native evidence. Continue existing PR 135 through applicable final-Head checks, independent native review and merge to remote `dev`, following the current [review policy](../../review.md). The earlier GitHub Codex review requirement recorded below is historical, not an extra gate for this refresh.

<a id="intent"></a>
## Intent

The installed official CLI returned `CLI_RESPONSE_CONTRACT_INVALID` when exporting a `qz.data_quality` artifact from a successful native `DATA_VALIDATE` run. Its bytes were not read and its scientific outcome remains unverified. Restore authenticated export of the original artifact bytes while preserving unrelated work, user data and research isolation. This prerequisite does not establish a qualified Alpha or portfolio.

<a id="spec"></a>
## Requirements and design

The published [artifact content route](../../../apps/server/src/artifacts.rs) declares `application/octet-stream` attachments. `ArtifactView.media_type` describes the stored payload; it is not this route's transport MIME. Match the published transport in the shared CLI branch, retaining metadata ID, exact byte count, response bounds, authorization and credential-reflection checks. Keep other JSON, Runtime and historical-import contracts strict.

The original observation found the mismatch in `dev` (`5e7f2959e38ac3b9c47676c517a39040343e23a2`) and `main` (`d7da6bff197bcbf272ba998ae0a15b2c1b25071c`). That installed binary matched the Linux asset in [the historical dev Release](https://github.com/zhengui666/QuaZonai/releases/tag/v2.0.0-dev.20260928110756.36413797690). The new integration base `1a7aaceda1e5557bfa8f0df8f907eb34b0ce5684` still has the same source mismatch. Correct the CLI against the existing API; no server route or generated-contract change is needed.

<a id="plan"></a>
## Implementation plan

1. Change [shared CLI export](../../../apps/server/src/client/mod.rs) to expect the attachment transport MIME. The portable [CLI entry](../../../apps/cli/src/main.rs) includes this same source; historical downloads already expect octet-stream.
2. Align the existing [transport regression](../../../apps/server/tests/client_transport.rs) with the real route. Cover code, JSON data-quality and binary payloads, plus wrong MIME and byte-count rejection using its existing fixture.
3. Run applicable [project checks](../../project.md#commands), resolve review findings and merge the verified final Head to `dev`. Preserve the primary dirty checkout; author only in the isolated worktree.

<a id="verification"></a>
## Verification

The pre-fix live failure and public Release provenance were observed before this change. The existing server [download regression](../../../apps/server/tests/artifacts_http.rs) already asserts octet-stream, attachment disposition and unchanged bytes. The old CLI fixture used the logical payload MIME and therefore missed this API mismatch.

The original PR authoring record states that its shared CLI/fixtures were updated and `git diff --check` plus `rustup run 1.98.1 cargo fmt --all -- --check` passed at that earlier head. Those are historical checks, not new-base evidence. Applicable commands from the repository root remain:

```sh
rustup run 1.98.1 cargo test --locked -p server --test client_transport native_cli_artifact_export
rustup run 1.98.1 cargo test --locked -p quazonai-cli
cargo fmt --all -- --check
make check-docs
```

No local build, test, installation or service change has been performed for this patch. Fixture success will not establish live export recovery or scientific acceptance; those require subsequent native CLI observations.

<a id="review"></a>
## Review

The original PR's review/CI observations retain their original source identities. For the current refresh, review the corrected test expectations against the published download route and retained rejection paths through the current independent native-review process. No new GitHub Codex review is requested.

<a id="delivery"></a>
## Delivery

Merge to remote `dev` is pending. Release, installation and live export verification have not occurred. Small-contract data coverage, funding assumptions, Alpha qualification and portfolio delivery remain separate unfinished research work.

<a id="handoff"></a>
## Handoff

Original authoring started at detached `5e7f2959e38ac3b9c47676c517a39040343e23a2` in the managed `artifact-export-transport` worktree. The current isolated refresh starts from the actual dev integration identified below. The coordinator owns branch creation, checks, GitHub delivery and subsequent native export verification.

## Current dev integration (2026-10-04)

Refresh only this existing PR on dev `1a7aaceda1e5557bfa8f0df8f907eb34b0ce5684`, tree `bcd66f5f80b72cb25e81c80623b956dd885c59ed`. The original PR's production and transport-test patch applies unchanged. Add one existing-harness HTTP regression for original CODE, PARAMETERS and REPORT bytes, with Unicode, whitespace and decimal spelling retained. It uses the current browser owner session to create fixtures and a current project-scoped RESEARCH_READ machine credential to run the real native CLI over TCP. It does not import the unpublished owner bridge, account relay, saved-profile helpers or scientific branch.

Production behavior changes only the attachment MIME expectation in shared CLI export. No schema, migration, dependency, compiler flag, native image recipe, single-ELF/source-tools packaging or scientific code changes are included. The current server binary and portable CLI share the same implementation.

The three changed Rust files passed the pinned 1.98.1 formatter in edition 2021 and the patch passed `git diff --check`; this is not a full-workspace Cargo check. Required current-candidate checks are the existing artifact-export transport tests, the new exact `client_http::native_cli_artifact_exports_preserve_code_parameters_and_report_bytes` SQL/TCP test, applicable portable CLI tests and final-Head CI. New-base executable verification is still UNRUN. Earlier PR 135 CI/live observations and the separate 733/a8 branch's 9/9 acceptance are regression evidence only, not a pass for this candidate. The final reviewed commit and tree will be recorded before execution and publication.
