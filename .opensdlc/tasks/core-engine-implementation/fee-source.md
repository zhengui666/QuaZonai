# Preparing an independent maker/taker fee source

Nautilus 0.67 instruments retain contract terms but no longer contain maker/taker
rates. An ordinary maker/taker backtest therefore needs a separately frozen venue
policy. Prices or a simulation request cannot supply that evidence.

The native source type is [FrozenMakerTakerFeesV1](../../../crates/contracts/src/catalogs.rs).
The existing data conversion commands still produce a new native catalog. During
preparation, pass an independently prepared JSON file with `--fee-source`; the
original converted catalog and fee file remain unchanged.

## Source fields and meaning

| Field | Required meaning |
| --- | --- |
| `schema_version` | Integer `1`. |
| `source_reference`, `source_version` | Original authorized source/capture identity and immutable revision. These strings are declarations; they do not authenticate a provider or prove applicability. Retain the original source evidence they identify. |
| `origin` | Fee-source origin, independently of price-data origin. Use `FIXTURE` for software fixtures and `SYNTHETIC` for invented research assumptions. `REAL` requires an actual original source; it does not certify historical availability, an account's fee tier, or PIT eligibility. `LEGACY_UNKNOWN` is rejected. |
| `venue` | Exact native venue, matching every rate's `instrument_id` suffix. |
| `currency`, `currency_precision` | Original native quote currency code and precision; both must match selected instruments. This path is the existing single-currency maker/taker path. |
| `available_at_ns` | Original time the policy was available, as a nonnegative decimal string. It must be no later than replay event start. Do not backdate a fee schedule retrieved today to justify earlier decisions. |
| `valid_from_ns`, `valid_until_ns` | Constant-policy interval `[from, until)`, as decimal strings. It must cover event start and extend strictly beyond decision cutoff. Actual native fill-clock time, including latency after the last BAR, must also lie inside it. |
| `rates` | Nonempty list with unique `instrument_id`, `maker`, `taker`. Rates are exact decimal strings expressed as fractions, with absolute value below one; negative maker rebates remain signed. Include every selected instrument. Confirmed zero must be explicit. |

Use the original venue/account fee source applicable to the selected instruments
and execution interval. Copy its exact rate values into the independent contract,
retaining the capture/revision and original availability facts. This preparation
path does not fetch account fees, infer a tier, verify a provider's statement, or
convert a current public rate into historical fee evidence. If applicability or
availability is unknown, restrict the research claim accordingly and do not invent
timestamps. A changing schedule cannot be represented as one constant policy.

The native model remains official `MakerTakerFeeModel::new` plus exact instrument
overrides. Its 0.67 commission path rounds each fill to the currency precision
with half-even ties. Exact Money storage does not change that policy: an
unrounded 0.0008 USD commission becomes 0.00 USD. This model choice is frozen by
the existing upstream class/version in execution settings; QZ does not install a
different commission formula or recover discarded fractions afterwards.

## Prepare without modifying source files

After the existing conversion step, use the supported source-plugin command. The
following paths identify files already obtained from the source owner; the command
does not generate fee evidence:

```sh
python3 -B runtimes/data/source_plugins.py prepare coinbase-candles \
  --native-output /data/converted/new-native-source \
  --declaration /data/inputs/catalog-declaration.json \
  --selection /data/inputs/native-dataset-selection.json \
  --fee-source /data/inputs/original-venue-fees.json \
  --native-bin target/release/catalog-prepare \
  --output /data/prepared/new-native-catalog
```

For an already authorized native catalog without a plugin import report, the
native preparation entry point accepts the same independent input:

```sh
target/release/catalog-prepare \
  --catalog /data/native/original-catalog \
  --declaration /data/inputs/catalog-declaration.json \
  --selection /data/inputs/native-dataset-selection.json \
  --fee-source /data/inputs/original-venue-fees.json \
  --output /data/prepared/new-native-catalog
```

Both commands require a new output directory. A fee file already present as
`frozen-maker-taker-fees.json` in the original catalog is preserved automatically;
if an explicit file is also supplied, both policies must agree. No request fee
values are copied into a missing source.

Preparation validates the source window, selected instrument identities, venue
and native quote currency, then publishes:

- `catalog/frozen-maker-taker-fees.json`: the independently read, validated policy.
- `catalog-metadata.json.execution_fees`: the same policy, bound to the selected
  catalog. Omit this declaration field to derive it from the independent file;
  if explicitly declared, it must match the file.
- The source-plugin receipt's `source_artifacts.fee_source`: the original fee-file
  identity, with the existing plugin's bytes/checksum conventions. The wrapper
  rechecks original inputs after execution and checks the published metadata and
  catalog fee file against that original input. Decimal spelling normalization
  does not permit a changed numeric rate.

For current v3, the owner configures the prepared catalog and its original request
in private `service.json.datasets`; see [dataset configuration](../../../apps/service/README.md#operator-dataset-configuration).
The preparation receipt does not register a retired Rust Runtime or mutate a
service. Preserve the original source rights, immutable inputs, partition
separation and independent validation. Metadata provenance alone never grants
source permission or research qualification. At backtest execution, request
`fee_rates` must exactly match the selected frozen source rates; missing, changed,
mismatched or expired fee evidence fails instead of becoming zero cost.

## Data-only and specialized models

Omitting `--fee-source` remains valid for data-only preparation and forecast use.
Ordinary maker/taker execution then reports `MAKER_TAKER_FEE_SOURCE_REQUIRED`;
prepare a new output with the independent file before rerunning that backtest.
Do not modify or relabel a historical catalog in place.

Polymarket native binary-option fees remain bound to the original instrument
`info.fee_schedule`, with official price-dependent commission and separate
redemption handling. Its 0.67 taker fee truncates to five decimal places; maker
rebates use five-place half-even rounding. Recorded Polymarket v2 fees keep their
original recorded fee pools. Spot cash keeps its existing explicit frozen spot
schedule and research-policy acceptance. These specialized models are not
replaced by this ordinary maker/taker source file.
