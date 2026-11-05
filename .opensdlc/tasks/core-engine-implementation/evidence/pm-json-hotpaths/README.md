# PM native evidence JSON CPU hotspots

## Observed running test

An existing `make check-core` run spent substantial CPU time in the controlled `apps/service/test/pm-forward-http.test.ts` flow. The requested RESEARCH_ONLY process (PID 73306) finished naturally before sampling and passed in 295.25 seconds. The follow-on PAPER process (PID 74177, Node v22.19.0) was sampled through a loopback Node inspector. No test was terminated and no application payload, local variable, heap, environment variable, or credential was captured.

`original-paper.cpuprofile` retains the raw call stacks and samples. `original-paper-summary.json` includes self and inclusive time, with repeated recursive frames deduplicated per sample. `profile-method.json` records the profiler commands, interval, UTC file timestamp and approximate capture start; the raw profile retains its original monotonic timestamps. This one shared-host window is not an attribution of the entire test's duration.

In the 16.232-second window, 13,359 samples showed:

| Call path | Sampled time | Share of window |
| --- | ---: | ---: |
| `native-evidence.ts` → `jsonChunks` / recursive `fragments` | 4.759 s inclusive | 29.32% |
| `computePmDecision` → `evidence` → `pmArtifactDigest` | 3.790 s inclusive | 23.35% |
| Garbage collector | 3.221 s self | 19.85% |

Percentages belong to this window, and overlapping inclusive rows must not be summed. The profile refers to function line numbers before this optimization. `source-comparison.json` proves that these original encoders and the evidence storage path were unchanged from `78a84bdd31f524b4965560fc4b6ae4ab53cafcef`. No new remote `canonicalJsonDigest` frame occurred; this controlled test uses the local binary path, which bypasses remote request hashing. The evidence therefore identified existing JSON storage and digest costs, not an observed quadratic regression in the recently added remote hash.

## Implementation and preserved contracts

`pmArtifactDigest` retains its exact original traversal, sorting, scalar conversion and missing-value handling. It now shares `createBufferedJsonHash` with native request hashing. Both use Node's official SHA-256 implementation, buffering complete tokens before native updates. The target is 64 Ki UTF-16 code units, not 64 KiB of UTF-8 bytes. A larger scalar is sent whole, and Unicode surrogate pairs are never split between native updates.

`jsonChunks` now has one generator that yields complete chunks. Its explicit stack has one frame per active array/object instead of a recursively delegated generator for every JSON token. It retains the original token order, chunk boundaries and one-token lookahead. Storage is proportional to active nesting and the keys of active objects, plus the current chunk and any large scalar; the encoder does not build a full JSON result or token list. A repeated active ancestor can be finite when getters or consumer mutations change it. That uncommon subtree uses the original recursive token walker instead of being rejected solely for sharing an ancestor. Actual persistent cycles still fail with the original recursion RangeError. Ordinary JSON remains on the explicit stack; no arbitrary depth threshold or fabricated value is introduced. BigInt and getter errors still propagate, including failures after previously emitted chunks.

`retainNativeEvidence` consumes the same `jsonChunks` export, so its gzip format, part ordering, task association, original evidence bytes and final descriptor publication are unchanged. No separate storage, financial qualification rule, identity algorithm, or reduced scientific output was introduced.

## Verification

The following related Node 22 test command passed all 38 tests after the review correction, including finite dynamic ancestor getters and the original `< length` semantics for an invalid array proxy. All three TypeScript checks passed. `tests-final.log` preserves the test output, and `verification.json` binds the checks to source-file hashes.

```sh
node --experimental-strip-types --test apps/research/test/native-json.test.ts apps/research/test/native-evidence.test.ts apps/service/test/pm-evidence.test.ts apps/research/test/remote-native.test.ts apps/nautilus/test/server.test.ts
npm --prefix apps/research run typecheck
npm --prefix apps/service run typecheck
npm --prefix apps/nautilus run typecheck
```

Tests compare large nested evidence against independent `JSON.stringify` bytes and the original encoder's exact chunks. They cover numeric/string key order, UTF-16 boundaries, lone surrogates, NaN/Infinity, sparse arrays, undefined, legacy non-JSON values, repeated references, cycles, getter observation order, early iterator cancellation and late serialization errors. Ancestor tests compare each pull and getter count when finite self getters revisit an object, a paused consumer disables a getter or shrinks an ancestor array, and an ancestor getter fails late or the consumer closes early. The PM digest is compared with the original token-at-a-time SHA-256 and the independent business digest. Gzip is independently decompressed and compared to `JSON.stringify`, and official task restart, namespace identity, digest mismatch, failed part save, and a late compression-stream failure remain covered.

The final application's complete native HTTP flow and CI are separate acceptance checks. A passing microbenchmark or storage test does not replace them.

## Paired function measurements

`probe.mjs` contains the original functions and imports the actual optimized application exports. The fixture is explicitly synthetic storage data: 20,000 nested native-shaped rows with exact decimal/nanosecond strings and Unicode. The chunk workload uses the evidence path's 1 Mi UTF-16-unit target and consumes the full result into SHA-256; it does not time gzip, database persistence, HTTP, or science. The digest workload measures the canonical artifact hash.

`paired-results.json` preserves all 20 process results: five alternating original/optimized pairs for each workload. Before the 17-second collection, the core run had completed and `ps` showed no active Cargo or other test processes; the scientific benchmark waited until this window ended. The cloud host is still shared. Each result comes from an independent cold Node v22.19.0 process, with fixture construction and an explicit GC outside the measured invocation.

| Median | Original | Optimized |
| --- | ---: | ---: |
| Chunk serialization and consumption | 486.46 ms | 165.13 ms |
| PM artifact SHA-256 | 198.26 ms | 131.47 ms |
| Chunk process max RSS | 221,792 KiB | 191,060 KiB |
| Digest process max RSS | 174,808 KiB | 177,568 KiB |

Every pair returned the same full-output digest. Chunk paired wall ratios ranged from 2.57 to 2.99; digest ratios ranged from 1.00 to 1.79. These are function-level measurements for this fixture, not a whole-system speedup. Digest max RSS slightly increased in this sample; no universal memory reduction is claimed. Post-call heap deltas in the raw data are GC-sensitive and are not peak allocation. Process max RSS includes module loading and the fixture.

These measurements preceded the review correction for dynamic ancestor getters. That rare route now delegates to the original walker; ordinary benchmark inputs still take the explicit-stack route. The full benchmark was not repeated for the correction, and the recorded numbers remain measurements of the original collected revision rather than a new final-head timing claim.

Reproduce with the locked project dependencies installed, using Node 22.19.0 on a coordinated idle host:

```sh
node --experimental-strip-types --expose-gc .opensdlc/tasks/core-engine-implementation/evidence/pm-json-hotpaths/probe.mjs chunks original 20000
node --experimental-strip-types --expose-gc .opensdlc/tasks/core-engine-implementation/evidence/pm-json-hotpaths/probe.mjs chunks optimized 20000
node .opensdlc/tasks/core-engine-implementation/evidence/pm-json-hotpaths/run-pairs.mjs 20000 5 > /tmp/qz-json-hotpath-rerun.json
```

The runner also executes both digest variants and refuses unequal result hashes. Preserve rerun output separately from the original measurements.
