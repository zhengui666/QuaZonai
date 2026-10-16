# Modular native response validators

## Purpose and boundary

Partition one official AJV standalone emission into stable schema-owned modules,
retain the eager compatibility entrypoint and native function identities, and
load only the response validator needed at existing asynchronous HTTP boundaries.
This is an independent bounded experiment, based on dev `4e4a9fb`, not a contract
or evaluation-schema change. The previously reviewed lazy section loading and
data-dialog recovery remain intact.

No upstream implementation is copied or patched. There are no private AJV cache
reads, handwritten validators, runtime source evaluation, schema weakening,
credentials, permissions, service writes or HTTP write replay. Existing Rust JSON
and TypeScript API snapshots are unchanged. There is no JavaScript consumer in
the native CLI.

## Compiler and ownership

- [Native compilation](../../../apps/web/scripts/validator-native.mjs) preserves
  schema registration, options, exact-reference decisions and aliases. AJV's
  public `code.process` observes original schema object identity and returns the
  exact code unchanged
- [Compiler adapter](../../../apps/web/scripts/validator-partition.mjs) uses pinned
  TypeScript symbols, canonical schema pointers and dependency SCCs. It accepts
  named functions, recognized pure single-const initializers, static native
  exports and pure function `.evaluated` attachments. Unknown shapes, missing
  provenance, unreachable functions, collisions and lexical name capture fail
  before publishing anything
- Same-schema function instances are never merged. Wrapper bindings, shared
  constants, metadata and initialization order are retained; only unreachable
  recognized-pure constants can disappear
- [Generator](../../../apps/web/scripts/generate-validators.mjs) compacts with
  pinned Vite/esbuild and creates one canonical CJS graph, selective scalar/error
  facades, metadata and literal-target lazy dispatch
- [Ownership](../../../apps/web/scripts/validator-output.mjs) is limited to
  `src/generated/response-contract/` and the two existing response facade files.
  The deterministic manifest records the complete payload paths, import IDs,
  SHA-256, byte lengths and schema/binding provenance; its own path is explicit
  and its complete bytes are independently checked
- `--check` independently recompiles the complete expected output set. Missing,
  extra, stale, symlinked and untracked expected files are rejected by CI's
  `--require-tracked`. Unknown files are never deleted. Stale deletion requires
  previous ownership, safe path, generated marker and matching recorded bytes
  and hash. The unrelated `api.d.ts` is never cleared
- Output analysis finishes before writes; unchanged bytes are not rewritten.
  The manifest is published last. Multi-file publication is not atomic, and an
  interrupted publish must fail the next independent check

## Consumers and recovery

Production consumers use `response-contract/decimal`, `/problem`, `/base-currency`,
`/cost-amount`, `/cost-currency`, `/catalog-key`, `/metadata` and `/lazy`.
The original `response-contract` remains an eager compatibility facade for
existing consumers/tests; app startup does not import it.

The generated lazy entry exports `validateResponseAsync` and
`validateResponseResultAsync`. The latter snapshots the complete native errors
synchronously in the same continuation as validation, after module loading.
Concurrent calls share a module-loading promise but cannot overwrite each
other's returned errors. Failed promises are removed; a later explicit load may
retry, but browser syntax/module-evaluation failures can remain cached until
refresh. No URL cache-busting is used.

`api.ts` preserves content/media decisions, response-body ownership, authentication
notification for 401, cancellation, explicit retries and original intent keys.
A validator load failure becomes local status-0 `RESPONSE_VALIDATOR_UNAVAILABLE`,
preserving generic uncertain-outcome recovery even after a raw HTTP 201. A response may
already have been committed: the UI tells the user to refresh/query the original
result instead of changing and blindly resubmitting the request. No network
operation is automatically repeated.

### Evaluation integration recipe

After rebasing the independently staged evaluation schema, the report upload's
`beforeUpload` handler is already asynchronous. Import `validateResponseAsync`
and `ResponseValidatorLoadError` from `@quazonai/web/response-contract/lazy` and
await the existing exact `reportPath`, GET, 200, application/json validation.
Keep the existing file-size limits and sequence guards around file/error/state
updates. Translate a chunk-load failure to a distinct validation-unavailable
message, not `REPORT_JSON`; do not submit the report without validation. This
requires no new helper root or eager aggregate import. Regenerate and rerun
final-head checks after that schema rebase; this batch does not validate it.

## Checks and measured tradeoffs

- `npm --prefix apps/web run test:validator-adapter`: full generator repetition,
  localized mutation, original native results/full errors, retained function and
  alias identities, CJS/default/named ESM, recursive/wrapper/shared/shadowed cases,
  lexical collision guards, fail-closed syntax, ownership and loader concurrency
- `npm --prefix apps/web run check:generated`: independently verified complete
  output set and repository tracking, before write-mode generation
- `make check-web`: preserved generated drift, full web types/tests and production
  build checks, plus native adapter tests and complete PWA/static closure check
- [Browser matrix](../../../apps/web/tests/validator-loading.spec.ts) covers normal
  Vite CJS prebundling, production delayed/failed validators, late navigation,
  retained edits/idempotency, failed PWA install and complete delayed precache.
  It runs in hosted Web CI after Chromium installation; local Chromium/socket
  execution is platform-blocked and was not retried
- Existing native Rust schema/browser checks, unsaved PWA updates, NetworkOnly
  API/health routes and the 2 MiB per-asset precache limit stay in place

Initial final-production adapter evidence: 981 native exports / 989 total facade
exports, 308 retained native function instances, 316 core modules, 336 owned
files, 45,126 result/full-error comparisons and 35,298 route/media comparisons.
Two complete generations are identical. Inserting an earlier unrelated response
adds one module and leaves every existing module byte-identical; editing Decimal's
constraint changes one module without renaming it.

Full-app uncompressed startup closure measured from Vite's actual entry manifest:
3,221,932 bytes at the original base versus approximately 1.66 MB with modular loading.
Total static distribution is larger, approximately 4.05 MB versus 4.40 MB. All
emitted lazy JS/CSS assets are in the generated precache and below 2 MiB. These
are build-graph measurements, not browser startup or download latency timings.
A local full generation took approximately 4.4 seconds versus 0.87 seconds for
the original aggregate generator. The benefits are selective loading and stable
localized diffs, not lower total bytes or faster full generation.

## Delivery gate

Keep publication blocked until an independent native reviewer has checked the
exact final source/generated manifest, actionable findings are resolved, and all
applicable final-head hosted CI (including both browser modes and native PWA)
has passed. Unit/static checks do not establish unperformed browser, service,
authenticated account, or evaluation acceptance. No push or merge was performed
as part of implementation.
