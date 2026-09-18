# Source dependency inventory

## Ownership and reuse

DESIGN B9 requires dependency and license/SBOM evidence. The existing CI Rust job uses
[Syft v1.52.0](https://github.com/anchore/syft/releases/tag/v1.52.0), through the official
[download action](https://github.com/anchore/sbom-action/tree/e22c389904149dbc22b58101806040fa8d37a610/download-syft)
pinned at `e22c389904149dbc22b58101806040fa8d37a610`. Both are upstream Apache-2.0 tools;
this adds no application dependency. Git exports the exact committed inputs; Syft owns
package discovery, dependency relationships and output formats. QZ does not implement
another lockfile parser, SBOM generator, vulnerability scanner or license policy engine.

The scan sees only Cargo.toml/Cargo.lock, apps/web/package.json/package-lock.json and
runtimes/codex/package.json/package-lock.json. It does not scan the checkout's build
outputs, downloaded dependencies, unrelated fixture locks or deleted legacy frontend.
One native invocation writes Syft JSON and SPDX JSON into the existing native CI artifact,
with the original inputs and tool version. Existing jq checks require nonempty output,
the pinned generator and findings from all three lockfiles. A generation/check failure
fails the existing job; there is no new workflow, release upload or dependency-submission
permission. The original build/test checks and timeout remain unchanged.

## Interpretation

This is a **source-lock inventory**, not the dependency set actually linked into a binary,
an OCI image inventory or an installed-host inventory. Optional, test and other-platform
lock entries can appear. Syft's native JSON retains Cargo source metadata; preserve it and
the original lockfiles when interpreting Git revisions or coordinates transformed into
SPDX. Component identifiers are upstream output, not new QZ domain identities.

The [Cargo parser](https://github.com/anchore/syft/blob/v1.52.0/syft/pkg/cataloger/rust/parse_cargo_lock.go)
and [package mapping](https://github.com/anchore/syft/blob/v1.52.0/syft/pkg/cataloger/rust/package.go)
do not reconstruct absent license texts from Cargo.lock. Missing licenses remain unknown;
a successful scan is not permission to relabel them, remove attribution or declare complete
license clearance. Binary/image coverage and redistribution obligations require their own
evidence. [Third-party notices](../../THIRD_PARTY_NOTICES.md#generated-source-inventory)
provide the retrieval command and preserve the original copied-code attribution.

The [CI workflow](../../.github/workflows/ci.yml) is the execution authority. Inspect its
exact commit, actual generator outcome and uploaded files; these instructions alone are
not evidence that a scan ran. The main trade-off is one pinned tool download and a small
lockfile scan in the existing native job, not a new hosted service or runtime dependency.
