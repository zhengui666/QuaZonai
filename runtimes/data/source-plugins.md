# Operator source plugins

`source_plugins.py` is one capability-oriented entrypoint for bounded free public
acquisition and existing native preparation. It does not call a QuaZonai service,
create grants, register catalogs, place orders, or qualify research data. No
account, secret, authenticated request or paid fallback is introduced.

## Capabilities and compatibility

```sh
python3 -B runtimes/data/source_plugins.py plugins
python3 -B runtimes/data/source_plugins.py convert --help
```

- `polymarket-prices`: plan, download, verify. Original sampled price marks have
  no native BAR/trade conversion
- `coinbase-candles`: plan, download, verify, convert. Original OHLCV observations
  are converted by `catalog-prepare ingest-candles` using original native
  instrument definitions
- `hf-snapshot`: plan, download, verify. Generic immutable public files only;
  arbitrary Parquet is not promised a native converter
- `polymarket-capture`: the snapshot lifecycle plus conversion through existing
  `polymarket-history capture`, specifically the supported lokima format with
  original Gamma responses and both recorded CLOB feeds
- `polymarket-archive`: the snapshot lifecycle plus existing `archive` conversion
  for `moose-fills`, `time-seventeen-v2`, and `joseph-books`

CLI commands and source-specific options come from the registry's capabilities.
An unsupported operation is absent from the CLI, rather than a false ready state.
To extend it, register a `SourcePlugin` containing each supported `Capability`'s
argument configurator and handler. Native adapters remain responsible for source
interpretation, exact quantities, original clocks, and native serialization.

The runner calls the original [HTTP acquisition](acquire.py) and
[snapshot helper](snapshot.py), not a second transport or Parquet decoder.
Their original manifests, schemas, and provider descriptors remain unchanged,
including the acquisition descriptor's original `native_conversion` value.
The new registry advertises the separately available native handoffs without
invalidating previously frozen acquisition manifests. No claim of native support
should be inferred from an HTTP provider descriptor alone.

HTTP planning is offline; Hugging Face planning reads public repository metadata
and resolves a fixed commit. Downloads retain each helper's existing bounded
selection, original terms, checksum, and publication behavior. Snapshot downloads
can resume the same verified selection; mutable HTTP acquisitions require a new
directory. Snapshot verification is offline file-integrity checking, not source
authenticity, completeness, permission, or point-in-time attestation.

## Native prerequisites

Build the repository's pinned official native dependencies as described in the
[project guide](../../.opensdlc/project.md#commands):

```sh
cargo build --locked -p job --features polymarket-history,catalog-prepare \
  --bin polymarket-history --bin catalog-prepare
```

Supply the actual executable file through `--native-bin`. The runner never
interpolates a shell command, installs dependencies, or invokes a development
agent. The existing root `catalog-prepare --catalog ... --declaration ...
--selection ... --output ...` interface remains separate and unchanged.

## Coinbase OHLCV

These paths and the small epoch window are examples; choose the actual original
terms and requested market/window. Definitions must be an original Nautilus
`InstrumentAny` JSON array with supported identity, currency, precision and real
observation clocks. The runner does not generate definitions from today's product
metadata or guessed fee/tick settings.

```sh
python3 -B runtimes/data/source_plugins.py plan coinbase-candles \
  --instrument BTC-USD --start-seconds 1788220800 --end-seconds 1788220980 \
  --interval-seconds 60

python3 -B runtimes/data/source_plugins.py download coinbase-candles \
  --instrument BTC-USD --start-seconds 1788220800 --end-seconds 1788220980 \
  --interval-seconds 60 --max-bytes 1048576 \
  --terms-file /absolute/original-coinbase-terms.txt \
  --output /absolute/data/coinbase-selection

python3 -B runtimes/data/source_plugins.py verify coinbase-candles \
  --acquisition /absolute/data/coinbase-selection/acquisition.json

python3 -B runtimes/data/source_plugins.py convert coinbase-candles \
  --native-bin target/debug/catalog-prepare \
  --acquisition /absolute/data/coinbase-selection/acquisition.json \
  --instruments /absolute/original-coinbase-instruments.json \
  --output /absolute/native/coinbase-selection
```

Conversion verifies the original acquisition before calling the native converter
and rechecks it afterward. The final report must bind the SHA-256 of both original
input files, report the exact selected BAR count, retain conservative admission
flags, and attest native readback. Actual catalog Parquet and detached source
evidence must exist. The native report and its limitations are returned unchanged
inside a preparation result. The importer handles one explicitly selected product
per acquisition, with original native definitions and supported tick-only versions.
Its `native_selection` uses BAR event-label bounds and the actual availability
cutoff, suitable for inspection and the existing separately declared partition
preparation. Source candle bucket-start bounds must not be reused blindly as BAR
label bounds. No partition, source permission or historical availability claim is
generated from this selection.

Bucket-end event times remain distinct from actual retrieval availability. A
batch can therefore have equal initialization timestamps. Do not invent per-row
nanoseconds, backdate retrieval, fill missing buckets, or promote retrospective
observations to historical availability. Native serialization/readback does not
by itself establish scientific acceptance: historical definition/fee evidence,
permission, coverage, partition isolation and fresh validation remain separate
requirements. Native preparation and catalog readback can accept equal original
receipt times while preserving strictly increasing event times. This does not
qualify REST-backfilled history for prediction research: the forecast output
contract still requires increasing availability times and labels becoming
available later, and that contract is unchanged. No forecast/PIT eligibility is
inferred from native preparation or a successful catalog readback.

## Recorded Polymarket history

The source and pinned selection below match the existing bounded example in the
[history guide](README.md#captured-historical-availability). Acquisition uses the
same snapshot helper, including preserved source card/license evidence.

```sh
python3 -B runtimes/data/source_plugins.py download polymarket-capture \
  --dataset lokima/polymarket-btc-eth-5m-updown-8h \
  --revision 97d05eac7324a410db4dabb4f06b25cc21aae23f \
  --include 'data/clob_ws/btc/1785357600/*.parquet' \
  --include 'data/reference/discovery.parquet' \
  --include 'metadata/clock.json' --include 'PROVENANCE.md' \
  --include 'metadata/window_meta.parquet' \
  --include 'metadata/quality_report.json' \
  --max-bytes 33554432 --output /absolute/data/lokima-selection

python3 -B runtimes/data/source_plugins.py verify polymarket-capture \
  --snapshot /absolute/data/lokima-selection/snapshot.json

python3 -B runtimes/data/source_plugins.py convert polymarket-capture \
  --native-bin target/debug/polymarket-history \
  --snapshot /absolute/data/lokima-selection/snapshot.json \
  --market-slug btc-updown-5m-1785357600 \
  --start-seconds 1785357600 --end-seconds 1785357900 --bar-seconds 1 \
  --output /absolute/native/lokima-selection
```

The native adapter retains its historical Gamma, dual-feed, parameter, event
identity, original clock and resolution checks. The wrapper does not interpret
Parquet or reconstruct a replacement market. It checks the original snapshot
before/after conversion and binds final native evidence to that exact snapshot,
format, market, window and record counts. Original native diagnostics are relayed
to stderr; stdout contains one JSON result. Existing native format and research
limitations remain in force.

For another supported existing archive, replace the plugin with
`polymarket-archive` when planning/downloading, then select its actual format:

```sh
python3 -B runtimes/data/source_plugins.py convert polymarket-archive \
  --native-bin target/debug/polymarket-history \
  --snapshot /absolute/data/original-fills/snapshot.json \
  --format moose-fills --instruments /absolute/original-instruments.json \
  --start-seconds 1667260800 --end-seconds 1667261100 --bar-seconds 60 \
  --output /absolute/native/original-fills
```

`joseph-books` preserves its existing minute-end book semantics and accepts no BAR
aggregation. `time-seventeen-v2` can additionally receive an existing original
`--chain-evidence` file, verified by the native adapter. Larger explicit snapshots
need an appropriate `--max-bytes` on verification/conversion. Chain acquisition,
live capture collection, arbitrary vendor schemas, and source-specific native
extensions are not added by this wrapper.

## Failure and evidence boundaries

Native conversion requires a new output directory. It uses a fixed argv list,
closed stdin, no shell, and a one-hour process bound. Missing/invalid publication,
changed inputs, unexpected admission flags, incomplete artifacts or nonzero exit
are failures. Report reads are bounded to 1 MiB and detached evidence to 128 MiB. Exit zero alone is insufficient. Failed outputs and raw evidence
are never deleted or reused; inspect the original native diagnostics and retry
only into a new output directory.

`NATIVE_ARTIFACTS_VALIDATED` means that the wrapper checked the native publication
report, provenance links and artifact presence/framing. It is not an independent
Python decode of native Parquet. The Coinbase native converter supplies the
explicit readback result; existing Polymarket native behavior and its Rust
roundtrip tests remain authoritative. Stub subprocess tests exercise orchestration
failures only and are not a live-source or scientific validation claim.

```sh
python3 -B -m unittest discover -s runtimes/data -v
```

Source grants, service registration, Runtime mounting, frozen InputSets, separate
Discovery/Validation/Sealed preparation and fresh `DATA_VALIDATE` are not performed
by any plugin operation. Consult the [history guide](README.md) and
[acquisition provider guide](providers.md) for original evidence requirements and
unsupported source coverage.
