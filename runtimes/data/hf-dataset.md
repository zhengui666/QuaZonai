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
accepts the explicit existing `moose-fills`, `time-seventeen-v2` or `joseph-books`
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
