# Isolate Docker native build inputs

## Scope

Local first CI performance batch, based on PR [158](https://github.com/zhengui666/QuaZonai/pull/158)
source `5a6180c98d8e24cbe1b5d98c989769ba2fa14b2b`. The source fix and
PR [152](https://github.com/zhengui666/QuaZonai/pull/152) packaging comparison
remain separate. This batch does not establish a five-minute CI result.
The completed source Container job took 69m13: host native build 26m16,
Docker server 10m55, operator 27m03, and actual smoke 125.642s. The host cache
miss included a runner ImageVersion change from `20260927.320.1` to
`20260920.314.1`; its exact-key binding remains unchanged. Cache scope and
availability still need observation on each hosted baseline/candidate.

The [collector](../../../deploy/docker/native-inputs.mjs) uses the existing pinned
Node image and built-in modules. It copies whole native workspace trees, root
migrations, generated contracts/test fixtures referenced at compile time, Cargo
manifests/lock/toolchain and the native helper/recipe into a fresh directory.
The [Dockerfile](../../../deploy/docker/Dockerfile) copies only that directory
into the Rust producer. Web/docs/licenses/Python packaging changes can therefore
leave the copied native input bytes unchanged. Full-root copying occurs only in
the lightweight collector; actual BuildKit reuse must still be observed.

Workspace layout, Docker context exclusions and platform are intentionally narrow.
New recognized Cargo config, unsupported member/manifest/literal-include inputs
and symlinks fail with an explicit boundary error. Required missing inputs fail. Whole native trees preserve
build scripts and JSON/SQL assets rather than guessing dependencies by extension.
This is the current explicit closure, not a general proof of arbitrary future Rust
or build-script inputs. The narrow literal-include check cannot discover every
dynamic filesystem dependency; new external build inputs require closure review.
Native Cargo commands, features, profile flags, target, pinned images, install
paths and strip operations remain unchanged in the
[executable recipe](../../../deploy/docker/native-build.sh). All acceptance stays.

## Producer and report identity

Native identity hashes sorted paths, file modes and bytes together with the
collector/native Docker recipe, pinned images, platform and executable helpers.
[Cost reporting](../../../deploy/docker/operator_cost.py) recomputes this identity
from the current checkout and checks actual hashes of all four shipped ELF files.
Baseline and candidate application ELFs must also match. Final OCI revision and
version identify current packaging; they are checked independently of native reuse.

Producer/report schema 2 stores `input_sha256`, `recipe_sha256`, `platform`,
`elf_sha256` and `original_native_build_elapsed_seconds`,
`original_disk_before_bytes`, `original_disk_after_bytes`. These are original
operator producer observations, retained unchanged when its layer is reused.
Current total build elapsed time continues to use the external monotonic marks.
Schema 1 historical reports retain their revision-bound meaning; schema 2 never
rewrites an old producer revision or converts old metrics to current timings.
A schema 1 producer cannot satisfy schema 2 current-input verification.

PR 152's strict exact-recipe and ELF comparison is untouched. Its final fixed B
must include the source fix and this batch; both final B/C sides need the same new
recipe/producer schema before its packaging-only comparison is run. Earlier
old-recipe cost reports remain historical and cannot substitute for that pair.

## Validation and outstanding proof

Focused tests cover native/non-native mutations, recipe/packaging identity,
complete projection, missing/unsupported/escaping inputs, original measurements,
stale digests, ELF mismatch and historical-schema rejection. Run with the existing
`python3 -B -m unittest discover -s deploy/docker -p '*_test.py'` entrypoint.

Local helper checks do not compile Rust or run Docker. The full local helper suite
has one environment-blocked legacy-installation fixture: AF_UNIX socket creation
is denied even with a permitted escalation retry. It remains required; no skip
or acceptance change is introduced.

Before a performance or compatibility claim, run a new-recipe cold image build,
the existing real installation/update/recovery and scientific checks, then a warm
build with only non-native source and packaging revision changed. Record actual
server/operator layer reuse, unchanged producer input/ELF hashes and original
measurements, fresh final labels, externally measured elapsed time and every
applicable acceptance result. Cache availability, cold compilation and unrelated
Store/Runtime/browser execution costs still bound whole-PR time.


## Bounded same-builder probe

The Container CI job explicitly enables one
[probe](../../../deploy/docker/native_cache_probe.py) after all existing real
installation/update/recovery checks. Reusable release/dev-image callers default
to disabled. Both normal Docker builds and the probe select the exact builder
name emitted by this job's pinned setup-buildx action; no supplied shared builder
name is introduced. The probe records its setup name/driver/nodes, actual builder
inspection and observed Buildx version.

An owned temporary `git archive` of the asserted actual HEAD receives one unique
root `QZ_NATIVE_CACHE_PROBE_<token>.md` document. It is outside the native closure
and all non-collector image COPY sources; changing those COPY rules fails the
probe's exclusion check. The source digest is recomputed before and after this
single added file. No fake commit or packaging revision is created.

Buildx's supported
[rawjson progress](https://docs.docker.com/reference/cli/docker/buildx/build/#set-type-of-progress-output---progress)
emits the official BuildKit
[SolveStatus schema](https://github.com/moby/buildkit/blob/master/client/graph.go)
through its [JSON encoder](https://github.com/moby/buildkit/blob/master/util/progress/progressui/display.go).
The bounded raw log must show the collector RUN actually executed and both heavy
server/operator RUN vertices completed with `cached: true`. Elapsed time alone
cannot satisfy the probe. Independently measured hashes of all four real ELFs,
sizes, native input/recipe identities and original producer records must match the
actual original candidate. Original candidate tags and cost marks are preserved.

The build is limited to 180 seconds; the full helper has 240 seconds including a
30-second cleanup reserve and process-supervision allowance. A first job step
sets a probe-only deadline 74 minutes later, inside the unchanged 75-minute job
limit with an evidence-upload reserve. Late admission further shortens the probe;
insufficient budget records a blocked execution and fails it. This deadline never
gates any pre-existing acceptance check. Output is capped at 2 MiB. There is no
retry, timeout increase, runner change or relaxed gate.

Cleanup checks owned container IDs and labels, and removes only the probe tag
when its current image matches the known build output. Pre-existing tag collisions
fail without mutation; changed assignments are preserved. Timeout signals only
the owned CLI process group. That does not prove remote BuildKit termination:
uncertain solve state is retained as a distinct cleanup failure, with the owned
context preserved, and cannot become a cache-proof pass. The helper never stops
or prunes the builder. The setup action retains its existing job-end cleanup.

This probe adds actual CI overhead and proves only same-builder warm reuse. It
does not prove fresh-runner GHA cache restoration or whole-PR five-minute CI.
After the first hosted evidence, explicitly decide whether to keep this extra
regression check or remove the trial; do not silently call its overhead a speedup.

Independent review also exercised an exited CLI leader with a surviving child
holding stdout, and a duplicate initial BuildKit vertex after completion. Failure
cleanup now signals and escalates the owned group regardless of leader state.
Only genuine duplicate initial vertices preserve prior completion; a later
unfinished execution, cache conflict or error still fails. Both regressions fail
against the prior implementation and pass after the correction. The final focused
probe/projection/cost suite passes 28 tests; Docker and final-head CI remain unrun
locally, including actual cache reuse and the existing AF_UNIX-dependent check.
