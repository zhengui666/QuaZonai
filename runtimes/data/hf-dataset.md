# On-demand Hugging Face dataset source

The `hf-dataset` plugin uses the existing [source runner](source_plugins.py) and
[snapshot transport](snapshot.py). Installed releases containing this module
expose it through the same operator source command. It downloads requested
source-file partitions into one reusable cache. It does not automatically
register a catalog, trigger a research Worker, or decode arbitrary Parquet as
native Nautilus data. Existing `hf-snapshot` and archive adapters retain their
original interfaces and manifests. In particular, the existing archive converter
requires its original `snapshot.json`; the new `selection.json` is not that input.
An explicit pre-research native-adapter bridge is a separate acceptance step.

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
