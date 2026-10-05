# Operator source plugins

`source_plugins.py` is one capability-oriented entrypoint for bounded public-source
acquisition and existing native preparation. It does not call a QuaZonai service,
create grants, register catalogs, place orders, or qualify research data. No
account, secret, authenticated request or paid fallback is introduced.

## Source rights and acceptance

Public access, a free HTTP response, or an SDK license does not establish data-use
rights. Checked on 2026-10-01: the official [Exchange introduction](https://docs.cdp.coinbase.com/exchange/introduction/welcome)
links public market-data APIs to [Coinbase Market Data Terms](https://www.coinbase.com/legal/market_data)
(updated 2026-08-07); the [candles endpoint](https://docs.cdp.coinbase.com/api-reference/exchange-api/rest-api/products/get-product-candles)
belongs to that Exchange API. Without prior express written consent, section 3(5)
restricts using market data to develop, validate, benchmark or improve AI/ML
models, algorithms, agents or other automated systems; section 3(2) restricts
external redistribution of the data and derived works.

New Coinbase acquisition and acceptance for QuaZonai's intended live research and
automated-system validation remain blocked pending applicable independently
verified permission. Calling the same validation an availability observation does
not change its purpose. Offline help/plans and the capabilities below describe
technical interfaces, not permission. A supplied terms file or QuaZonai grant
cannot establish upstream rights by itself.

Dated source observations remain historical evidence. Synthetic `FIXTURE` /
`UNVERIFIED` runs and native installation checks demonstrate only their recorded
technical paths; they do not establish live-source acceptance, research eligibility
or a completed release installed for a user. Original definitions, historical
fees, PIT/availability evidence, grants and fresh validation remain separate
requirements.

## Capabilities and compatibility

```sh
python3 -B runtimes/data/source_plugins.py plugins
python3 -B runtimes/data/source_plugins.py convert --help
python3 -B runtimes/data/source_plugins.py prepare --help
```

- `polymarket-prices`: plan, download, verify. Original sampled price marks have
  no native BAR/trade conversion
- `coinbase-candles`: plan, download, verify, convert, prepare. Original OHLCV observations
  are converted by `catalog-prepare ingest-candles` using original native
  instrument definitions
- `hf-snapshot`: plan, download, verify. Generic immutable public files only;
  arbitrary Parquet is not promised a native converter
- `hf-dataset`: plan, download, verify, convert, prepare. [On-demand source-file partitions](hf-dataset.md)
  selected by explicit files or an actual market/date index; overlapping requests
  share a resumable cache. Explicit offline conversion uses the existing native
  archive schemas and original definitions; preparation requires native BAR
  output. This operator handoff does not register or trigger research
- `polymarket-capture`: the snapshot lifecycle plus conversion and BAR preparation through existing
  `polymarket-history capture`, specifically the supported lokima format with
  original Gamma responses and both recorded CLOB feeds
- `polymarket-archive`: the snapshot lifecycle plus existing `archive` conversion
  for `moose-fills`, `time-seventeen-v2`, and `joseph-books`. Preparation requires
  nonempty native BAR output from `moose-fills` or `time-seventeen-v2`;
  `joseph-books` is not supported for preparation
- `binance-vision-spot-klines`: offline plan, inspect, freeze, verify, convert and
  prepare for an already supplied single-symbol/day spot archive. No download or
  terms-acceptance capability; native conversion uses the restricted ordinary ZIP
  profile and original definitions described in the [archive guide](binance-vision.md)

CLI commands and source-specific options come from the registry's capabilities.
An unsupported operation is absent from the CLI, rather than a false ready state.
To extend it, register a `SourcePlugin` containing each supported `Capability`'s
argument configurator and handler. A preparation-capable plugin also registers
its native-publication validator, which receives the original declaration during
preparation; the common handoff does not infer a provider
from a path or maintain a separate source dispatch list. Native adapters remain responsible for source
interpretation, exact quantities, original clocks, and native serialization.
Each capability also declares whether it needs public network access. Inventory
returns `public_network_operations`: only HTTP downloads and snapshot
planning/downloads need a network. Archive inspection/freezing, verification,
conversion and preparation are offline operations.

The runner calls the original [HTTP acquisition](acquire.py) and
[snapshot helper](snapshot.py), not a second transport or Parquet decoder.
Their original manifests, schemas, and provider descriptors remain unchanged,
including the acquisition descriptor's original `native_conversion` value.
The new registry advertises the separately available native handoffs without
invalidating previously frozen acquisition manifests. No claim of native support
should be inferred from an HTTP provider descriptor alone.

HTTP planning is offline; Hugging Face planning reads public repository metadata
and resolves a fixed commit. HTTP downloads retain bounded selections, original
terms, path/size records and publication behavior; verification reconstructs their
source records and clocks. Snapshot and archive helpers retain their separate
existing verification behavior. Snapshot downloads can resume the same verified
selection; mutable HTTP acquisitions require a new directory. Snapshot verification
is offline file-integrity checking, not source authenticity, completeness,
permission, or point-in-time attestation.

## Native executables

For an installation whose completed release includes the source tools, the
installed source command uses the application image's matching operator payload;
consult the [installed guide](../../deploy/docker/README.md#installed-free-source-tools)
and check that release's command help and plugin inventory. No source checkout or
host build tools are required for installed execution. With `--native-bin` omitted, this module
selects its adjacent `bin/catalog-prepare` for candle conversion and all catalog
preparation, or `bin/polymarket-history` for history conversion. It never searches
`PATH` or falls back to a debug build. Installed mode rejects executable overrides.

For source-checkout development, build the repository's pinned official native dependencies as described in the
[project guide](../../.opensdlc/project.md#commands):

```sh
cargo build --locked -p job --features polymarket-history,catalog-prepare \
  --bin polymarket-history --bin catalog-prepare
```

Supply that actual executable file through `--native-bin`. The runner never
interpolates a shell command, installs dependencies, or invokes a development
agent. The existing root `catalog-prepare --catalog ... --declaration ...
--selection ... --output ...` interface remains unchanged and is used by the
prepare capability below. Option names must be spelled out; abbreviations are
not accepted.

## Coinbase OHLCV

The plan below is offline. Download/conversion examples describe the interface
for independently permitted inputs; the [source-use block](#source-rights-and-acceptance)
must be resolved before intended live validation. These paths and the small epoch
window are examples. Definitions must be an original Nautilus
`InstrumentAny` JSON array with supported identity, currency, precision and real
observation clocks. The runner does not generate definitions from today's product
metadata or guessed fee/tick settings. Coinbase's [Exchange fees](https://help.coinbase.com/en/exchange/trading-and-funding/exchange-fees)
are tiered; a current schedule alone does not establish the applicable historical
maker/taker rate.

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

Conversion reconstructs the original source records before calling the native
converter and rechecks them afterward. Original acquisition and instrument-definition
bytes must remain unchanged, and detached evidence retains their parsed contents.
The final report records the exact selected BAR count, conservative admission
flags and native readback. Older acquisition checksum fields remain readable but
are not verified; `verify` reports `SOURCE_RECORDS_VALIDATED`, not checksum or
source-authenticity certification. Actual catalog Parquet and detached source
evidence must exist. The native report and its limitations are returned unchanged
inside the conversion result. The importer handles one explicitly selected product
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

## Prepare an existing native BAR catalog

Preparation takes an existing published native output, an explicit original
declaration, an explicit `NativeDatasetSelectionV1`, and a new output directory.
It neither reacquires the source nor generates declarations. The declaration must
retain the intended partition, identities, origin, PIT status, revision policy,
availability explanation, universe, calendar and membership evidence. Selection
must use the actual native BAR event-label bounds and an explicit decision cutoff.
See the [history guide](README.md) for the native preparation contract.

```sh
python3 -B runtimes/data/source_plugins.py prepare coinbase-candles \
  --native-bin target/debug/catalog-prepare \
  --native-output /absolute/native/coinbase-selection \
  --declaration /absolute/original-declaration.json \
  --selection /absolute/original-native-selection.json \
  --output /absolute/prepared/coinbase-discovery
```

Use `polymarket-capture`, `polymarket-archive` or `binance-vision-spot-klines` with their matching native output
for supported BAR preparation. Sampled PRICE_MARK, arbitrary Hugging Face files,
empty BAR output and `joseph-books` output cannot use this path. An existing import
report is a publication record, not an independent attestation of historical
authenticity.

The wrapper checks source publication, artifact presence and declaration
consistency through the registered plugin before invoking the
existing root `catalog-prepare` command with those three original paths and the
new output. Native code owns declaration validation, selection, original
definitions and fees, settlements, measured row count and quality, isolated
copying, and native readback. Do not supply derived `row_count` or `quality` in the
declaration. Missing original fees, definitions from the future, or insufficient
evidence remain native failures. Original event/receipt clocks and numerical
values are retained, and Sealed metadata follows native redaction.

A preserved archive with `SYNTHETIC` provenance cannot use an `origin=REAL`
declaration: preparation rejects that contradiction before native execution or
output creation. Other origin values and PIT declarations remain subject to the
native metadata contract, including `VERIFIED` requiring `AS_KNOWN_THEN`; synthetic
origin alone does not define a separate PIT policy. Operator-declared provenance
retains the existing trusted-operator boundary. This consistency check neither
authenticates a source nor establishes historical availability. Direct root
`catalog-prepare` still accepts explicit operator declarations without detached
source evidence; later `DATA_VALIDATE` does not reopen that archive evidence.

Successful stdout contains one `CATALOG_PREPARED` handoff with:

- Canonical output, catalog root and original `catalog-metadata.json` paths
- Byte length and SHA-256 of the exact final metadata file, including its original
  formatting; native stdout is not the metadata byte source
- `catalog_registration` containing exactly `root` and `metadata_file`, for the
  Runtime's existing `CatalogRegistration` entry
- `identity_hints` containing only `native_catalog_ref` from `registered_ref` and
  `native_storage_version` from `storage_version`; both must match the explicit
  declaration
- Local source report/evidence paths with byte lengths and digests, plus remaining
  registration and validation steps
- Optional `producer` version, revision and immutable image identity, supplied by
  the installed manager only when all `QZ_OPERATOR_VERSION`,
  `QZ_OPERATOR_REVISION` and `QZ_OPERATOR_IMAGE` values are present. These values
  identify the operator image and do not certify source quality or provenance

Keep the handoff and detached source artifacts local. They can include private
filesystem paths. SHA-256 binds file bytes; it does not authenticate their upstream
source. The handoff does not embed or replace the authoritative metadata document.
Runtime still reads and validates the original metadata bytes from `metadata_file`,
and service registration still fetches them from the selected Runtime. Hints cannot
supply quality, origin or PIT status. Keep source evidence outside the research
catalog mount, and use paths that exist verbatim on that Runtime's host.

## Failure and evidence boundaries

Native conversion and preparation require a new output directory. They use a fixed argv list,
closed stdin, no shell, and a one-hour process bound. Missing/invalid publication,
changed inputs, unexpected admission flags, incomplete artifacts or nonzero exit
are failures. Report, declaration, selection and final metadata reads are bounded
to 1 MiB; detached evidence reads are bounded to 128 MiB. Preparation rechecks the
exact declaration, selection and source publication bytes around native execution.
Exit zero alone is insufficient: the final `catalog-metadata.json` and native
catalog must exist, and a partial metadata file does not count. Failed outputs and raw evidence
are never deleted or reused; inspect the original native diagnostics and retry
only into a new output directory.

Preparation also rejects source/output overlap, symlink paths and mismatched
published identities. Nonzero exit, timeout or interruption never emits a success
handoff. A wrapper check can fail after native metadata has been published; it
retains that output for inspection and does not delete a valid native artifact or
the original acquisition. Native failures keep the existing
`QZ_CATALOG_PREPARATION_FAILED` diagnostic; the wrapper does not invent a more
specific scientific reason or expose exception chains.

`NATIVE_ARTIFACTS_VALIDATED` means that the wrapper checked the native publication
report, provenance links and artifact presence/framing. It is not an independent
Python decode of native Parquet. The Coinbase native converter supplies the
explicit readback result, as does the archive candle converter; existing Polymarket native behavior and its Rust
roundtrip tests remain authoritative. Stub subprocess tests exercise orchestration
failures only and are not a live-source or scientific validation claim.

```sh
python3 -B -m unittest discover -s runtimes/data -v
```

`CATALOG_PREPARED` means native publication and the wrapper's handoff checks
succeeded, not that a catalog is registered or research is admitted. Preparing
each intended Discovery/Validation/Sealed partition requires its own explicit
declaration, selection and new output. Source/grant registration, dataset
registration, Runtime configuration, frozen InputSets and fresh `DATA_VALIDATE`
remain separate steps. Consult the [history guide](README.md) and
[acquisition provider guide](providers.md) for original evidence requirements and
unsupported source coverage.
