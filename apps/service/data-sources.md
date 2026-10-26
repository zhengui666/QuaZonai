# Data-source extensions

Manage datasets in **Settings → Data** and providers in **Settings → Data Plugins**. The extension directory lists providers as `kind: data` and links to their management page.

`qz.data.config` reads the existing `service.json.datasets` array. `qz.data.local-config` loads a JSON file containing one exported DatasetConfig or an array. Configure its file path, save, enable it and load data. Relative file paths resolve from the service data directory; catalog paths inside the file resolve from that file's directory. Existing data tools can export this format. A bare catalog directory needs the original research request, partitions and policy before it can be registered.

Saving configuration does not start acquisition. **Load Data** calls the provider and replaces its successful dataset result. A failed load retains the previous result and reports the error. Disable removes those datasets from new selections while keeping their stored result, files and historical research. Re-enable restores the stored result. Configuration and successful datasets live in `data-sources.json` and survive restart; startup does not automatically download or convert data.

Existing investment decisions retain source lookup when a provider is disabled or a refresh omits a previously published dataset. Their monitors continue reading the retained source configuration, including after restart; actual source read failures keep their normal behavior. New admissions still use only the enabled, current dataset list.

## External providers

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

New providers implement source-specific acquisition or reuse the existing [operator tools](../../runtimes/data/source-plugins.md), then return actual prepared datasets. Downloaded files alone do not become research-ready data. Duplicate dataset IDs across enabled sources fail the update instead of silently replacing another source. Current backtest settings bind newly selected datasets; captured research keeps its original input.

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
