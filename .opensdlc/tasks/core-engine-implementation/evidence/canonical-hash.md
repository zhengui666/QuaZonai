# Current TypeScript native request identity probe

This measurement uses the current TypeScript remote-native path on the working tree based on `78a84bdd31f524b4965560fc4b6ae4ab53cafcef`, rather than the retired Rust runtime/store. `canonical-hash-probe.mjs` compares the original recursive whole-string encoder against the actual exported `apps/nautilus/src/server.ts::executionId` function. The same incremental implementation now lives in `apps/research/src/native-json.ts::canonicalJsonDigest` and is called by both the research client and worker. It uses Node's official SHA-256 implementation and retains the original sorted-key JSON bytes.

## Raw measurements and limits

`canonical-hash-results.json` preserves all six original measurements: three independent Node processes per variant on Node `v24.19.0`, in the recorded alternating order. This was a shared cloud execution environment with concurrent work, including Rust compilation. These are synthetic request-hashing measurements, not remote HTTP, p95/p99, whole-engine, or trading throughput results. Three samples do not establish a stable performance ratio.

The fixture has 100,000 rows containing an exact nanosecond string and decimal string, plus a 1 MiB binary attachment encoded as base64. Both variants consume the same in-memory request. All six hashes equal `428285cc0e2fa27b1a9190e5c485f46deffaa1d99dd9901fddfb71d16c4ee7fe`.

| Median measurement | Original whole string | Incremental buffered hash |
| --- | ---: | ---: |
| Hash invocation wall time | 229.214808 ms | 157.190583 ms |
| Process lifetime max RSS | 175,988 KiB | 101,872 KiB |
| Post-call heap-used delta | 76,578,432 bytes | 827,384 bytes |

The heap delta is GC-sensitive and is not peak allocation or retained memory. Process max RSS includes fixture creation and module loading. The measurements support eliminating a whole-request temporary canonical string; they do not prove that all allocation is bounded to 64 KiB. Individual scalar strings, including base64 attachments, are still serialized as complete JSON tokens. Original input storage, transport serialization, parsing, and scientific computation are outside the measured invocation.

## Reproduce

From the repository root, after installing locked research dependencies:

```sh
npm ci --prefix apps/research --ignore-scripts --no-audit --no-fund
node --experimental-strip-types --expose-gc .opensdlc/tasks/core-engine-implementation/evidence/canonical-hash-probe.mjs original
node --experimental-strip-types --expose-gc .opensdlc/tasks/core-engine-implementation/evidence/canonical-hash-probe.mjs buffered
```

Each command emits one raw JSON record and starts a separate process. To reproduce the original order, run `original`, `buffered`, `buffered`, `original`, `original`, `buffered`. Collect a larger set on an otherwise idle host before using times for capacity estimates.

## Contract verification

The following commands passed after both sides adopted the shared helper:

```sh
node --experimental-strip-types --test apps/nautilus/test/*.test.ts apps/research/test/native-json.test.ts apps/research/test/remote-native.test.ts
npm --prefix apps/nautilus run typecheck
npm --prefix apps/research run typecheck
```

The test run passed 26 tests. A frozen digest and independent original encoder cover ordering, numeric object keys, exact decimal/nanosecond strings, Unicode and lone surrogates, and large token/attachment boundaries. The research HTTP test checks each submitted identity against the old canonical implementation and worker `executionId`, while preserving attachment bytes and per-invocation identities. Existing controlled-process tests cover duplicate/restart receipts, changed identity rejection, cancellation and unknown termination, output budgets, and tick admission. They are protocol validation, not production scientific performance evidence.

## Remaining measured-work boundary

The worker still reads and parses the progress NDJSON file for progress requests. Incremental progress indexing or caching has not been added: the effect needs a long-running job trace before adding retained state. Binary digest checking still reads current executable bytes because the identity boundary must detect changes. No executable stat-only identity cache, timing-based cancellation change, old Rust scheduler restoration, or wire-format replacement was introduced.
