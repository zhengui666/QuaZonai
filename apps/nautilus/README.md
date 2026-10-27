# Remote Nautilus scientific adapter

This is the independently deployed HTTP adapter for the official Nautilus-based
scientific executable. The QuaZonai application does not launch or bundle that
executable. An ordinary Nautilus process without this API is not an HTTP endpoint
for QuaZonai. Paper and live target/account connections use their existing
downstream HTTP contract; this adapter exposes scientific computations only.

Install the repository's research dependencies, build the matching native `job`,
then start the adapter on the machine that owns the native engine:

```sh
export QZ_JOB_BINARY=/opt/quazonai/bin/job
export QZ_TICK_JOB_BINARY=/opt/quazonai/bin/job-high-precision
export QZ_NAUTILUS_DATA_DIR=/data/nautilus
export QZ_NAUTILUS_CATALOG_ROOTS='["/data/catalogs","/data/quazonai/catalogs"]'
export QZ_NAUTILUS_HOST=127.0.0.1
export QZ_NAUTILUS_PORT=8090
# Set QZ_NAUTILUS_TOKEN privately when bearer authentication is used.
node --experimental-strip-types apps/nautilus/src/main.ts
```

The data directory contains private execution receipts and temporary work. Keep
it on persistent storage and use one worker process for that directory. Catalog
roots are existing directories, configured by the operator; catalogs and
partition manifests must be readable at the same absolute paths used by the
application's existing input and revision checks. In container deployments mount
these data directories read-only into the worker. This includes the application's
frozen catalog copies when that workflow creates them. Model, scope and native
object artifacts are uploaded with the request; catalog history is not silently
copied over HTTP. Protect non-loopback HTTP with the deployment's private network
or TLS reverse proxy, and configure the matching token environment reference in
the application. The worker never forwards its bearer credential or provider
environment to the scientific child process.

`QZ_TICK_JOB_BINARY` explicitly selects an optional high-precision companion for
`evaluate-tick-factor`. All BAR operations keep `QZ_JOB_BINARY`. Configure the
companion for the official 16-decimal TradeTick catalogs produced by the
Hyperliquid importer; a standard nine-decimal job cannot decode that native
format. Omitting the companion retains single-binary operation for existing
standard-format inputs. Catalog paths never choose or infer an executable.

With a companion, the advertised implementation identity binds both executable
byte identities and their standard/tick roles. Both are checked again before
executing new work; replacing either file requires restarting the worker and
obtaining its new capabilities. Original receipts retain their original
implementation identity. Without a companion, the existing single-binary digest
is unchanged.

`deploy/docker/native-build.sh` preserves `job`, `source-tools` and the standard
acceptance helpers before building `job-high-precision`,
`catalog-prepare-high-precision` and `market-history` with the official
high-precision feature. The remote native image carries the two job formats and
matching offline preparation/import operators side by side; the application
image carries no native executable. Use the matching preparation format and do
not overwrite or relabel old catalogs to change their binary precision.

`evaluate-tick-factor` uses the same native request, receipt, attachment and
cancellation contract. Its first installation profile admits one tick execution
at a time; a new invocation over capacity receives HTTP 429 before an execution
receipt is created. Existing original receipts and active-request observers stay
readable, and BAR computations keep their existing concurrency behavior. Set
`QZ_NAUTILUS_TICK_CONCURRENCY` to configure that owner limit.

The refusal contains the original `requestId`, `schemaVersion:1`,
`admission:"REJECTED"` and a specific capacity/resource-enforcement error. The
application retains it as non-execution evidence in the existing remote-reference
facts; it does not invent a terminal scientific receipt. A generic HTTP error or
a refusal with another request identity cannot settle an uncertain execution.

Tick execution requires Linux `/usr/bin/prlimit`. The worker applies both soft
and hard address-space limits to the configured native executable, defaulting to
2 GiB through `QZ_NAUTILUS_TICK_MEMORY_BYTES`. This is a virtual-address-space
ceiling, not a measurement of resident memory; a native build using larger
mappings or thread reservations may require an explicitly larger installation
limit. Unsupported resource enforcement fails explicitly. Tick standard output
is limited to 64 MiB by `QZ_NAUTILUS_TICK_OUTPUT_BYTES`; exceeding it stops and
joins the child and retains a failed receipt. These owner settings cannot be
raised by fields in the scientific request. They do not qualify the source or
replace native row, cohort, output-file or scientific budget checks.

The version 1 contract is defined in
[`packages/contracts/src/nautilus.ts`](../../packages/contracts/src/nautilus.ts):

- `GET /health` reports process readiness without credentials.
- `GET /v1/capabilities` reports the native executable's SHA-256 identity and
  supported scientific commands.
- `POST /v1/execute` accepts a versioned request, implementation identity,
  invocation identity, original JSON input, arguments and Base64 attachments.
  `@file:name` references a model or scope attachment; `@objects:prefix` references
  a directory of uploaded native object files. Catalog and partition arguments
  must resolve inside the configured roots.
- `GET /v1/executions/:requestId` reads the original receipt. Repeating the exact
  same POST reads that receipt or waits for the same active execution.
- `DELETE /v1/executions/:requestId` requests cancellation and returns only after
  the actual scientific process has exited. A completed result stays completed.

Every computation invocation has its own UUID. Its request ID is the SHA-256 of
the canonical request without `requestId`, including that invocation UUID. A
single uncertain HTTP operation is reconciled with that original identity;
independent computations of the same inputs do not reuse historical results or
cancel each other. A disconnected final POST observer stops the native child,
and graceful worker shutdown stops and joins active children. An abrupt worker
loss leaves an `UNKNOWN` receipt: it does not certify cancellation or grant
permission to execute that identity again. Existing completed and failed receipts
survive restart. These receipts are transport evidence, not a second research
scheduler or proof of scientific validity.

Run the HTTP/process recovery tests with `npm --prefix apps/nautilus test`.
These controlled subprocess tests establish adapter behavior; the separate
native integration suite checks actual Nautilus computations.
