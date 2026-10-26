# Settings, direct instance credentials and data-source extensions

<a id="task"></a>
## Task

The owner requested removal of Origin restrictions, direct instance credentials with masked inputs, and data management under Settings through an extensible data-source mechanism. Start from current remote `dev` and merge the completed change back into `dev`.

<a id="intent"></a>
## Intent

Allow the owner's clients to access the authenticated HTTP API from any Origin. Let the owner enter instance tokens directly. Move datasets into Settings and give data plugins their own management page. Preserve existing data, applied migrations, research identities and unrelated work.

<a id="spec"></a>
## Requirements and design

- Remove the Origin allowlist and its rejection code. Reflect the request Origin and requested preflight methods/headers; actual API requests retain owner authentication.
- Instance settings accept `token` for backtest, paper and live connections. The UI uses password inputs. Private instance configuration stores the supplied value; research and scientific requests carry a credential reference. Existing environment references remain readable for older configurations.
- Settings contains separate Data and Data Plugins pages. Existing `#/data` and `#/extensions` links continue to work; research returns to the new Data location.
- Data plugins use a small `fields` / `load(config, context)` protocol, separate from Pi Durable research extensions. Explicit `kind: data` modules appear in the extension directory and the dedicated management page. See the [plugin contract](../../../apps/service/data-sources.md).
- The built-in service configuration source preserves existing datasets. A local-file plugin reads actual exported DatasetConfig objects, including their native catalog paths. External providers can reuse existing acquisition and conversion tools and return the same DatasetConfig contract.
- Save configuration separately from loading. Successful refresh replaces only that source's datasets; errors preserve its previous successful result. Disabling removes its datasets from new selections and retains disk data and captured research. Save configuration and successful results atomically in the existing data directory.

Research: the existing [operator source registry](../../../runtimes/data/source_plugins.py) already owns source-specific acquisition and conversion. [Hugging Face's official download API](https://huggingface.co/docs/huggingface_hub/en/guides/download) supports dataset repositories, version selection and filtered cached downloads. [Nautilus data](https://nautilustrader.io/docs/latest/concepts/data/) retains instrument-specific market types. Therefore this change reuses exported DatasetConfig metadata rather than guessing research requests and partitions from arbitrary Parquet directories. No new package or Python requirement is needed for application-side plugin loading.

<a id="plan"></a>
## Implementation plan

Parallel assignments: Origin/CORS; instance backend and credential transport; instance UI and data-plugin UI; settings navigation; data-plugin backend; dependency setup and user documentation. The primary session integrates the API, checks the complete behavior and handles PR delivery.

Reuse Fastify authentication hooks, existing instance settings and atomic JSON persistence, current DatasetConfig paths and science binding, existing extension discovery configuration, Ant Design settings forms, and current service/browser tests. Source modules never enter the Pi research registry. No online plugin marketplace or separate scheduler is introduced.

<a id="verification"></a>
## Verification

- Origin owner/API regression passed after removing the restriction.
- Four data-source and HTTP integration tests passed: local-file import and paths, external modules, replacement, enable/disable, optional-field clearing, invalid numeric output, failed refresh preservation, restart, authentication, unique extension names and retained research input.
- Three instance settings tests passed, including direct authenticated calls, same-ID URL/token rotation and recovery of both old and current credential references after restart.
- Web type checking, 26 unit tests and the production build passed. The 70-case browser run passed 67 cases; the three failures identified obsolete instance/data locators and a settings-section focus bug. All three passed after the respective corrections; desktop/mobile plugin views were inspected.
- Thirteen deployment configuration/maintenance tests passed. The full `make check-ts` suite is running. GitHub CI also runs the existing scientific/core coverage for this branch.

<a id="review"></a>
## Review

Parallel review found and resolved a data-source/instance-save binding race, duplicate extension names and optional number clearing. Final GitHub Codex read-only review and CI are maintained in the PR.

<a id="delivery"></a>
## Delivery

Branch: `codex/settings-data-extensions`, based on remote `dev` at `b4d25a48`. PR and merge are pending. The original dirty checkout is preserved. Installed services are outside this code-delivery task.
