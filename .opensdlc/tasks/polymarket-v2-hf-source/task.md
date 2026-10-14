# TimeSeventeen Polymarket v2 source integration

<a id="task"></a>
## Task

**Task ID:** `polymarket-v2-hf-source`

**Original request:** Integrate `https://huggingface.co/datasets/TimeSeventeen/Polymarket-v2` into this project, based on current remote `dev`. The owner selected project capability integration and an isolated worktree, not an import into the existing installation.

**Endpoint:** Reusable source selection, existing acquisition/native preparation, current v3 dataset configuration export, verification and independent review. Remote delivery remains subject to final-Head CI/review and scoped write authorization. No production deployment or full archive download.

<a id="intent"></a>
## Intent

Existing `hf-dataset` and `time-seventeen-v2` support acquisition and conversion, but the publisher has no QZ partition index and current TypeScript service configuration is not the retired Runtime registration workflow. Provide a usable date/token preset and an honest offline handoff without inventing source evidence or duplicating transport/science.

<a id="spec"></a>
## Requirements and design

The approved plan adds `polymarket-v2-hf` to the existing source registry. Expand half-open UTC dates into exact `OrderFilled/YYYY_MM_DD.parquet` files verified against one fixed Hub revision. Preserve exact token strings and original selection intent; downloading remains whole-file. Original native pUSD definitions select local rows. Strict native precision, clocks, deduplication and optional chain corroboration remain unchanged.

Conversion/preparation/export stay offline. Export one `DatasetConfig` fragment from an actually published catalog and original owner-selected factor requests, optionally original compatible held-out/policy/strategy inputs. Reuse TypeScript request normalization rather than duplicating research rules in Python. Do not mutate installation configuration, start research, promote PIT/coverage/permission, expose raw evidence or claim physical catalog freezing. Preserve new-output and failed-artifact boundaries.

<a id="plan"></a>
## Implementation plan

1. Add the source-specific helper and registry capabilities, reusing `hf_dataset`/`snapshot` and the HF native bridge.
2. Add an offline TypeScript exporter and a thin registry invocation, reusing `DatasetConfig`, `iterationRequest`, `partitionBoundaries` and canonical request comparison.
3. Package the helper and exercise installed operator help/inventory; use existing explicit Python/Node container entrypoints, never the retired manager `source` command.
4. Add date/revision/token/provenance/failure/export/Sealed regressions and current service consumption coverage; connect source tests to ordinary checks.
5. Document actual current v3 use, run applicable Python/TypeScript/native/package/link checks, and obtain independent review of the final change.

<a id="verification"></a>
## Verification

Local verification on 2026-10-14 (uncommitted working-tree scope, not final committed-Head/hosted CI acceptance):

| Check | Actual result |
| --- | --- |
| `make check-ts check-deploy` | Passed, including service 54, research 23, CLI 3, Python source 225 (18 preset regressions), package 2 and release-helper 3 tests. Deployment Node checks: 15 passed, 1 existing opt-in real CLI-distribution check skipped; that skipped release scenario is not claimed |
| `make check-science` | Passed: formatting and 469 native tests across 40 targets, including actual public-v2-schema Parquet decoding, preparation/readback, Python→Node export, separate iterative/Sealed Wasmi evaluation, and strict malformed-amount failure |
| `bash deploy/docker/native-build.sh debug …` and `make check-core QZ_JOB_BINARY=…/job` | Passed with matching locally rebuilt executables; connected real factor/held-out, shared-account portfolio and original downstream receipt flow |
| `make check-web` | Passed: typecheck, unit check, production build |
| `npm --prefix apps/web run test:e2e` | First full run: 4 passed, 1 dropdown-overlay click timeout in unchanged observation test. Isolated rerun passed; complete rerun passed all 5 tests without source changes |
| Public HF `plan polymarket-v2-hf` | Passed for `[2026-10-10,2026-10-11)` at `c2a6429292da3a4c6e263c2c84351c77f9bce097`: exactly `OrderFilled/2026_10_10.parquet`, 251,741,536 bytes, `cc-by-4.0`. Token `101` was explicitly a selection-intent test, not evidence that this token exists in that file. No source data partition downloaded |
| Actual local application image | Built with current Python/TypeScript payload and existing compatible native binaries; actual offline Node 22.19 exporter/installed Python-registry suite: 46 passed, none skipped |
| `deploy/docker/smoke.py` with local app/scientific images | Passed: real controlled native factor/held-out, setup/login, original receipt replay, restart/update, stopped SQLite/PostgreSQL backup restore, isolated compiler and packaged preset inventory/help/exporter imports |
| Changed Markdown links/anchors and `git diff --check` | Passed. Whole-repository `make check-links` fails on 59 pre-existing references in 18 unchanged inputs, mostly historical records pointing to retired Rust/Web files; those records were not rewritten |
| Independent non-Codex review | Independent non-Codex read-only recheck completed on the final working-tree source/test snapshot: original confirmed findings addressed; no remaining actionable finding. Reviewer independently reran all 46 exporter and 18 preset tests and `git diff --check`; all passed. This is not committed-Head/hosted CI or production acceptance |

Commands used Rust 1.98.1, locked official dependencies, `CARGO_BUILD_JOBS=2`, debug/test debuginfo disabled and the existing Cargo target cache. Because `/tmp` was nearly full, tests used task-owned `TMPDIR` under ignored `.ai-bridge/polymarket-v2-hf-source/tmp`; no unrelated temporary data/cache was deleted. Node host checks used 26.7; actual image checks used the pinned 22.19 runtime.

Initial regression and recovery evidence: the new registry test first failed before implementation, then passed. The full Python suite initially encountered `ENOSPC` in the existing >128 MiB preservation test; moving only task temporary output to the home filesystem yielded all 223 passes. Genuine native integration initially caught fixture provider/UUID contract mistakes and a real exporter mismatch between import version `0.63.0` and native quality version `nautilus-persistence/0.63.0`. Corrected fixture declarations/UUIDv7 and exporter expectation, added provider/version rejection tests, then reran genuine conversion/preparation/evaluation successfully. No native scientific rejection was relaxed.

Local packaging reused job/source-tools from the existing `bcbef01a2d2ef2129d96a70d3389f239fd4575c2` published application and its existing scientific compiler/kernel payload: `git diff --exit-code bcbef01a… HEAD -- Cargo.toml Cargo.lock rust-toolchain.toml apps/job/src apps/job/Cargo.toml crates runtimes/native` was empty. Host-built binaries require newer glibc than the pinned bookworm application; compatible source-identical payload reuse avoids an unnecessary second native build. Local smoke-only images use consistent base-revision labels for the existing manifest validation, not a new committed revision or release claim. Initial smoke attempts with an invalid local revision string and inconsistent image labels were correctly rejected before installation; corrected local metadata then passed the full smoke. The initial image test used a checkout-only test-script path; the test now explicitly selects the existing checkout or actual installed operator entrypoint, and all 46 image tests pass.

Independent review corrections were verified with focused red/green regressions before full reruns:

- Formal science workflow now installs/caches both service and research dependency trees; its new workflow check first reproduced the missing-service failure.
- Reject native-invalid periods, explicit zero fuel, event-end/cutoff contradictions, unknown fields at every native-request level and duplicate JSON keys (including escaped keys). Invalid originals are never silently repaired or exposed as model-facing extra data.
- Compare source token sets without rewriting original argument order. Bind compatible Sealed preprocessing to identical original conversion starts, BAR interval, first measured event and exact canonical typed native definitions; Sealed source end covers the primary. Private definition text preserves uint64 and Decimal values, using the actual native serializer's omitted-optional-field normalization. It is not added to exported `data`.
- Optional strategy now requires an exact native instrument and existing static policy constraints. Primary measured **and actual conversion** bounds must cover the full final request with correct interval-end/exclusive-end semantics. Prefix-only primary + full Sealed remains research-only; no Sealed values were copied into primary to bypass the current portfolio limitation.
- Reuse the shared preparation publication read once, preserving the mandatory preset and original byte/stat mutation guards. Reuse its already validated typed evidence for the small receipt, and do not calculate hashes. Shared ancestor checks are cached only within a verification pass, with a fresh final pre-link identity/symlink pass.
- Genuine native rerun with an omitted original optional definition field passed. It first caught unsupported Python Decimal serialization in private lineage; a narrowly scoped exact-JSON serializer and precision regression fixed that without coercing decimal values or large timestamps.

All native market records/definitions and controlled model/downstream runs are explicitly synthetic `FIXTURE`/`UNVERIFIED`, not real HF acceptance, model quality, historical PIT, coverage, profit or real-account results. No live service dataset configuration, research admission, production deployment or hosted CI has been performed.

<a id="delivery"></a>
## Delivery

Work is isolated on `feat/polymarket-v2-hf-source`, starting from `bcc8683c325291be85514f2db4020025474328a4`; `git ls-remote origin refs/heads/dev` still reported that revision at final verification. Local implementation, relevant regressions, disposable installed-image smoke and independent working-tree review are complete. Use the [operator guide](../../../runtimes/data/polymarket-v2.md) for explicit selection/preparation/export; the source itself is not scientifically qualified by these tests.

At local implementation delivery, original worktree changes, user data and installations remained untouched; no commit, push, PR, dev merge, production deployment or remote write had been performed. The opt-in CLI-distribution check and pre-existing global historical-link failures remain explicitly unpassed as noted above.

### Authorized remote delivery and installed update

On 2026-10-14, the owner explicitly requested merging into `dev`, waiting for its normal release, then updating the existing local QuaZonai service on port `18080`. This authorizes the necessary commit/push/PR/merge and that installation update; it does not authorize a full HF download, automatic dataset import, research/model invocation, forced queue cancellation, database replacement or unrelated installation changes.

The original installation is schema 4 at `/home/zzy/.local/share/quazonai-ts`, owned by UID 1000, with Web port 18080 and retained PostgreSQL port 55432. Before delivery it runs `v3.0.0-dev.20261013135828.38409387057`, revision `bcc8683c325291be85514f2db4020025474328a4`; liveness returns 204 and no pending deployment operation exists. Do not read or disclose credential/provider/auth contents.

Delivery must check the exact committed PR Head with independent non-Codex review and applicable hosted CI, verify the actual remote merge, then consume the normally published immutable release rather than install a local test image. Use the original installed update manager, preserving root/owner/ports/configuration and original durable state. Its stopped-writer idle inspection and complete recovery point are mandatory; unresolved work must not be bypassed. Hosted outcomes and the installed result remain pending at this authorization checkpoint and will be recorded in GitHub and the delivery report, not claimed from local tests.
