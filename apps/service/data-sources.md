# Data-source extensions

Manage datasets in **Settings → Data** and providers in **Settings → Data Plugins**. The extension directory lists providers as `kind: data` and links to their management page.

`qz.data.config` reads the existing `service.json.datasets` array. `qz.data.local-config` loads a JSON file containing one exported DatasetConfig or an array. Configure its file path, save, enable it and load data. Relative file paths resolve from the service data directory; catalog paths inside the file resolve from that file's directory. Existing data tools can export this format. A bare catalog directory needs the original research request, partitions and policy before it can be registered.

Saving configuration does not start acquisition. **Load Data** calls the provider and replaces its successful dataset result. A failed load retains the previous result and reports the error. Disable removes those datasets from new selections while keeping their stored result, files and historical research. Re-enable restores the stored result. Configuration and successful datasets live in `data-sources.json` and survive restart; startup does not automatically download or convert data.

Existing investment decisions retain source lookup when a provider is disabled or a refresh omits a previously published dataset. Their monitors continue reading the retained source configuration, including after restart; actual source read failures keep their normal behavior. New admissions still use only the enabled, current dataset list.

## External providers

The built-in disabled `qz.data.hyperliquid` provider loads existing public history
from the fixed Hugging Face archive profile, with optional exact 100ms native
trade aggregation. It never opens a live market feed. See the
[history guide](../../runtimes/data/hyperliquid.md) for source coverage, local
archives, downloads, native conversion and research-admission boundaries.
Archives appear with `ready: false`; they do not bypass native preparation.

Register a module in the existing service configuration:

```json
{
  "extensions": [
    {
      "name": "example.data",
      "module": "./providers/example-data.mjs",
      "kind": "data",
      "enabled": true,
      "config": { "path": "./exports/datasets.json" }
    }
  ]
}
```

The module path resolves from the service configuration file. Its default export implements [DataSourcePlugin](src/data-sources.ts):

```js
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

export default {
  name: 'example.data',
  version: '1',
  fields: [{ key: 'path', label: 'Dataset file', type: 'text', required: true }],
  async load(config, { dataDir }) {
    return JSON.parse(await readFile(resolve(dataDir, config.path), 'utf8'));
  },
};
```

`load` returns `DatasetConfig[]`, with absolute catalog and revision-manifest paths. The built-in local-file provider already handles relative catalog paths. Providers can expose text, password, number and boolean fields; the settings UI renders them directly. Provider configuration is private owner configuration; return dataset metadata and research inputs without secrets. API and plugin exceptions use public error codes rather than including credentials or arbitrary exception text.

New providers implement source-specific acquisition or reuse the existing [operator tools](../../runtimes/data/source-plugins.md), then return actual prepared datasets. Downloaded files alone do not become research-ready data. Dataset IDs remain owned by their original source, including retained history: that source can update its IDs, while another source cannot reuse them and redirect existing decisions. Current backtest settings bind newly selected datasets; captured research keeps its original input.

These modules run in the service and are separate from Pi Durable research extensions. Install/configure the module on the service host and restart for discovery; save, enable, disable and load the discovered source through Settings. Module installation is explicit service configuration, with no browser-side package execution.

## HTTP API

All routes use the existing owner authentication:

| Route | Behavior |
| --- | --- |
| `GET /api/v3/data-sources` | Providers, fields, saved configuration, enabled state, dataset IDs, last successful load and error |
| `PUT /api/v3/data-sources/:name` | Save `{ "enabled": true, "config": { ... } }` |
| `POST /api/v3/data-sources/:name/refresh` | Load and register the provider's datasets |
| `GET /api/v3/extensions` | Includes data providers with `kind: data` |
| `PATCH /api/v3/extensions/:name` | Applies data-provider enablement immediately |
| `GET /api/v3/datasets` | Current datasets available to new research and portfolio selection |

## Review source evidence

A source plugin may supply `DatasetConfig.reviewSourceEvidence` together with its
existing `revisionManifest`. Both descriptors use paths relative to that manifest's
directory and exact lowercase SHA-256 values:

```json
{
  "metadata": { "path": "validation-metadata.json", "sha256": "<64 lowercase hex characters>" },
  "license": { "path": "research-grant.json", "sha256": "<64 lowercase hex characters>" }
}
```

The files must also occur in the manifest's `files` array with identical size and
hash. Every catalog file remains covered by that same immutable source manifest.
The registered dataset's scientific inputs must exactly match the manifest's
resolved data; connection credential references do not change scientific identity.
A source revision cannot be reused for different snapshot bytes. These are server
source-plugin settings; `ResearchInput`, model tools and HTTP review metrics cannot
supply replacement provenance.

The metadata file uses the existing Rust `RuntimeCatalogMetadataV1` schema. It must
be the source owner's original `VALIDATION` metadata: `registered_ref` equals the
configured dataset ID, `native_snapshot_ref` equals the manifest's `sourceRevision`,
and its sole quality report describes precisely the registered validation interval
and availability cutoff. The native `review-source` command validates the complete
metadata, instrument/universe and quality relationships through the existing domain
validators. A command completion does not itself create `REAL` origin, verified
historical availability or permission.

The grant file uses `ReviewSourceLicense`: `schemaVersion: 1`, the exact `datasetId`,
`sourceId`, `sourceRevision` and native `datasetRevisionId`, an original `grantId`,
`issuer` and `reference`, `status: "ACTIVE" | "REVOKED"`, declared `purposes` including
`"RESEARCH"`, and exclusive nanosecond-string bounds `validFromNs`/`validUntilNs`.
The native quality report's `dataset_revision_id` must equal that grant identity.
A missing grant, unknown source origin, unverified PIT, restated history, expiry,
revocation, changed bytes or invalid native metadata cannot pass the provenance gate.

Admission freezes the verified source documents and digests in a separate official
Durable source-evidence document before the model starts. The review report cites
that source evidence; the model receives a provenance summary rather than raw
metadata or sealed values. The recorded PIT explanation remains the configured
source owner's evidence, attributed as such. Hash verification and structural native
validation do not independently reconstruct historical receiver clocks. Datasets
without these documents remain usable for evidence-limited exploratory research and
receive `INCONCLUSIVE` for automatic review release.

## Quantitative review evidence

New research freezes its quantitative Review policy. Missing historical availability,
license, reference calibration, execution or independent evidence produces an
`INCONCLUSIVE` report, even when the native calculation succeeds.

A trusted source plugin or server DatasetConfig may register `reviewSourceEvidence`:
`metadata` and `license` each contain a relative `path` and its `sha256`. The files
must already appear, with matching byte counts and hashes, in the dataset's
`revisionManifest`. The metadata uses the existing native catalog metadata schema;
its registered dataset, source revision, dataset revision and Validation selection
must match the admitted input. The source-owner license uses
`packages/contracts/src/review-source.ts`. Merely setting claims in ResearchInput,
or uploading metrics through HTTP, cannot establish source qualification.

For a separate independent window, configure `heldoutMetadata` and, when its native
dataset revision differs, `heldoutLicense`. The metadata's native partition is
`SEALED`; the factor/portfolio evaluator calls that purpose `HELDOUT`. It must cover
the actual independent selection, not the selection used for fitting weights.
Source-owner evidence and native schema/quality validation are reported separately
from independently demonstrated market results.

`POST /api/v3/review-policies` derives efficacy q10/q90 anchors from at least 30
historical reference roots. Coverage, stability fractions, mechanism fractions and
probabilities retain versioned fixed scoring ranges; constant valid values on these
bounded scales do not invalidate calibration. Every metric still requires 30 legal
source observations. Supply `requestId`, `kind`, `evaluationStartNs` and
`researchIds`, or `allocationIds` for `ALLOCATION`; evidence is loaded from retained
Validation reports. Missing or degenerate reference evidence is rejected. Register
the resulting policies for the corresponding data, frequency and prediction horizon
before starting candidates; do not tune them from independent outcomes.

A research start can include `reviewedFactorIds` referencing single-factor versions
with earlier PASS reports. The service resolves and freezes model bytes, report
versions and original Validation end times. The `strategy_ensemble_fit` tool accepts
`sourceFactorIds` from that frozen set; only evidence strictly earlier than the new
TRAIN cutoff can influence the fit. The model never supplies review scores or q.

The portfolio page exposes multi-portfolio configuration with explicit member,
cluster and turnover constraints and a scientific-call budget. Optional
`independentDatasetId` selects a separate registered window. Weights always create
a new candidate and shared-account study. The original target delivery conditions
remain required; a configuration report does not send an order.

An independently consumed suffix requires its own `heldoutMetadata` descriptor
(and `heldoutLicense` when its native dataset identity needs a different grant).
That original metadata must declare `SEALED`; its quality selection must match the
actual held-out interval. Validation metadata or a constituent's source report
cannot substitute for it. The documents are checked before the independent read.
A new factor import, portfolio or allocation also re-reads the current configured
source and grant: retraction, revocation, expiry, missing files and changed scope
block new use. A changed source revision requires new evidence rather than renewing
an old review; historical reports remain readable.
