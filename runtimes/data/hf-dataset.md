# On-demand Hugging Face dataset source

The `hf-dataset` plugin uses the existing [source runner](source_plugins.py) and
[snapshot transport](snapshot.py). Installed releases containing this module
expose it through the same operator source command. It downloads requested
source-file partitions into one reusable cache. It does not automatically
register a catalog, trigger a research Worker, or decode arbitrary Parquet as
native Nautilus data. Existing `hf-snapshot` and archive adapters retain their
original interfaces and manifests. The explicit `convert hf-dataset` bridge
uses the native archive adapter's separate `--selection` input; it never
manufactures the older `snapshot.json` or its checksums. Native conversion and
catalog preparation occur before registration/freezing, and remain offline.

## Select files before downloading

```sh
python3 -B runtimes/data/source_plugins.py plan hf-dataset \
  --dataset OWNER/DATASET --revision main \
  --manifest partitions.json --market MARKET_ID \
  --start-date 2026-09-01 --end-date 2026-09-03 \
  --max-bytes 2147483648

python3 -B runtimes/data/source_plugins.py download hf-dataset \
  --dataset OWNER/DATASET --revision main \
  --manifest partitions.json --market MARKET_ID \
  --start-date 2026-09-01 --end-date 2026-09-03 \
  --max-bytes 2147483648 --cache-dir /data/hf-cache \
  --output /data/research-request-01

python3 -B runtimes/data/source_plugins.py verify hf-dataset \
  --selection /data/research-request-01/selection.json
```

`OWNER/DATASET` and `MARKET_ID` are placeholders, not an assumed user account or
uploaded repository. Planning reads repository metadata and the explicitly
named small partition index, but does not fetch source data files. Branches,
tags and ordinary refs are resolved once; all selected files use that returned
revision. The requested ref is also retained. For reproducible retries after a
branch moves, supply the returned revision through the same `--revision` option.
No file checksums are calculated on this path.

Without a partition index, use explicit file globs:

```sh
python3 -B runtimes/data/source_plugins.py download hf-dataset \
  --dataset OWNER/DATASET --include 'data/2026-09-01.parquet' \
  --max-bytes 2147483648 --cache-dir /data/hf-cache \
  --output /data/explicit-request-01
```

The plugin never infers markets or dates from filenames. Market/date selectors
require a real index. A request must contain a market, a date range, or explicit
files; supplying only the repository/index does not fetch everything. Includes
may further narrow a partition selection. Unmatched includes, unknown markets,
and selections with no overlapping indexed files return an explicit error.

## Actual partition index

Upload an index describing only files that actually exist at the requested
repository revision. The index is bounded to 8 MiB and uses this JSON shape:

```json
{
  "schema": "qz.hf_partitions/1",
  "files": [
    {
      "path": "data/market-a/2026-09-01.parquet",
      "markets": ["market-a"],
      "start_date": "2026-09-01",
      "end_date": "2026-09-02",
      "format": "parquet"
    }
  ]
}
```

Dates and request bounds are half-open: `[start_date,end_date)`. Selection uses
actual partition overlap and exact market identifiers. If a requested market
shares a file with other markets, or the file spans a wider date range, the
whole necessary file is downloaded and its actual partition bounds/markets
remain in the result. There is no claim of row-level remote filtering.
The index and selection do not establish uninterrupted date coverage, all-market
coverage, or point-in-time availability; `coverage` is `NOT_ASSERTED`.

When source-backed date bounds are known but market membership is not, retain
that distinction explicitly. The existing index schema also accepts:

```json
{
  "path": "data/2026-09-01.parquet",
  "markets": [],
  "market_mapping": "UNKNOWN",
  "start_date": "2026-09-01",
  "end_date": "2026-09-02",
  "format": "parquet"
}
```

Such a partition can be selected by date or explicit file include. A market
filter fails if any candidate partition has unknown membership after applying
the date and include bounds; unknown partitions are never silently discarded
from a market-filtered request. The marker survives planning, cache handoff,
verification and native selection. An unmarked empty list, a null mapping,
or `UNKNOWN` paired with a nonempty list is invalid. `ALL` and `*` are not
all-market aliases; market identifiers continue to match exactly. Existing
nonempty market lists without the marker keep their original behavior.

Use a matching consumer release for this additive index capability. A
provenance inventory is not automatically this selection index, and a date
label alone is not evidence for actual file contents or market membership.

`--max-bytes` still bounds the total sizes of selected **complete files**,
including cached files. A narrow date range does not reduce a monthly object's
budget. The limit is not a cumulative network-transfer or cache-disk quota;
metadata, repeated transfers, retained rejected partials and other revisions
are separate costs. It does not enable remote row-group filtering.

Supported declared formats are `parquet`, `json`, `jsonl`, `csv`, `zip`, `gzip`
and `opaque`. Explicit includes infer only the corresponding common extension;
unknown extensions receive byte-size checking only.

## Shared cache and recovery

Each requested file has one cache location per repository and resolved revision.
Overlapping requests reuse completed files, without copying source bytes into
each request directory. The result's `files[].local_path` gives the actual
cached source path for a supported downstream adapter. The request directory
holds `request.json` and, only after all files complete, `selection.json`.

Interrupted transfers retain partial bytes and file metadata. Retrying the same
request resumes with HTTP Range. An ignored Range response restarts the partial
instead of appending a complete body. Unexpected status, size or Content-Range
fails without publishing a source file. An OS-released per-file lock prevents
concurrent overlapping requests from downloading the same file twice. Complete
data is atomically published without replacing an existing cached file.
Source bytes rejected by a format check are retained as a rejected partial; a
later retry may fetch fresh bytes. Conflicting existing cache/request files and
symlinks are rejected without overwriting the original data.

The source-checkout API can resume the same request directory. The installed
manager deliberately requires a new output directory for every invocation.
Keep the stable cache inside its owner-managed `--output-parent` writable mount,
and use a new request handoff even when resuming earlier partial cache bytes:

```sh
python3 /path/to/installed-bundle/manage.py source \
  --directory /path/to/quazonai-installation --output-parent /data/hf \
  -- download hf-dataset --dataset OWNER/DATASET \
  --include 'data/2026-09-01.parquet' --max-bytes 2147483648 \
  --cache-dir /data/hf/cache --output /data/hf/request-02

python3 /path/to/installed-bundle/manage.py source \
  --directory /path/to/quazonai-installation --read-only /data/hf \
  -- verify hf-dataset --selection /data/hf/request-02/selection.json
```

The installed bundle paths are placeholders. `/data/hf` must already be a
separate, writable, owner-managed directory. Do not also mount that writable
parent or its children as read-only inputs. Verification requires the request
and referenced cache paths to be available together through read-only mounts.
The manager obtains network policy from the actual plugin inventory: planning
and downloading are networked; verification remains offline.

Checks are limited and reported: byte size; Parquet header/footer envelope;
archive signature; complete JSON up to 32 MiB (larger JSON has prefix checking
only); or the first JSONL/CSV row. These are ordinary transfer/format checks,
not full Parquet decoding, source authenticity, scientific qualification or
checksum attestation. Native adapters own record semantics and original clocks.

## Access and source rights

This first slice uses the existing public, unauthenticated Hub transport. It
does not read tokens, create accounts, expose private-repository authentication,
accept new terms, or bypass private/gated repository controls. Dataset-card
license information is recorded when supplied; missing information is reported
and is not converted into an automatic download gate or a rights grant.
Uploading and using data still requires the actual applicable source rights.
No local source data is deleted by this plugin.

## Explicit offline native preparation

For a release containing both this plugin and its matching native executable:

```sh
python3 -B runtimes/data/source_plugins.py convert hf-dataset \
  --selection /data/hf/request-02/selection.json \
  --format moose-fills --instruments /data/native-inputs/instruments.json \
  --start-seconds 1788220800 --end-seconds 1788307200 --bar-seconds 60 \
  --output /data/native-output/converted-01

python3 -B runtimes/data/source_plugins.py prepare hf-dataset \
  --native-output /data/native-output/converted-01 \
  --declaration /data/native-inputs/catalog-declaration.json \
  --selection /data/native-inputs/native-dataset-selection.json \
  --output /data/prepared-output/catalog-01
```

The second `--selection` is the original native dataset-selection contract, not
the HF acquisition request. Input definitions, declaration and native selection
are actual operator-provided originals; the plugin does not generate them or
infer historical fees, ticks, precision, instruments or point-in-time status.

For installed execution, use separate owned output parents and literal inputs:

```sh
python3 /path/to/installed-bundle/manage.py source \
  --directory /path/to/quazonai-installation \
  --read-only /data/hf --read-only /data/native-inputs \
  --output-parent /data/native-output \
  -- convert hf-dataset --selection /data/hf/request-02/selection.json \
  --format moose-fills --instruments /data/native-inputs/instruments.json \
  --start-seconds 1788220800 --end-seconds 1788307200 --bar-seconds 60 \
  --output /data/native-output/converted-01

python3 /path/to/installed-bundle/manage.py source \
  --directory /path/to/quazonai-installation \
  --read-only /data/native-output --read-only /data/native-inputs \
  --output-parent /data/prepared-output \
  -- prepare hf-dataset --native-output /data/native-output/converted-01 \
  --declaration /data/native-inputs/catalog-declaration.json \
  --selection /data/native-inputs/native-dataset-selection.json \
  --output /data/prepared-output/catalog-01
```

All named parents must already be owner-managed directories. Conversion reads
only completed selected-cache Parquet files and original definitions. It
accepts the explicit `moose-fills`, `time-seventeen-v2`, `sii-order-filled` or `joseph-books`
schema and reuses the native row decoder. A format label or Parquet envelope is
not enough: the native decoder must successfully read the actual required
columns and produce nonempty observations. The new selection branch can use
arbitrary safe partition paths and does not infer a schema from a filename.
The old `--snapshot` input retains its original interface and layout semantics.

UTC conversion windows may narrow a requested half-open date range. Index
markets select file partitions; original instrument token IDs select decoded
native observations. No market-to-token mapping is guessed when the actual
source/definitions do not supply it. The original request and actual native
window remain in detached source evidence.

`selection.json`'s `retrieved_at` is the request's assembly/cache-reading time,
not historical availability or the first HTTP download of reused bytes.
Native source metadata states
`REQUEST_SELECTION_AT_NOT_HISTORICAL_AVAILABILITY`. An explicitly supplied
original v2 chain observation may advance the native observation to the later
of the two recorded clocks; it does not establish historical PIT. Ordinary
size, format and stat observations detect changes during conversion; no new
source-file or instruments checksums are calculated on this branch.

Preparation consumes the actually published native BAR catalog and original
declaration/native selection. It can proceed without re-downloading or keeping
the original raw cache mounted. It re-reads the native final catalog metadata
and returns `catalog_registration.root` and `catalog_registration.metadata_file`
for the existing operator registration flow. `joseph-books` is books-only and
has no BAR preparation support. Preparation returns explicit unperformed
steps; it does not register the Runtime/catalog, create source grants/datasets,
freeze inputs, run `DATA_VALIDATE`, or start research. Those existing steps
must complete before a research cycle. Scientific jobs remain offline.

Official Hub behavior and file/revision interfaces:
[download guide](https://huggingface.co/docs/huggingface_hub/guides/download),
[Hub API](https://huggingface.co/docs/hub/api).

## Verification

```sh
python3 -B -m unittest discover -s runtimes/data -p 'test_*.py'
```

The new tests cover actual indexed selection, date boundaries, unknown markets,
byte budgets, overlapping and concurrent cache reuse, interruptions, exact
Range validation, full-size unpublished partial recovery, ignored ranges,
truncation, over-sized responses, format rejection, conflicts and symlinks.
A disposable loopback HTTP server exercises real urllib redirects, interrupted
source bytes, Range resumption and repeated cache reads. Its small source bytes
are explicitly fixtures. It is not a live Hub download or native research run.

`test_hf_bridge.py` checks offline Python orchestration and handoffs using
explicit fixture native output. It does not prove actual native conversion.
The Rust archive tests additionally contain real small Parquet writer/decoder
and native catalog readback cases, including the older snapshot path. Run the
single library target when the pinned compiler/dependencies are already cached:

```sh
cargo +1.98.1 test --offline --locked -p job --features polymarket-history \
  --lib operator::polymarket_history::archive::tests -- --nocapture
```

This target must actually pass before claiming native bridge acceptance. Missing
compiler/dependencies or insufficient disk is a reported blocker, not a pass;
do not replace it with fake converters or a broad dependency rebuild.

## SII raw OrderFilled schema

`--format sii-order-filled` decodes the published raw schema of
[SII-WANGZJ/Polymarket_data](https://huggingface.co/datasets/SII-WANGZJ/Polymarket_data).
It requires snake_case asset fields, 32-byte little-endian uint256 amounts/fees,
canonical transaction/order hashes, unsigned block/log identifiers and explicit
V1/V2 contract labels. Derived `trades.parquet`, `quant.parquet` and `users.parquet`
are not interchangeable raw inputs. Exact integers outside the pinned native
number range fail; neither float recovery nor silent integer truncation is used.

V1 contracts require original `USDC.e` instrument collateral; V2 contracts require
`pUSD`. Exchange-counterparty taker summaries are excluded, self trades retained,
and aggressor side left unknown. Raw per-fill fees are not a historical fee
schedule and do not supply the missing original instrument rules. No prices are
normalized to the YES outcome.

The native ID remains `137_block_log`, within Nautilus's 36-character limit.
The [source collector](https://github.com/SII-WANGZJ/Polymarket_data/blob/188eee28f09ba83d79c125bfb367f72ac93962c4/polymarket/fetchers/rpc.py)
preserves `eth_getLogs.logIndex`, a block-global index. The adapter independently
checks original `(transaction_hash, log_index)` identity and rejects conflicting
block mappings or records. `source_metadata.sii_event_identities` retains each
selected native ID's transaction hash, block, log index and contract. An order
hash is not a unique fill key. Original file evidence remains detached.

Acquisition is still whole-file. This adapter does not add remote row-group
filtering or make the 127.2 GB monolithic raw file a small download. Prefer a
licensed, fixed, source-preserving small raw partition when available. A
re-encoded row-group extract must retain its actual derived provenance and must
not be represented as a byte-identical source file or forged HF selection.
The acquisition byte budget remains mandatory; do not raise it simply to test
compatibility. The `sii-order-filled` decoder also supports the existing explicit
legacy snapshot path when an original source snapshot already exists.

Vendor event timestamps remain UNVERIFIED event-time proxies. The public
collector revision includes a fallback timestamp-estimation path; its existence
does not prove which values in a later export used that path. Latest Gamma
market state, export time, on-chain fields and successful decoding do not prove
historical membership, definition availability, fee schedules, settlement or PIT.
Current native tests use clearly synthetic records encoded as genuine Parquet;
real SII row-group decoding and catalog/Runtime registration still need separate
source evidence and acceptance.
