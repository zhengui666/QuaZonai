# Polymarket native Paper downstream control

Provide the existing authenticated loopback Downstream control protocol for the
bounded original-claim Polymarket Cash host. This adapter does not change
Nautilus, execution/fee models, research admission, the original frozen target,
or historical data. It creates no credential and registers no venue execution
client. Local official simulation orders remain local.

## Surface

`job polymarket-paper serve --config /approved/service.json` serves one original
V2 claimed target through `/downstream/v1/capabilities`, `/targets`, `/status`
and `/stop`. `polymarket-paper apply`, `status` and `stop` reuse the existing
literal-loopback authenticated client. Existing one-shot `run`/`source` and
Binance `paper` behavior remain separate.

The service configuration contains exactly:

- `schema_version`: 1
- `host_config`: path to the original Polymarket HostConfig JSON
- `frozen_metadata`: path to its original RuntimeCatalogMetadataV1
- `dataset_revision`: path to the exact original DatasetView
- `output_directory`: a new private directory under an approved parent
- `claim_state_directory`: an absolute canonical private directory on the stable runtime volume, reused across restarts and output-directory changes; it must not be inside `output_directory`
- `credential_file`: an existing owner-private control-service bearer file
- `bind`: a literal loopback socket address
- `max_seconds`: the bounded observation period, 1 through 300
- `proxy_env`: optional explicitly approved existing variable name, otherwise absent

The control-service bearer is not the QZ machine identity used to claim handoffs
or relay account observations. Reuse only already-authorized credentials for
their intended services. Provisioning persistent access or configuring a reverse
proxy is a separate approved operation, not this implementation.

## Lifecycle and evidence

The service retains byte-original input files and exclusively reserves its
output directory before advertising capabilities. An existing directory fails;
restart never restores a native account or repeats an admitted claim. The stable
`claim_state_directory` protects claim identity independently of that output
reservation. The original claim is durably reserved before the native owner is
queued. A completed claim replays its original versioned lifecycle status across
processes, byte-for-byte, without starting another owner. An incomplete or corrupt
record requires recovery and is never silently retried. A different claim conflicts
within the same session; a different legitimate claim has a separate durable key.
Choosing a new evidence directory does not bypass protection in the same runtime
state root. Preserve that root; do not create a new state root to retry old work.
See [durability and recovery boundaries](../paper-claim-journal/task.md).

The Polymarket profile advertises PAPER and package version 2 only. It rejects
V1, another venue/account kind and leverage before reservation. Complete original
config/claim/metadata/fee/provenance validation still belongs to the unchanged
native host. Starting means an admitted owner is preparing; Running is emitted
only after its actual original quote-driven engine has started. This is an
event-time native Paper simulation, not a venue account connection.

Stop records intent and interrupts both source-record waiting and post-EOF
source-process waiting. The existing child cleanup terminates and reaps the
actual source child; the owner disposes the native engine. Terminal state is
published only after the owner thread joins. Cancelled/incomplete sessions keep
diagnostics and never receive a relay-eligible binding. Failure to establish
cleanup remains Failed, not an asserted clean stop.

After native completion, a compact in-memory summary supplies terminal status;
large original report/snapshot evidence is not reread through an input-size cap.
The complete versioned `terminal-status.json` is retained in the stable claim
journal only after the owner joins, and the original output directory keeps its
terminal observation as well. A replay validates the complete schema and identity
but returns the original bytes with `X-Paper-Claim-Replayed: true`; it is not a fill
receipt or account restoration. The existing two-second final HTTP observation
drain follows. The foreground service then exits. Loss of an HTTP
connection is not proof of native termination: use the actual retained status
and process outcome. No ACK or account relay is performed automatically.

## Verification and remaining acceptance

Unit coverage includes profile capability/admission, exact claim replay and
conflict, stop intent versus native state, evidence-directory reuse refusal,
original-file non-overwrite, cancellation of silent/half-line and post-EOF
children with actual reap, and status independence from reports above 8 MiB.
These use explicitly synthetic fixtures and loopback/disposable processes only.
Existing original-quote fills, official fee, terminal valuation and converter
regressions remain applicable without replacing real business inputs.

A real deployed Downstream, its approved persistent identities, original
TARGET_WEIGHT Alpha/FORWARD inputs, valid claimed target, real current native
simulation and backend relay/readback are separate acceptance requirements.
Compilation or fixture tests do not assert those outcomes. Production deployment
continues to use an authorized official release image, never a local build
silently installed over the user's service.
