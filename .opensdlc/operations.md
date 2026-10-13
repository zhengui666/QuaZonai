# Engineering operations

<a id="controls"></a>
## Action boundaries

Develop on a branch from current remote `dev`, in an isolated worktree. Preserve unrelated source changes, installations, credentials, datasets, migrations, journals and backups. Tests use task-owned directories and disposable services. A code merge does not authorize production deployment, paid inference, broker orders or restoring a user's database.

<a id="delivery"></a>
## Delivery

The task and [review policy](review.md) define acceptance. Check the exact PR Head, resolve concrete findings, rerun affected checks and verify the actual remote `dev` merge. GitHub PR/Actions records own hosted outcomes. Do not call an unrun, skipped, cancelled or old-Head check a pass.

<a id="container-release"></a>
## Images and publication

Publication and installation support **Linux x86_64 only**, including the standalone CLI. The application packages official Node, the TypeScript service/CLI, Web assets, production npm packages and the matching native scientific entrypoints. Pi Durable runs under one application process; the retired Rust server/Worker/runtime/CLI are not deployment dependencies. Keep upstream package/toolchain/license files and the published version's exact source revision.

Build native scientific binaries once with a consistent feature selection and reuse them for application and scientific images. TypeScript/Web changes should reuse that scientific build rather than recompile it. Use official npm/Cargo/Buildx caches; do not add handwritten checksum or cache-validation machinery. Publish the same artifacts that passed installed acceptance, not a different rebuild.

<a id="dev-release"></a>
### Development releases

A `dev` release consumes successful exact-source core CI, builds each artifact once, exercises the compact installed start/restart/backup path and publishes those assets. It must not repeat the deleted Rust-service regression before another cold compilation. Retain formal main/tag selection and immutable completed-version semantics; a failed draft/tag operation must be reconciled before retrying.

<a id="dev-image"></a>
### Development images

Use the repository's actual [workflows](../.github/workflows) for isolated image verification/publication. A built or published image does not switch a user's installation. Do not infer a successful install from an image tag or `--version` output.

<a id="research-host"></a>
## Service and durable host

The TypeScript [service](../apps/service) opens the original database through [openResearch](../apps/research/src/host.ts); do not also start the standalone research host against that database. Native SQLite owns conversations, submissions, task graph, replay and usage. Current application facts are native documents, not another task queue or copied Agent state.

`QZ_CONFIG` selects the private service configuration. Keep original `research.database`, registered extension/provider modules, dataset inputs and downstream bindings. Resolve relative paths from that file, not from a later working directory. Provider credentials stay in explicitly configured host-side storage/environment; native science does not inherit them. Missing model/data/connection is an honest unavailable state, never demo success.

Restart reopens the same database, repairs original admitted-but-unsubmitted requests and resumes original native work. Official retry, stream timeout, compaction and progress settings are public Harness configuration; native task inspection remains the execution authority. Retryable model failures and exhausted authentication/quota are different outcomes. Unsupported models never silently fall back.

Graceful shutdown closes admissions and observers, then closes the Harness so interrupted work retains its native recovery semantics. Container restart supervision may reopen the process; it cannot prove an external job or account action was cancelled. Short deterministic recovery tests do not establish a literal 24-hour observation or real-model research quality.

<a id="scientific-runtime"></a>
## Scientific execution and data

Scientific processes use the matching [native job](../apps/job) and original typed inputs. Reuse official Task/NodeExecutionEnv lifecycle, explicit bounded model execution and native Nautilus/Clarabel/Arrow behavior. Preserve request/model/provenance association and the original deadline across recovery. Computation is not qualification, equity comes from the native shared account and missing values remain null.

Source acquisition and catalog preparation remain at [data tools](../runtimes/data/README.md) and [source plugins](../runtimes/data/source-plugins.md). Catalog paths, partitions, fees, availability and rights are actual owner configuration; preparation does not establish PIT, source permission or profitability.

<a id="runtime-targets"></a>
### Configuration changes

Change settings through the current API/CLI or the installed version's supported configuration command. Extension changes must expose their actual loaded/disabled/pending-restart state. Keep the task definitions and original input needed by pending work; toggling a plugin must not silently rewrite an admitted study or erase its results. A configuration file write is not a successful reload or connection probe.

<a id="recovery"></a>
## Backup, upgrade and restore

Use the [installed bundle's instructions](../deploy/docker/README.md), not a removed `server worker`, `runtime doctor` or an old executable against a new schema. Before maintenance, close new admission, inspect the actual task graph and reconcile existing science/downstream work. Drain requests already admitted before deciding the application is idle. Maintenance does not create cancellation receipts.

Stop every writer before archiving the complete persistent directory: durable SQLite and sidecars, auth/config/provider state, user extensions, catalogs and scientific objects. Preserve installation root/owner/ports, historical PostgreSQL, `master.key`, original encrypted secrets, independent journals, pending records and backups. A live directory copy is not a consistent backup. Restore the original paths and ownership and run only one owner.

Upgrade must adopt original durable state rather than create an empty replacement database. Existing applied SQL and Runtime SQLite migrations are retained unchanged under [migrations](../migrations). Historical Runs/Attempts and external identities remain their original facts. If an installed legacy control plane cannot be safely continued, reject the cutover with its concrete reason; do not clear the queue, infer cancellation or manufacture successful migrated Tasks. New installation checks do not prove an unperformed legacy upgrade.

<a id="access-cutover"></a>
### Access recovery

Preserve original password/device verifiers and revocation facts. Recovery of access, data restoration, remote cancellation and model-provider login are separate actions. Enter secrets only in the user-controlled private terminal or browser; never place passwords, tokens or provider values in a task, URL, argv, source or logs. Do not guess new identities to make an old denied client work.

<a id="runtime-recovery"></a>
### Historical Runtime state

Keep original journals/catalogs/objects and the four applied migration files. Do not start the retained legacy gateway or its Worker during normal Node startup, relabel its unresolved jobs as durable or mix files from different recovery points. An unresolved remote identity needs actual reconciliation before cutover; having retained its SQLite file is not completed recovery.

<a id="observe"></a>
## Observe and diagnose

| Observation | Action |
| --- | --- |
| 202, timeout or lost reply | Read original native submission/task/receipt; replay only the original request and identity |
| Unsafe interrupted or unknown downstream result | Reconcile original external identity; do not create a replacement effect |
| Scientific failure or missing metric | Keep the original failed trial and reason; no zero fill or fallback source |
| Stopped observer | Inspect native work separately; observation ending does not cancel research |
| Missing model/dataset/connection or blocked plugin | Repair the concrete configuration; no fake readiness or task-state translation |
| Failed build/check | Inspect that Head's actual job, fix the owning path and rerun affected checks |

The code refactor does not install an unattended production monitor or perform a real account run. Target-only paper/live observation records the configured downstream/account source; QZ does not send financial orders.

<a id="metrics"></a>
## Measurements

Record source revision, actual elapsed time, workload/cache state and outcome. The pre-refactor `66353559` hosted logs show routine Rust CI 35m11s, PG 25m35s and dev release 3h41m53s; three native image compiles account for 87m54s. These are a measured baseline, not the new implementation's performance. Keep new CI/build measurements in the task or Actions rather than creating another metrics ledger.
