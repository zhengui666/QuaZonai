# Installed source tools

## Scope

The owner's approved autonomous dev iteration adds a usable installed entrypoint
for existing free source capabilities. Base:
`20c763fa6a6db327eed705b5f6d69b3e908a9316`. Design, implementation and review remain
native; no Codex design/development/review is used. Draft publication and exact-Head
independent review/CI are coordinator-owned and pending.

Ship the existing Python registry/acquisition modules and two optional native
import/preparation binaries inside the application image, outside the directory
extracted onto the host. Installed `manage.py source` chooses the active immutable
image, queries that image's network capabilities, and exposes only explicit
read-only inputs and a separate output parent at identical host/container paths.
Reserved container tools/system paths and installation/Codex state cannot be
mounted. No arbitrary installed native executable override is accepted.

The existing registry gains native BAR preparation and an original-byte metadata
digest, Runtime registration paths and identity-only handoff. Native code remains
the source of metadata validation, measurement, readback and publication. No
declaration, fee, calendar, permission, PIT status or research qualification is
invented. UI hints, new services, registry/schema duplication and native diagnostic
expansion are outside this batch. The default scientific image/cache, deployment
bundle filenames and manifest schema remain unchanged.

Usage and artifact semantics are maintained in the
[source guide](../../../runtimes/data/source-plugins.md) and
[installed guide](../../../deploy/docker/README.md#installed-free-source-tools).

## Compatibility and measured cost

Old installed updaters enforce an exact deployment archive member set before
executing the new manager. Preserve that set and schema 2. A separate operator
image remains an alternative if measured application growth is disproportionate;
it requires complete five-image publication and manifest-aware historical asset
classification, not an unconditional new required archive.

The current debug importer is hundreds of MB; this is not a release-size estimate.
The [Container action](../../../.github/actions/container/action.yml) records actual
stripped release sizes, same-source application-base/candidate image and compressed
archive sizes, normal candidate build time, operator-stage time and sampled disk
usage through [operator_cost.py](../../../deploy/docker/operator_cost.py).
Candidate-first and then warm baseline timing is explicitly not a cold comparison.
Measurements, archive limits and installed smoke must pass and receive independent
review before merge. Local disk is insufficient for broad native builds; no
unexecuted image measurement is reported as successful.

## Verification and remaining evidence

- Python registry tests cover supported native BAR preparation, exact metadata
  bytes, source input changes, partial output and installed executable binding
- Manager tests cover pinned images, inventory-derived network policy, ownership,
  literal paths, protected mount roots/state, overlap and legacy archive members
- Existing installed smoke uses a three-candle synthetic acquisition with real
  packaged conversion/preparation, checks host-visible metadata/handoff bytes,
  retains `FIXTURE`/`UNVERIFIED`, and runs under host build-denial wrappers on the
  normal and no-checkout cold-install paths
- Appropriate existing release/package/installer and native data tests remain
  required. Exact-Head hosted Container acceptance/costs are separate from local
  deterministic checks

The preserved public Polymarket snapshot remains at fixed upstream revision
`97d05eac7324a410db4dabb4f06b25cc21aae23f`, with original snapshot SHA-256
`27ebd2f39f5a58ab8442c435d9e5f5b2b71229b5db265283141cb3a281aa1278`.
Eight original files total 30,900,196 bytes. The bounded recorded selection has
1,946 trades, 483 one-second BARs, two definitions and two payouts. Original
bytes and clocks stay outside Git and untouched; no redownload is needed.
Reverification and an existing-native-binary conversion can establish local source
integrity/conversion, but are not installed-image acceptance. Real preparation
still requires an explicit truthful declaration/selection, including the intended
universe/calendar scope; none was present with the preserved archive. Do not
fabricate those inputs or claim historical qualification to complete a smoke.

Before completion, the coordinator must record the exact staged/committed source,
executed local checks, hosted image/installed results and actual cost report. A
fixture or this task description does not stand in for those outcomes.

## Local implementation evidence

On the combined implementation tree, Python source discovery passed 90 tests,
deployment discovery passed 132 executed tests in each of the main/dev release
modes, package tests passed 10 checks and installer checks passed 12. The existing
`codex_test.CodexTests.test_legacy_installation_keeps_identity_and_native_home`
cannot create its temporary AF_UNIX socket in this executor; it failed with
`Operation not permitted` and was explicitly excluded from the final local
deployment reruns. It remains mandatory in hosted CI. YAML/Python syntax and
`git diff --check` passed. No native build was run.

The new three-candle smoke inputs also completed conversion/preparation using an
already-existing debug native executable, preserving `FIXTURE`/`UNVERIFIED` and
the original metadata bytes/digest. A bounded reconversion of the original
Polymarket snapshot retained all reported counts and produced byte-identical
detached source evidence, SHA-256
`ffa07eb4f16a3ffb3633eef469186e161bdf4fe78a54f165cb1415e90d6eede6`.
Neither check used a packaged Docker image. Docker is absent locally; installed
container/cold-install acceptance, stripped release sizes, image/archive costs
and build durations have not yet been measured. The preserved real archive lacks
an explicit declaration/native selection for real catalog preparation; that
remaining evidence was not invented.

Independent review requested two operational corrections. Archive measurement now
uses the actual release tag and gzip representation, preserving prior local tag
assignments; a regression runs the real packager orchestration against the same
fixture stream. Source containers now carry generic invocation names and
installation/owner labels. The smoke records identity before launch, reconciles
actual owned containers after local failure/timeout, and preserves diagnostics and
partial output outside enclosing temporary cleanup when completion is uncertain.
Local Docker orchestration tests verify those branches; real hosted acceptance
and the final independent-review outcome remain pending.

The installed output-reuse negative proof is retained: a bounded process loads
the actual installed manager, blocks every process-launch boundary, and requires
the existing-output rejection before Docker could run. Original publication bytes
are checked afterward. Container and no-checkout cold-install CI retain only
bounded invocation ID/state/exit/cleanup JSON facts as failure artifacts; raw
stdout/stderr, source payloads, declarations and credentials are excluded.
Successful installed calls check the actual owned-container set before cleanup.
The smoke does not deliberately interrupt a real Docker invocation; real
timeout/cancellation behavior remains unrun and is reported separately from those
successful terminal observations. Local mocked recovery tests are not evidence
of a real Docker fault-path run.
