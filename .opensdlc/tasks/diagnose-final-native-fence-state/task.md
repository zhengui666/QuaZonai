# Preserve bounded final-fence operands on failure

## Observed evidence

Baseline: maintenance head da1bca89. Run 37607974734, Store job 112748026474,
actually ran the original native Mission case. After 78.78 seconds it failed
with CLOSE_MISSION / Unavailable. Its ordinary-library diagnostics recorded
stop.not-fenced, then host.stop, then client.group. They did not identify the
stop-command success bit, final load classification, fragment presence or whether
the existing one-time cache refresh ran. No CPU-accounting failure was recorded.

Production closes by cgroup.kill before native stop, unlike the running-service
fixture. A collected transient before stop could produce a non-successful stop
and a not-found observation, skipping the old success-gated refresh. Alternatively
the second observation could remain not-found after a successful stop/refresh.
Neither operand combination is asserted to be this run's actual state.

## Narrow helper change

The private helper adds FenceLoad = Masked / Loaded / NotFound / Other and
FenceState containing that closed classification and a fragment_present boolean.
not_fenced receives the original stop_succeeded and refreshed booleans and safe
before/after snapshots. It records exactly eight approved fields:

- phase and failure_class, fixed to stop.not-fenced / Unavailable
- stop_succeeded and refreshed
- before_load and before_fragment_present
- after_load and after_fragment_present

No arbitrary native state string, fragment path, unit identity, command output,
exit integer, environment or credential enters this interface. The existing
failure helper keeps its original two-field event. Both use ordinary-library
WARN tracing in the original test process, requiring no new config or service.
The returned error remains the original Unavailable. This is failure evidence,
not a fence predicate or proof of a successful shutdown.

The service owner supplies the complementary patch: preserve the original
mask/terminal/empty-cgroup checks, refresh at most once for an already-confirmed
not-found observation, and apply the unchanged final fenced predicate. Persistent
not-found still fails; loaded with a fragment still requires a successful stop.
This helper patch alone does not modify that policy or execute a stop/reload.

## Verification boundary

The added pure tracing test enumerates every closed load value and checks the
exact eight-field whitelist, booleans and unchanged return. Existing two-field
and error-payload exclusion tests remain intact. Combine with the service patch
before the shared native build/test window; the original real-PG Mission case
must then run again. A passing logger test does not establish the failing run's
operands, resource closure, final CPU receipt or successful Mission completion.

Current status: final independent helper/service static review passed with no
outstanding P1/P2 findings. Combined native build/tests and the original real
manager/PG case remain pending; this is not a successful-close result.
