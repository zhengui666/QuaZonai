# Fix native artifact export transport

<a id="task"></a>
## Task

**Task ID:** `fix-artifact-export-transport`

**Source and endpoint:** The user's request to continue small-contract research delivery requires readable native evidence. Complete this prerequisite through final-Head CI, explicit clean GitHub Codex review and merge to remote `dev`, following [review policy](../../review.md).

<a id="intent"></a>
## Intent

The installed official CLI returned `CLI_RESPONSE_CONTRACT_INVALID` when exporting a `qz.data_quality` artifact from a successful native `DATA_VALIDATE` run. Its bytes were not read and its scientific outcome remains unverified. Restore authenticated export of the original artifact bytes while preserving unrelated work, user data and research isolation. This prerequisite does not establish a qualified Alpha or portfolio.

<a id="spec"></a>
## Requirements and design

The published [artifact content route](../../../apps/server/src/artifacts.rs) declares `application/octet-stream` attachments. `ArtifactView.media_type` describes the stored payload; it is not this route's transport MIME. Match the published transport in the shared CLI branch, retaining metadata ID, exact byte count, response bounds, authorization and credential-reflection checks. Keep other JSON, Runtime and historical-import contracts strict.

Both remote `dev` (`5e7f2959e38ac3b9c47676c517a39040343e23a2`) and `main` (`d7da6bff197bcbf272ba998ae0a15b2c1b25071c`) still contain the mismatch. The installed binary matches the Linux asset in [the current dev Release](https://github.com/zhengui666/QuaZonai/releases/tag/v2.0.0-dev.20260928110756.36413797690). Correct the CLI against the existing API; no server or generated-contract change is needed.

<a id="plan"></a>
## Implementation plan

1. Change [shared CLI export](../../../apps/server/src/client/mod.rs) to expect the attachment transport MIME. The portable [CLI entry](../../../apps/cli/src/main.rs) includes this same source; historical downloads already expect octet-stream.
2. Align the existing [transport regression](../../../apps/server/tests/client_transport.rs) with the real route. Cover code, JSON data-quality and binary payloads, plus wrong MIME and byte-count rejection using its existing fixture.
3. Run applicable [project checks](../../project.md#commands), resolve review findings and merge the verified final Head to `dev`. Preserve the primary dirty checkout; author only in the isolated worktree.

<a id="verification"></a>
## Verification

The pre-fix live failure and public Release provenance were observed before this change. The existing server [download regression](../../../apps/server/tests/artifacts_http.rs) already asserts octet-stream, attachment disposition and unchanged bytes. The old CLI fixture used the logical payload MIME and therefore missed this API mismatch.

The shared CLI and regression fixtures have been updated. `git diff --check` and `rustup run 1.98.1 cargo fmt --all -- --check` passed; executable behavior checks are pending. Run from the repository root:

```sh
rustup run 1.98.1 cargo test --locked -p server --test client_transport native_cli_artifact_export
rustup run 1.98.1 cargo test --locked -p quazonai-cli
cargo fmt --all -- --check
make check-docs
```

No local build, test, installation or service change has been performed for this patch. Fixture success will not establish live export recovery or scientific acceptance; those require subsequent native CLI observations.

<a id="review"></a>
## Review

PR, final-Head CI and explicit clean read-only GitHub Codex review are pending. Review the corrected test expectations against the published download route and retained rejection paths.

<a id="delivery"></a>
## Delivery

Merge to remote `dev` is pending. Release, installation and live export verification have not occurred. Small-contract data coverage, funding assumptions, Alpha qualification and portfolio delivery remain separate unfinished research work.

<a id="handoff"></a>
## Handoff

Authoring starts at detached `5e7f2959e38ac3b9c47676c517a39040343e23a2` in the managed `artifact-export-transport` worktree. The coordinator owns branch creation, checks, GitHub delivery and subsequent native export verification.
