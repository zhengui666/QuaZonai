# TimeSeventeen Polymarket v2 → current v3 datasets

`polymarket-v2-hf` is an operator preset for the public
[TimeSeventeen/Polymarket-v2](https://huggingface.co/datasets/TimeSeventeen/Polymarket-v2)
repository. It reuses the existing HF cache and native `time-seventeen-v2`
converter. It does not import into a running installation, start research,
create permissions or certify the source.

## Inputs and boundaries

- Select explicit UTC dates `[start-date,end-date)` and one or more **outcome token
  IDs**, as exact positive uint256 decimal strings. A condition/market slug is not
  a token. The owner supplies the original mapping and native definitions.
- The preset selects exactly `OrderFilled/YYYY_MM_DD.parquet`. It does not use
  `daily_aligned*`, invent a QZ partition index or infer historical membership.
  Missing requested daily files fail, rather than silently narrowing the request.
- Planning resolves one immutable Hub revision and saves the full plan. Download
  uses that saved plan, not today's `main`. Tokens filter local native conversion;
  HF still transfers **whole daily files**. Optional `--max-bytes` limits selected
  complete-file sizes, including cached files, not cumulative transfer or disk use.
  There is no default budget and no default full-archive download.
- Only `plan` and `download` need public network access. Freeze, verification,
  conversion, preparation and configuration export are offline.
- Provide original externally tagged Nautilus `InstrumentAny` JSON definitions
  with `BinaryOption`, matching `raw_symbol` tokens and **pUSD** currency. Do not
  guess fees, precision, lifetime, historical clocks or settlements. This native
  archive format accepts one original definition per selected token, not a
  reconstructed definition-transition history; the token set must equal the request.

Observed on 2026-10-14 at revision
`c2a6429292da3a4c6e263c2c84351c77f9bce097`: the Hub inventory contained 193
`OrderFilled` files dated 2026-04-03–2026-10-12, totaling 47,826,598,821 bytes,
including 2026-09-10. This is a dated filename/size observation, not a download,
record-level reconciliation or gap-free coverage claim. The card declares
CC-BY-4.0; retain publisher attribution and applicable source terms. Public access
and a recorded card do not independently establish all intended use rights.
The older inventory in the [history guide](README.md#source-selection) remains
an observation of its original revision.

## Source-checkout workflow

Run from the trusted source checkout with Python 3.10+, Node >=22.19, service and
research npm dependencies, and matching native executables. The paths below are
placeholders; replace `TOKEN_ID_FROM_ORIGINAL_DEFINITION` with an actual selected
token. Each publication uses a new output path. Keep downloaded data separate
from scripts, outside Git and outside researcher-readable evidence mounts.

### 1. Save a plan, then acquire only its files

```bash
python3 -B runtimes/data/source_plugins.py plan polymarket-v2-hf --start-date 2026-10-10 --end-date 2026-10-11 --token TOKEN_ID_FROM_ORIGINAL_DEFINITION --revision c2a6429292da3a4c6e263c2c84351c77f9bce097 --max-bytes 300000000 --output /absolute/v2/plans/selection-01.json
```

Inspect its fixed revision, original token/date request, file list, license and
`total_bytes` before downloading. Repeat `--token` for other original definitions.
The token above is intentionally not a sample market or a fabricated definition.

```bash
python3 -B runtimes/data/source_plugins.py download polymarket-v2-hf --plan /absolute/v2/plans/selection-01.json --cache-dir /absolute/v2/cache --output /absolute/v2/requests/request-01
```

If all original files already exist in the shared cache, use offline freeze
instead of download, with a different new request directory:

```bash
python3 -B runtimes/data/source_plugins.py freeze polymarket-v2-hf --plan /absolute/v2/plans/selection-01.json --cache-dir /absolute/v2/cache --output /absolute/v2/requests/offline-01
```

```bash
python3 -B runtimes/data/source_plugins.py verify polymarket-v2-hf --selection /absolute/v2/requests/request-01/selection.json
```

Downloads retain partial bytes and resume through the existing shared per-file
locks/HTTP Range path. Retry the identical saved plan/request; conflicting input
cannot reuse it. Freeze requires every complete file and never downloads or
resumes. No source-file checksum is computed on this HF-selection path. Size,
format envelope and stat checks are not Parquet decoding, upstream authenticity
or historical availability. See [cache recovery](hf-dataset.md#shared-cache-and-recovery).

### 2. Convert original tokens to native BARs

```bash
python3 -B runtimes/data/source_plugins.py convert polymarket-v2-hf --selection /absolute/v2/requests/request-01/selection.json --instruments /absolute/v2/inputs/original-instruments.json --bar-seconds 60 --native-bin target/debug/polymarket-history --output /absolute/v2/native/converted-01
```

Omit `--native-bin` in installed execution. Optional paired `--start-seconds` and
`--end-seconds` narrow the original UTC dates; they cannot widen them. Optional
`--chain-evidence` supplies an original supported Polygon evidence file, not a
new acquisition or guessed correction.

Source fills use `[start-seconds,end-seconds)`. BARs are interval-end labeled and
can equal the source end. Native scientific selections independently use
exclusive nanosecond `event_end_ns`; the preset never adds a nanosecond or moves
a caller's cutoff to include a closing BAR. Choose your original scientific
bounds explicitly and honor the metadata's microsecond datetime precision.
Empty intervals remain absent. Strict native six-decimal float recovery,
canonical chain/block/log deduplication, exchange-summary exclusion and original
clocks are unchanged. Bad amounts fail, not round or disappear. Derived source
`price` and aggressor labels are not authoritative; `usdc_amount` is not a reason
to substitute USDC for pUSD. [Native source limitations](README.md#quality-research-admission-and-recovery)
remain applicable.

### 3. Prepare the explicit scientific selection

```bash
python3 -B runtimes/data/source_plugins.py prepare polymarket-v2-hf --native-output /absolute/v2/native/converted-01 --declaration /absolute/v2/inputs/original-discovery-declaration.json --selection /absolute/v2/inputs/original-discovery-selection.json --native-bin target/debug/catalog-prepare --output /absolute/v2/prepared/discovery-01
```

The second `--selection` is **NativeDatasetSelectionV1**, not HF `selection.json`.
Use the original [native declaration and partition contract](README.md#prepare-isolated-research-partitions),
including `provider_kind=NAUTILUS_CATALOG`, actual origin, membership, calendar,
availability, identities and partition. Omit measured `row_count` and `quality`;
native preparation measures, copies and reads back the actual catalog before
publishing `catalog-metadata.json`. This archive's PIT must remain `UNVERIFIED`.
Missing fees, future definitions and malformed declarations remain failures.

Success saves `preparation.json` beside (not inside) `catalog/`. It retains the
fixed source reference, date/token request, actual native window, original
scientific selection and source-report paths. It also retains canonical typed
native definitions as lossless JSON text for preprocessing compatibility; original
uint64 clocks are never compared through a rounded JavaScript Number. Raw input
definitions remain in detached evidence; optional omitted fields are normalized
only by the existing native serializer. Original raw cache is no longer
needed for preparation or export; retain it separately for audit. Do not treat
the receipt or metadata as an independent source attestation.

### 4. Export a DatasetConfig fragment

```bash
python3 -B runtimes/data/source_plugins.py export-dataset polymarket-v2-hf --prepared /absolute/v2/prepared/discovery-01/preparation.json --request /absolute/v2/inputs/original-factor-request.json --id polymarket-v2-selection-01 --name 'Polymarket v2 selection; PIT UNVERIFIED' --output /absolute/v2/config/dataset-01.json
```

The output parent must exist. The exporter checks the actual receipt, native
metadata/selection, nonempty Parquet framing, original small import report and
original factor request. It does not decode Parquet again or replace Rust
scientific validation. Exact request nanosecond/fuel/decimal strings are
preserved. Duplicate JSON keys, unknown native request fields, invalid period/fuel
constraints and event-end/cutoff contradictions fail without silently repairing
original inputs. Optional `--policy FILE` and `--strategy FILE` copy explicit
compatible originals, not automatically fitted strategies. Strategy instrument,
weight, TTL, threshold and pUSD currency must satisfy the existing consumer.
The default legacy portfolio consumer needs the full final request on the primary
catalog. With explicit `forecastMode: "price-volume"` and
`evaluationPartition: "HELDOUT"`, the consumer instead captures and replays the
original compatible Sealed binding. A genuine prefix-only primary plus full Sealed
catalog is supported in that mode, including ensembles. Do not copy held-out values
into the researcher-visible primary. `ready` defaults to **false**;
`--ready` is owner technical enablement only, never scientific qualification.
Existing outputs and symlinks are refused without replacement.

For independent evaluation, separately prepare a `SEALED` catalog and supply
**both** `--sealed-prepared RECEIPT` and `--sealed-request ORIGINAL_FACTOR.json`.
The full held-out request must include the same train/validation criteria and
parameters: the current normalized, validation-clipped `iterationRequest`
values must be identical. Token sets are compared without changing original
argument order. Original typed native preprocessing definitions and BAR interval
must also match; both conversions must start at the same original source bound,
retain the same first measured observation and use a Sealed source end at least
as late as the primary's. A full declared selection alone cannot
turn physically suffix-only data into the original causal prefix. Arbitrary
disjoint suffix-only requests are rejected.
The primary catalog must cover iterative input; the Sealed catalog must cover
the full original independent request. Keep Sealed catalog values outside
researcher-readable mounts; use the evaluator's original binding. No held-out
source values or detached evidence are inserted into exported `data`.

The fragment has `data={catalog,request,sealed?}` for the current TypeScript
service. An owner reviews it and merges **one entry** into the existing private
`service.json.datasets` array, preserving all other configuration, data and
credentials. Mount original catalog paths verbatim wherever native jobs run.
Do not replace the entire configuration with this fragment. There is no current
Runtime/DataGrant registration flow or POST import route; `/api/v3/datasets` lists
configured entries. Export neither edits that configuration nor restarts the
service nor starts research.

The service captures input **JSON**, not immutable catalog bytes. Keep prepared
catalogs unchanged/read-only for every replay and independent evaluation; an
exported path is not a global physical freeze. Permission, PIT, coverage,
independent evaluation and economic evidence remain separate from computation.

### Conditional recorded-cost research

Prepare original costs for a caller-selected native BAR window with the operator command:

```sh
source-tools polymarket-history recorded-fees --selection /absolute/original-hf-selection.json --catalog /absolute/original-data-only/catalog --bar-selection /absolute/original-native-bar-selection.json --output /absolute/new-prepared/catalog/recorded-fees
```

`--bar-selection` is the existing `NativeBarSelectionV1`, not an HF selection or a market shortlist supplied by this command. It selects one-minute LAST BARs with bare outcome-token instrument IDs (`TOKEN.POLYMARKET`) from the original data-only catalog. The output parent must exist; the output directory must be new. The command does not change either source, fabricate definitions, register data, restart research or select a fixed universe. The output is the existing per-instrument `RecordedTradeFeesV1` layout, not a new fee model. The consuming prepared catalog must retain the same original quantities and BARs.

Official Arrow/Nautilus decoding reads only selected native series. Every selected BAR's volume must equal its original native TradeTick quantities. Original HF costs are then joined by canonical transaction/log ID, token and timestamp; the native quantity is copied, never reconstructed from rounded source amounts. Original source quantity, fee, cash, provider price and directions are retained. Equal repeated cost records are consumed once; conflicting records, missing costs/records and volume mismatch fail before output creation. Source zeros remain zeros. Raw source files remain detached from researcher mounts; prepare prefix and independent-evaluation inputs separately. This prepares explicitly conditional cost replay, not a historical fee schedule, actual book depth, measured reception latency or PIT qualification.

`POLYMARKET_V2_RECORDED_FEES` reads `recorded-fees.json` from each original catalog.
It is a QZ adapter through the official native `FeeModel`, not a venue fee formula.
Keep the original source revision, transaction ID, timestamp, `fee_usdc`, cash
amount, price and provider direction fields. Preserve raw `token_amount` as
`source_quantity` and join the same-ID original native TradeTick quantity as
`quantity`; do not renormalize quantities or guess absent fees. Each selected BAR's
native volume must equal the sum of its linked original native trade quantities.

For this explicitly conditional cost replay, the actual shared native simulation
clock selects its latest completed one-minute boundary. That BAR's source records
are consumed once in `(timestamp,id)` order; each portion costs the original fee
times allocated/native-record quantity. Round the sum once per native fill, half-even
at pUSD precision. Source zeros remain zeros; missing BARs/records, null fees and
exhausted source quantities fail. This does not infer historical maker/taker
eligibility or turn provider directions into exchange aggressor evidence. Native
results retain actual fill time, source BAR time and each original record/allocation;
Nautilus books the returned commission once. No global-average or fitted fee rate
is charged. Keep prefix and Sealed fee records physically separate.

`POLYMARKET_V2_RECORDED_LIQUIDITY` is an explicit, catalog-backtest-only fill
policy paired with the recorded-fee adapter, native version `0.63.0` and the
closed native fill probability/seed parameters. It previews that same original
record pool at the actual replay clock. Positive MARKET IOC fills are capped by
both order leaves and remaining source quantity; only the actual fee callback
consumes records, across both order sides. Native IOC cancellation prevents L1
remainder extrapolation. A canceled reduction abandons its dependent increases;
subsequent targets use actual holdings, not assumed released margin. The native
engine still owns fills, cancellations, positions and money. The book uses the
native bid/ask, with seeded slippage applied once by Nautilus after the book fill.
Missing/exhausted pools and unsupported orders fail explicitly; returning an
empty book would silently fall back to ordinary liquidity. Original callback
failures are observed after replay even when the upstream engine only logs them.
This policy is a disclosed quantity-allocation assumption, not historical book
depth, maker/taker eligibility or available contemporary liquidity. Declare it
in a new research binding; the legacy default-fill policy remains unchanged.

The share-price accounting proxy is not a historical BinaryOption definition.
Nautilus CurrencyPair requires the single-currency account to be MARGIN, so this
conditional model uses leverage 1, nonnegative weights/cash and gross exposure at
most 1; it does not claim physical cash-token inventory. Instrument maker/taker
fields and matching `fee_rates`, if supplied as an observed-training sizing reserve,
are not the charged commission; the explicit recorded-fee adapter overrides them.
Disclose model ticks/quantity units, positive latency, OHLC fills and slippage,
the USD/pUSD conversion assumption and native 252-day daily-return Sharpe convention.
No payout, historical execution or executable HFT claim follows from these inputs.

## Installed disposable containers

Use the matching application image from the installation manifest as `APP_IMAGE`,
not the scientific compiler image. The image includes Python modules, the Node
exporter and native `source-tools` dispatch shims; no host Node/Rust build is
needed. Use explicit same-path input/output mounts. Network is allowed only for
planning/download; never pass provider credentials or the service environment.
See the [installed source-tool boundary](../../deploy/docker/README.md#scientific-images-and-source-tools).

For example, with existing owner-managed directories and a new output:

```bash
docker run --rm --init --network none --read-only --user "$(id -u):$(id -g)" --mount type=bind,src=/absolute/v2/requests,dst=/absolute/v2/requests,readonly --mount type=bind,src=/absolute/v2/cache,dst=/absolute/v2/cache,readonly --mount type=bind,src=/absolute/v2/inputs,dst=/absolute/v2/inputs,readonly --mount type=bind,src=/absolute/v2/native,dst=/absolute/v2/native --entrypoint /usr/bin/python3 "$APP_IMAGE" -E -s -B /opt/quazonai/operator/source_plugins.py convert polymarket-v2-hf --selection /absolute/v2/requests/request-01/selection.json --instruments /absolute/v2/inputs/original-instruments.json --bar-seconds 60 --output /absolute/v2/native/converted-02
```

Preparation similarly mounts native/inputs read-only and its separate prepared
parent writable. Export mounts prepared catalogs, inputs and original small
import reports at their recorded paths read-only; only its configuration output
parent is writable. Mounting the whole native parent is convenient but not
required: the source report is needed, raw source evidence/cache is not.
Freeze needs a writable cache mount for locks, even though source bytes are only
read. Verification can mount cache and request read-only. Do not shadow inputs
with overlapping writable mounts. Use these commands directly, **not** the
retired `manage.sh source` command.

## Verification scope

`make check-source` runs shared transport/preset tests; service tests cover
configuration preservation and actual v3 listing. The `polymarket_v2_source`
Rust integration uses real synthetic public-schema Parquet, actual native
conversion/preparation/readback, Python→Node export and Wasmi evaluation with
separate iterative/Sealed bindings. Its records and definitions are explicitly
`FIXTURE`, not downloaded source or market/PIT qualification. Applicable CI uses
that test through `make check-science`; packaging smoke checks the installed
inventory and exporter. Actual results and remaining checks are recorded in the
[task](../../.opensdlc/tasks/polymarket-v2-hf-source/task.md#verification).
