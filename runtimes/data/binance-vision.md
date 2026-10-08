# Offline Binance Vision source plugin

The source registry supports offline plan, inspect, freeze, verify, native
conversion and catalog preparation for one already supplied spot kline archive.
It has no download or terms-acceptance operation. The standalone Python library
and its plan/inspect/freeze/verify CLI remain compatible; native operations use
the existing `catalog-prepare` executable. Existing Coinbase/Polymarket frozen
descriptors and `qz.public_acquisition/1` remain unchanged. An installation must
use a completed release containing this plugin before these capabilities are
available. See the [source-plugin guide](source-plugins.md) and
[installed source command](../../deploy/docker/README.md#installed-free-source-tools).

## Scope and source identity

[binance_vision.py](binance_vision.py) supports Python 3.10+ and its standard
library. The reusable selection is one explicit symbol, base asset, quote asset,
UTC date and interval. Symbols are uppercase ASCII letters/digits, at most 32
characters; the explicitly declared distinct base and quote must concatenate to
the symbol. This is syntactic consistency, not a verified instrument definition
or proof the market/archive exists. No suffix splitting or currency guessing is
performed. Venue is always `BINANCE`, market `SPOT`, and provider identity is
`binance-vision-spot-klines` version 1. The stable provider descriptor excludes
current implementation capabilities so a future converter need not rewrite it.

Supported intervals are `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`,
`8h`, `12h` and `1d`. They divide one UTC day, with at most 1,440 buckets. One
second, week/month, multi-day and multi-symbol selections are outside this
profile. Dates must fit unsigned native nanoseconds. Planning is deterministic;
freezing requires the selected day to have completed.

The pinned [official archive README](https://github.com/binance/binance-public-data/blob/bd110bb04caad6ad964a0098809f18343b1e104b/README.md)
provides naming and the timestamp-unit transition. Source references are derived
URLs marked `DOCUMENTATION_DERIVED_NOT_REQUESTED`. A documentation Git revision
is never an archive revision. Archives have a local content-hash identity and
`upstream_revision=null`.

## Local commands and API

The following example is an offline selection, not a claim that these files
exist. Substitute your own already authorized local original files. No command
here downloads them or accepts terms.

```sh
python3 -B runtimes/data/binance_vision.py plan \
  --symbol ETHBTC --base-asset ETH --quote-asset BTC \
  --day 2024-12-31 --interval 5m

python3 -B runtimes/data/binance_vision.py inspect \
  --symbol ETHBTC --base-asset ETH --quote-asset BTC \
  --day 2024-12-31 --interval 5m \
  --archive /absolute/original/ETHBTC-5m-2024-12-31.zip \
  --checksum /absolute/original/ETHBTC-5m-2024-12-31.zip.CHECKSUM

python3 -B runtimes/data/binance_vision.py freeze \
  --symbol ETHBTC --base-asset ETH --quote-asset BTC \
  --day 2024-12-31 --interval 5m \
  --archive /absolute/original/ETHBTC-5m-2024-12-31.zip \
  --checksum /absolute/original/ETHBTC-5m-2024-12-31.zip.CHECKSUM \
  --output /absolute/private/new-archive-bundle

python3 -B runtimes/data/binance_vision.py verify \
  --output /absolute/private/new-archive-bundle
```

`inspect` returns counts and hashes, not market values. `freeze` accepts optional
`--provenance`, `--vision-terms-file`, `--incorporated-terms-file`,
`--license-file` and `--parser-spec-file`. Each evidence file retains its exact
bytes with `OPERATOR_SUPPLIED_UNVERIFIED` status; unknown evidence URL/revision
remain null. File contents and a checksum are never proof of applicable rights.

The Python API exposes `Selection`, `plan(selection)`,
`decode(archive_bytes, checksum_bytes, selection)`,
`freeze(selection, archive_path, checksum_path, output, *, provenance_path=None,
evidence_paths=None)` and `verify(output)`. Evidence paths use the fixed roles
`vision_terms`, `incorporated_terms`, `license`, `parser_spec`. `decode` returns
rows, counts and member integrity. Exceptions reject invalid input; no parser
repairs, arbitrary provider/venue/URL, caller code loading or network fallback
exists.

## Frozen envelope and clocks

`qz.public_archive_acquisition/1` is separate from existing HTTP acquisition
schemas. Its `artifact_scope=OFFLINE_LOCAL_IMPORT`, top-level `provenance_kind`
and `provenance_status` make clear that no request was made by this tool. The
bundle retains:

- `raw/archive.zip` and `raw/archive.CHECKSUM`: exact input bytes
- `provenance.json` when supplied: exact original declaration bytes
- `evidence/<role>.bin` when supplied: exact operator-supplied evidence
- `records.jsonl`: exact decimal lexemes and original archive/member/row lineage
- `archive.json`: final publication marker with the fixed plan, file sizes and
  SHA-256 identities, member identity, counts and conservative admission flags

Original inputs must be regular files within their limits. Symlink paths and
special files, including FIFOs, fail. Output must be a new directory with an
existing parent. Publication reuses the snapshot helper's atomic no-replace
writes; original and copied inputs are reread before publishing the final
manifest. Failure may leave an unpublished partial directory, which is never
resumed or silently removed. Use a new output path. Verification reparses ZIP
and CSV, regenerates all normalized rows and the full envelope, compares file
identities and rereads inputs to detect changes during the check. This is an
offline local integrity check, not protection from a privileged writer changing
files after verification; protect the frozen directory through normal ownership.

No supplied provenance means `UNKNOWN`. Explicit synthetic fixtures use:

```json
{"kind":"SYNTHETIC","retrieval":null}
```

An operator may retain a declaration of original retrieval clocks:

```json
{
  "kind": "OPERATOR_DECLARED",
  "retrieval": {
    "checksum": {"started_at":"2026-09-30T00:00:00Z","completed_at":"2026-09-30T00:00:01Z"},
    "archive": {"started_at":"2026-09-30T00:00:02Z","completed_at":"2026-09-30T00:00:03.000000009Z"}
  }
}
```

These clocks are checked for completed day, sequential checksum/archive ordering,
nonnegative duration and completion before import. Nanosecond precision is
retained without a floating-point conversion. The declaration remains
`DECLARED_UNVERIFIED`; there is no attested provenance input mode. `UNKNOWN`
requires null retrieval. `SYNTHETIC` may contain synthetic clocks but never
becomes a real capture. `OPERATOR_DECLARED` may also leave retrieval null.

Every normalized row has `observed_at=null` and `historical_available_at=null`.
Only `declared_observed_at` may preserve the supplied archive completion time.
Generated `imported_at` and `published_at` describe this local operation, never
retrieval or historical receipt. Verification rejects future publication/import
clocks and inconsistent order, but cannot independently authenticate any
self-consistent local clock. Native conversion refuses `UNKNOWN` or a missing
retrieval object, including a `SYNTHETIC` declaration without clocks. It preserves
the explicit archive completion as `ts_init` with a `receipt_basis` identifying
the original field, exact nanoseconds and `OPERATOR_DECLARED_UNVERIFIED` or
`SYNTHETIC` status. The original null observation/availability fields remain null
in preserved evidence. Local import time never becomes retrieval, and declared
initialization is not attested market receipt or historical PIT evidence.

The source-plugin `prepare` path compares the original catalog declaration with
the preserved publication. Known `SYNTHETIC` archive provenance rejects
`origin=REAL` before invoking native preparation or creating its output. Other
origin/PIT values retain the native declaration contract; this is an origin
consistency check, not verification or reassignment of operator-declared PIT
status. Receipt clocks, source evidence and metadata bytes are not rewritten.

`implementation_sha256` records the local importer file when freezing.
`implementation_status=RECORDED_LOCAL_HASH_NOT_ATTESTED` and
`implementation_revision=null` are deliberate: verification checks its format,
not a registry of trusted historical implementations. It is neither code
attestation nor the upstream data revision. Original v1 provider/parser rules
must remain reproducible when future implementations add capabilities.

## Parsing and source validation

Timestamp units follow the selected source date: milliseconds before 2025-01-01,
microseconds on/after it. Unit guessing by digit count is forbidden. Opens must
be aligned, strictly increasing and inside the selected UTC day. Inclusive
source close must equal open plus interval minus one source unit. Its original
value remains separate from the normalized exclusive `event_end_ns`. All source
timestamps, normalized nanoseconds, row indices and counts are decimal strings.

Exactly twelve headerless ASCII fields are required. LF/CRLF and an optional
final terminator are supported; quotes, multiline fields, BOM, controls, blank
records, ragged rows, duplicate/disordered buckets and out-of-day rows fail.
OHLC are positive and ordered; volumes are nonnegative; taker volumes cannot
exceed their corresponding total. Trade counts are bounded unsigned integers.
Decimal lexemes allow digits and an optional fractional part, without exponent,
sign, whitespace or redundant integer leading zeros. Decimal construction and
comparisons retain exact values; normalization never uses binary floats or
rounds to a guessed tick. Base and quote volumes remain separate; no invented
close-times-volume equality is tested. The ignored field is printable
ASCII and retained without assigned semantics.

Missing buckets remain absent and are listed. An empty member publishes
`NO_OBSERVATIONS`; this cannot support a successful native import. No fabricated
fees, original definitions, feed receipt times, calendar, availability or PIT
evidence are generated.

There are no application-imposed archive, decoded-member, field, decimal,
provenance, evidence, record or manifest byte budgets. The explicit UTC day and
interval still determine the maximum possible nonduplicate bucket count, and all
gaps remain reported. The checksum must contain one SHA-256 entry for the exact
expected ZIP basename (text/binary checksum markers allowed), optionally followed
by one LF/CRLF. Extra entries, paths and wrong hashes fail. New manifest `limits`
values are `null`; verification preserves historical positive limit metadata
without rewriting the frozen envelope or using it as an input/output cap.

The [stdlib ZIP APIs](https://docs.python.org/3/library/zipfile.html) enumerate
exactly one regular member, compare its expected basename, reject path aliases,
directories/special-file metadata, duplicate/extra members, encryption flags and
unsupported compression. Only stored/deflate is accepted. A canonical public
`ZipInfo` name probe rejects NUL suffix aliases. A shallow copy of the member
retains the stdlib's local-header/overlap checks while its public decoded-size
field permits one byte past the member's declared size. This prevents a forged
declared size and matching prefix CRC from clipping away later rows. The original
size, CRC and metadata remain unchanged and are compared with the decoded bytes;
source size or CRC mismatches are rejected before CSV parsing.
There are no private ZIP API calls, extraction calls or handwritten container or
compression parsers. Advertised sizes, emitted bytes and CRC are checked; an
exposed nonzero first local-header offset is rejected as a prefix.

**This does not certify a canonical ZIP container.** The public APIs do not
provide a complete raw-envelope/consumed-compressed-byte validation contract.
Unused ZIP64 records, extra fields, comments, trailing payloads and noncanonical
disk/header metadata may be accepted when the stdlib accepts the selected member.
Do not infer independent enforcement of every disk-number flag, exact compressed
stream termination or absence of bytes outside the decoded member. Synthetic
tests explicitly demonstrate accepted ZIP64/trailing variants and preserve their
complete original bytes. Output names this limitation as
`STDLIB_MEMBER_VALIDATION_NOT_CANONICAL_ZIP`; verification's integrity scope is
`RETAINED_BYTES_AND_DECODED_MEMBER_ONLY`. The separate native profile below does
not relabel or broaden that frozen Python verification claim.

## Native conversion and preparation

The native profile `CLASSIC_SINGLE_MEMBER_STORED_OR_DEFLATE_V1` supports one
ordinary stored/deflate member, including supported comments/extra fields and
classic streaming descriptors. It refuses ZIP64, multi-disk, encryption,
unsupported flags/versions, prefixes and unsupported tails. Python integrity
verification may accept a broader container that this native profile refuses;
the original frozen v1 envelope and stdlib validation label remain unchanged.

Official public typed ZIP parsers validate the complete selected central span,
raw expected names, required version, local metadata and data extent. Independent
bounded decompression must reach a complete stream, reproduce the declared size
and CRC, and retain every row. No private APIs, copied parser implementation or
handwritten ZIP field parser is used. The native converter separately reproduces
the entire original manifest and normalized records from the retained ZIP,
checksum, provenance and evidence before producing artifacts.

For source-checkout use, supply the actual built `catalog-prepare` binary:

```sh
python3 -B runtimes/data/source_plugins.py verify binance-vision-spot-klines \
  --acquisition /absolute/private/new-archive-bundle/archive.json

python3 -B runtimes/data/source_plugins.py convert binance-vision-spot-klines \
  --native-bin target/debug/catalog-prepare \
  --acquisition /absolute/private/new-archive-bundle/archive.json \
  --instruments /absolute/original-binance-instruments.json \
  --output /absolute/native/binance-selection

python3 -B runtimes/data/source_plugins.py prepare binance-vision-spot-klines \
  --native-bin target/debug/catalog-prepare \
  --native-output /absolute/native/binance-selection \
  --declaration /absolute/original-discovery-declaration.json \
  --selection /absolute/original-native-selection.json \
  --output /absolute/prepared/binance-discovery
```

`freeze` in the source registry accepts the same explicit selection/file flags
as the standalone example above. Registry `verify` uses `--acquisition` pointing
to the final `archive.json`. Installed commands use the same operation arguments
through `manage.py source`, explicit read-only input mounts and a distinct
output-parent mount, without `--native-bin` or host Cargo. All six operations
remain offline.

Original native `CurrencyPair` definitions must match venue, symbol, base/quote,
precision and historical version clocks. They are never synthesized from the
symbol or current metadata. Decimal-to-native conversion rejects rounding,
unsupported precision, tick/quantity grids, future definitions and ambiguous
definition/receipt ties. The native catalog is read back with exact BAR and
definition equality, original source files are rechecked, and the final report
is published last. Failed/partial output is retained, not reused.

The report's native BAR event-label bounds and explicit receipt cutoff are
inspection inputs, not an automatically generated partition declaration.
Preparation still requires a separately supplied original declaration and
selection. Existing registration metadata requires microsecond-representable
timestamps; unsupported precision rejects rather than truncating original
nanoseconds. The resulting host-path/digest handoff uses the existing catalog
registration flow and does not itself register or qualify a dataset.

## Permission and remaining work

The [pinned Vision Dataset Terms](https://github.com/binance/binance-public-data/blob/bd110bb04caad6ad964a0098809f18343b1e104b/TERMS_AND_CONDITIONS.md)
incorporate Binance Terms of Use and treat dataset access/attempted access as
assent. This stage never makes archive, checksum, HEAD/range, listing or fallback
requests and offers no terms-acceptance flag. Real acquisition requires a
separate user-controlled agreement step and exact bounded retrieval/use
authority. Saved terms, public URLs, synthetic tests and file hashes do not
replace that action. No account, credentials, paid download, automatic agreement
acceptance, redistribution or public real-source fixture is introduced.

The offline native path does not supply actual observation evidence or a data-use
grant. Research still requires original
rights, historical definitions/fees, availability, partitions and fresh
`DATA_VALIDATE`; every manifest retains `UNPROVEN` coverage, `UNVERIFIED`
historical availability, false qualification/registration and independent
permission review. Keep original/evidence files private and outside researcher
catalog mounts. Do not infer venue-faithful cash simulation or historical
prediction eligibility from spot candle data.

## Offline verification

```sh
python3 -B -m unittest discover -s runtimes/data -p 'test_binance_vision.py' -v
python3 -B -m unittest discover -s runtimes/data -v
```

[Tests](test_binance_vision.py) generate small synthetic ZIPs in memory and use
temporary local files. They cover multiple symbols/dates/intervals, the ms/us
transition, exact amounts, sparse/full/empty days, malicious ZIP/CSV/provenance,
changed inputs and unpublished failures, CLI operation and unchanged acquisition
contracts. These Python tests do not establish native conversion or installation;
the separate Rust and installed Container gates execute those paths. Neither
establishes real endpoint availability, permission or scientific qualification.
See the [archive task](../../.opensdlc/tasks/binance-vision-spot-archive/task.md)
and [native integration task](../../.opensdlc/tasks/binance-native-preparation/task.md).
