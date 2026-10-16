# Public-source acquisition plugins

This is an **operator-only acquisition foundation**, not a complete research-source integration. The shared runner supports two real public APIs without accounts, secrets, paid fallbacks or trading. Its normalized observations are inspection files, not native Nautilus data or qualified datasets. Scientific jobs remain offline; no service, HTTP contract or database admission path is added.

Native preparation is available separately through the [operator source-plugin registry](source-plugins.md). This guide describes the unchanged acquisition-only contract; frozen provider descriptors are not rewritten when preparation capabilities are added. Capabilities describe implemented interfaces, subject to the [current source-use boundary](source-plugins.md#source-rights-and-acceptance).

## Supported capabilities

| Provider | Explicit instrument | Acquired record | Selection and limitations |
| --- | --- | --- | --- |
| `polymarket-prices` | Decimal outcome token ID | Sampled `PRICE_MARK` | Source timestamp in `[start,end)`. Fidelity is a whole number of minutes. Marks carry no trade size, book, fee, settlement or historical instrument definition. Never convert them into synthetic trades or OHLCV. |
| `coinbase-candles` | Exchange product ID, such as `BTC-USD` | Original `OHLCV_CANDLE` | UTC bucket start in `[start,end)`; event time is bucket end. Supported intervals: 60, 300, 900, 3600, 21600 and 86400 seconds. Both selection boundaries must align to complete buckets. Missing buckets stay missing. |

The [Polymarket endpoint](https://docs.polymarket.com/api-reference/markets/get-prices-history) uses `market`, `startTs`, `endTs` and minute `fidelity`. The [Coinbase endpoint](https://docs.cdp.coinbase.com/api-reference/exchange-api/rest-api/products/get-product-candles) documents a 300-candle limit, potentially incomplete history, omitted no-tick intervals and responses containing earlier candles. These contracts were checked on 2026-09-30. Acquisition does not establish a gap-free history or original publication times.

`providers` prints machine-readable record types, terms references, limits of interpretation and `native_conversion=UNSUPPORTED`. A provider implements `Provider.plan` and `Provider.decode` in [providers.py](providers.py), and is inserted once in the static registry. The same runner then plans, acquires and verifies it without provider-specific CLI dispatch. Do not add dynamic code loading, arbitrary URLs, account setup or another domain service.

## Plan, download and verify

Python 3.10+ and the standard library are sufficient. Run from the repository root. Plans are offline, deterministic and require an explicit instrument, time range and interval:

```sh
python3 -B runtimes/data/acquire.py providers

python3 -B runtimes/data/acquire.py plan \
  --provider coinbase-candles --instrument BTC-USD \
  --start-seconds 1704067200 --end-seconds 1704153600 \
  --interval-seconds 3600
```

That example selects the 24 hourly bucket starts on 2024-01-01 UTC. Plans show the exact requests and bounds; they do not promise the endpoint retains those observations.

Before downloading, independently establish permission for the intended use and preserve the applicable source terms in a local file. Neither provider is assigned an open-data license. The Coinbase download interface below is conditional on resolving the [source-use block](source-plugins.md#source-rights-and-acceptance); it is not a live-validation instruction. [Polymarket terms](https://polymarket.com/tos) must also be assessed for the intended use. A supplied terms file is retained as unverified operator evidence, not an automatic grant or proof of legal compliance. The existing QuaZonai data-use grant and research-admission checks remain independent.

```sh
python3 -B runtimes/data/acquire.py download \
  --provider coinbase-candles --instrument BTC-USD \
  --start-seconds 1704067200 --end-seconds 1704153600 \
  --interval-seconds 3600 \
  --terms-file /absolute/original-coinbase-market-data-terms.txt \
  --output /absolute/data/coinbase-btc-2024-01-01

python3 -B runtimes/data/acquire.py verify \
  --output /absolute/data/coinbase-btc-2024-01-01
```

For Polymarket use `--provider polymarket-prices`, an original outcome token ID as `--instrument`, and an interval divisible by 60. The interval specifies sampling fidelity, not proof of a regularly spaced series. Token discovery and market-universe membership are separate evidence; the runner does not silently select today's universe for historical research.

## Frozen evidence and failure behavior

The output contains:

- `raw/NNNN.json`: byte-for-byte response bodies, including rows outside requested bounds
- `source-terms.bin`: the unchanged supplied source-terms file, with its provider reference and checksum
- `records.jsonl`: sorted selected observations, decimal price/volume strings, original row index and response file, event/selection timestamps and the actual retrieval clock
- `acquisition.json`: the final publication marker, containing provider/version/capabilities, original selection, exact requests, response headers/clocks, file sizes/SHA-256, counts, budgets and explicit admission limitations

Original JSON numbers are decoded directly to decimal values without a binary-float round trip. No tick-size rounding, assumed currency precision, zero filling, backdated metadata, fees or settlement is introduced. Repeated timestamps, invalid OHLC relationships, negative volume, malformed JSON, duplicate object keys and non-finite or unsupported numeric values fail before final publication. There is no application-imposed decimal digit or exponent budget. Source numbers remain exact; protocol-invalid or non-finite values still fail.

Every normalized row has `historical_available_at=null`. `observed_at` means this request finished locally. For a candle, bucket end is its event time; neither bucket end nor retrieval proves when the historical market first made that candle available. An empty selection publishes `NO_OBSERVATIONS` with zero rows, never a filled time series.

Requests use fixed official HTTPS hosts, GET only, no credentials/cookies and no redirects. Every page needed by the explicit selection is planned; there is no total request-count cap. Polymarket windows span at most one day; Coinbase windows select at most 299 buckets so a returned end-boundary bucket can fit within its 300-row cap. Each response is independently filtered to its planned half-open window. Out-of-window rows are counted and remain in raw evidence. Requests are sequential, separated by 350 ms. HTTP errors, payment/authentication responses, malformed bodies and network failures are final; there is no retry or fallback.

No implicit response, record, source-terms, manifest or output byte budget is imposed. `--max-bytes` optionally sets an explicit cumulative response-body budget. Connections and idle socket reads retain a 30-second timeout; healthy transfers have no cumulative wall-time deadline. A response without content length that exactly exhausts an explicit byte budget fails closed rather than reading beyond it. Manifest `limits` keys remain, with `null` for uncapped values; historical positive limits remain immutable provenance when verified.

Publication reuses [snapshot.py](snapshot.py)'s symlink checks and atomic no-replace writes. The output directory must be new. Neither completed nor interrupted API captures are refreshed or resumed; use a new directory after a failure. Complete response bytes are retained before parsing, including a malformed or unsupported body that stops interpretation. Partial directories remain unpublished without `acquisition.json`; an incomplete network body or response rejected by transport checks is not retained. Existing user files are never replaced.

`verify` is offline. It checks file bytes, reconstructs the provider plan, reinterprets raw responses and compares the entire normalized result. Local provenance clocks must be timezone-aware UTC, each request must finish before the next starts, and publication cannot precede any retrieval. The runner applies those same ordering checks before publication. It cannot authenticate an upstream publisher or establish independently verified source rights, coverage, historical availability or research qualification. A local checksum is not an upstream signature. Every manifest retains `coverage=UNPROVEN`, `historical_availability=UNVERIFIED`, `research_qualified=false`, `registered_in_quazonai=false` and an independent-permission-review requirement.

## Existing boundaries and remaining work

The [Hugging Face snapshot downloader](snapshot.py), [corroborated EVM collector](evm.py), [Polymarket native importer](../../apps/job/src/bin/polymarket-history.rs) and [catalog preparation](../../apps/job/src/bin/catalog-prepare.rs) retain their separate source and preparation contracts. This acquisition manifest does not replace them.

The [source-plugin registry](source-plugins.md) supplies native Coinbase BAR conversion through `catalog-prepare ingest-candles` and preparation of supported native BAR catalogs. Conversion retains original definitions, exact precision and actual retrieval clocks; it does not establish historical forecast/PIT eligibility. The `polymarket-history` importer requires `BinaryOption` instruments on `POLYMARKET`; Coinbase candles use the separate native candle importer.

An admitted research workflow still requires:

1. Source-specific historical instrument/universe, fee, calendar and availability evidence. Current product metadata must retain today's observation time. A new metadata request does not repair historical PIT. Polymarket sampled marks cannot satisfy the scientific BAR contract
2. Original declarations and selections for isolated partitions, explicit data-use grants, Runtime catalog configuration, service source/dataset registration, frozen InputSets and fresh `DATA_VALIDATE` through the existing registration path. Successful acquisition, conversion or preparation cannot replace qualification

The operator tools do not automate those service steps. Broader source adapters, independent historical coverage checks and streaming remain separate extensions; generic file acquisition does not imply support for converting their records.

## Verification

```sh
python3 -B -m unittest discover -s runtimes/data -v
```

The existing historical-data workflow discovers these tests. Offline tests cover both source schemas, exact decimals, pagination/request budgets, missing/extra rows, interrupted or duplicate data, atomic no-overwrite publication, symlinks, changed hashes/derived records, unsupported capabilities and unchanged admission flags. They do not require accounts or external data and do not establish live availability.

### Bounded live observations, 2026-09-30

These observations are separate from fixture regression tests and establish no general coverage, permission or native import:

- Polymarket: `market=32338220190071351435772801779725302244575775216413325951443816017994629993401`, `startTs=1790702858`, `endTs=1790789258`, `fidelity=1`. GET `/prices-history` started at `2026-09-30T17:31:05.201838Z` and completed at `17:31:13.406813Z`. The 40,362-byte JSON response SHA-256 was `d150f635e2ac52e0b83a73b64bf5e5781c6ef77c833d66f366e180bf62f17f12`. It decoded to 1,441 original marks, 1,440 inside the selection and one beyond the requested end, which the local filter excluded
- Coinbase: GET `/products/BTC-USD/candles?start=2024-01-01T00%3A00%3A00Z&end=2024-01-01T01%3A00%3A00Z&granularity=3600` started at `2026-09-30T17:32:05.281873Z` and completed at `17:32:13.056265Z`. This environment returned HTTP 200, `text/html`, and a 195-byte “Site Unavailable” page with SHA-256 `5b131ca14aa96311d3432b0062c7443d3b0e6346ec279bd376a75f0a76bcd5d7`. The runner rejects that MIME type. Coinbase live schema/end-to-end acquisition is **not verified** here; its adapter has official-contract and offline-fixture coverage
