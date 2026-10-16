# Bounded Web CI toward five minutes

Owner request: make ordinary CI finish within five minutes without dropping real
acceptance. Implementation base: `fe2cf418cfadb52b9a5f4b6f224e877372353f47`.
Issue/PR: not published; this patch awaits independent native review and hosted
final-Head evidence under [review](../../review.md).

## Work and preserved coverage

[Web](../../../.github/workflows/web.yml) starts four independent checkouts of the
exact event Head. The existing required name, `Web / contracts, browser and real
Rust API`, is an always-run aggregate: every one of the four jobs must explicitly
report success. Missing, failed, cancelled and skipped jobs fail the aggregate;
branch protection does not change.

- Source keeps the official Skills CLI's local/remote installation and byte
  comparisons, installation-shell tests, native script syntax/service tests,
  all generated-client/validator adapter checks, generation/drift checks,
  typecheck, all unit tests, static build, modular asset/precache check and the
  independent final generated check
- Synthetic A runs the complete settings file: 36 discovered instances at the
  base. Synthetic B runs the other four files: 39 instances, then the complete
  validator suite in both projects: 20 declared instances, including only its
  two pre-existing intentional dev PWA skips. B builds current production assets
- Native builds current production assets and the real Rust API/fixture pair
  with the original locked Cargo command, exports and diffs the actual OpenAPI,
  then retains PostgreSQL/PGMQ,
  pinned Caddy, systemd user services, all four cleanup fault cases and all seven
  ordered browser instances. Original auth, project/receipt/session continuity,
  PWA mutation/restoration, Worker crash/restart, API stop/start, admission and
  database cardinality/cleanup assertions remain in their original order

Each of the four executing jobs installs locked npm dependencies without lifecycle hooks. Every
browser job installs Chromium and native dependencies. Every executing checkout
must remain clean. No assertions, timeouts, worker counts or retry policies are
weakened; no other workflow is removed or made optional.

Native verifies the official `fonts-wqy-zenhei` package already installed by
the pinned Playwright Ubuntu dependency recipe. The shared check makes no
additional package or network request and fails if that prerequisite is absent.
It also verifies fontconfig selects the exact `WenQuanYi Zen Hei` sans family;
the Linux native browser asserts that actual family rendered the Chinese heading.
No font is added to frontend/PWA assets. Actual glyph rendering remains the
companion UI patch's native Chromium CDP assertion, not a CSS-only claim.

[Coverage accounting](../../../apps/web/scripts/web-ci.mjs) compares full current
Playwright discovery against both explicit synthetic file selections, rejecting
omissions, overlaps, unknown and empty groups. Each execution must produce the
same `(project, file, full title)` set exactly once with zero retries and the
expected successful outcome. Only the two named validator dev PWA skips are
allowed; each must still be present and skipped. Native performs the same
list/result reconciliation inside each original phase using its live private
fixture. Raw JSON stays in its private directory; uploaded native evidence and
synthetic job logs contain only source identities, statuses and timings.

## Native binaries always build from current source

This patch restores the original native toolchain setup, locked Cargo build and
actual schema export. It does not restore or save compiled binaries, override the
harness's binary paths, change permissions or add a cache authentication system.

The proposed cache was removed after independent review: manifest hashes and
self-reported dev workflow metadata prove consistency, not producer provenance.
[GitHub cache scope](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#restrictions-for-accessing-a-cache)
allows a PR to restore its own merge-ref entries, which can precede the trusted
base cache. The pinned [restore action interface](https://github.com/actions/cache/blob/55cc8345863c7cc4c66a329aec7e433d2d1c52a9/restore/action.yml)
returns matching keys but no actual cache ID/ref. An exact key therefore cannot
exclude an earlier forged entry in that PR scope. Restoring only on dev pushes
also does not establish the actual source ref of a default-branch fallback.

Trusted Actions-artifact reuse needs a separate design that independently binds
producer event, branch, workflow/run and actual downloaded bytes. It is outside
this patch. No cached build or old successful test replaces fresh acceptance.

## Evidence and limits

Local validation uses existing node modules and tiny test fixtures, without
Cargo compilation/linking, package installation, Docker or a browser launch.
Coverage/gate regression tests also exercise the installed Playwright JSON
reporter with pass/skip functions that use no browser fixture. Current discovery
reconciles 75 synthetic plus 20 validator instances. Native list-only discovery
finds five before restart, one after restart and one data-admission instance;
these were not executed. Local results on the frozen candidate:

- `node --test apps/web/scripts/web-ci.test.mjs apps/web/scripts/ensure-cjk-fonts.test.mjs`: 9 passed
- `node --test apps/web/scripts/native-user-services.test.mjs`: 8 passed
- `node --test deploy/install.test.mjs`: 12 passed
- Native harness syntax, all workflow shell blocks, local document links,
  original retained workflow-step equivalence and Git whitespace checks passed

The font script and tests are shared unchanged with the companion UI patch.
No native build, live server/schema export, browser acceptance, font install,
full source suite or hosted workflow was run locally.

This remains work toward five minutes, not a demonstrated speedup. The
[earlier Web run](https://github.com/zhengui666/QuaZonai/actions/runs/36890629263/job/110465045380)
spent 221 seconds in native build/schema and 154 seconds in ordered native
acceptance alone: a cold native path already exceeds 300 seconds before setup.
The more recent [PR159 Web run](https://github.com/zhengui666/QuaZonai/actions/runs/36909702124/job/110528996287)
took 970 seconds total: Chromium/system dependency installation 156, synthetic
interaction 266, validators 54, native build/schema 233 and native acceptance
151 seconds. Installation plus native acceptance alone is 307 seconds, so even
even a hypothetical no-compile native path exceeds five minutes at those costs.
The earlier 41-second browser-install observation does not justify a stable
252–282-second warm estimate. A browser cache is outside this patch, and cannot
be credited with savings until download and system-dependency costs are measured.
This patch always pays the native compilation cost. It establishes independent
coverage-preserving lanes, not a demonstrated sub-five-minute path.
Queue time, runner availability, aggregate scheduling, browser
dependency/font installation and asset builds count toward the user's elapsed
time. Settings/other partition balance also needs hosted measurement.

Before any five-minute claim or merge, independently review the final composed
Head and run its complete applicable hosted CI. Collect repeated current-source
runs, event-to-aggregate wall time including queue, per-job/per-test timings and
disk headroom. Exercise failed/cancelled/skipped shards and missing execution
instances. Local helper tests and discovery do not establish successful
native/browser execution or CI timing.
