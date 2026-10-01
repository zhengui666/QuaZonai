# Offline Binance Vision spot archive validation

## Scope

Bounded first batch of the owner's autonomous `dev` iteration, based on
`bfa3cfc625a752fdb554d9136b9b2eb4412c6f24`. The first batch was delivered in
[PR 153](https://github.com/zhengui666/QuaZonai/pull/153), merged as `1b01b2eb`.
The maintained [archive guide](../../../runtimes/data/binance-vision.md) defines
the API, envelope, bounds, actual verification scope and remaining work.

Add reusable offline one-symbol/day/interval selection, stdlib ZIP/checksum/CSV
validation, local freeze/reproduction and synthetic fixtures. Keep
`qz.public_archive_acquisition/1` separate from frozen Coinbase HTTP acquisition.
Fixed provider/venue identity and parser v1 remain independent of future runtime
capabilities. Unknown, operator-declared and synthetic provenance are explicit;
no local import clock becomes an original retrieval time. Missing candles stay
missing and exact values/source clocks remain in original bytes and lineage.

No native CLI/preparation modules, existing providers/registry, installed payload,
workflows, general contracts or old provider guides change. This avoids conflicts
with the single-executable operator consolidation and other source-doc work.
There is no download/convert/prepare capability or complete-plugin claim.

## Validation boundaries

- No data endpoint access, listing, HEAD/range or checksum request; no accounts,
  credentials, payments, agreement acceptance or terms-acceptance flags
- No handcrafted compression/container parser, copied upstream implementation
  or private ZIP API. Canonical raw-envelope validation is not claimed where
  stdlib public APIs do not expose it; unused ZIP64/trailing variants are covered
  as explicit limitations
- Native definitions, fees, precise BAR conversion, native independent
  verification/readback, installed tests and permission/PIT/scientific admission
  remain separate unfinished stages
- Synthetic tests must cover safe bounds, sparse/full/empty data, exact decimals,
  ms/us transition, unsupported identities, clock forgery, malicious inputs,
  mutation before publication and unchanged frozen HTTP source behavior

## Checks and review

Run the guide's focused and complete Python discovery suites without network.
Run diff whitespace and applicable Markdown link checks. This executor has
limited disk; do not run heavyweight native builds. `make check-docs` also runs
Rust client/schema tests; record that portion as unrun, not passed. No Rust
behavior changes are claimed.

Before publication, a separate native reviewer must inspect the exact staged
tree, this task and its synthetic evidence. Final-Head hosted CI, resolving
actionable review findings, and coordinator authorization remain required before
merge to `dev`, under the [review rules](../../review.md). A handoff artifact or
local Python pass does not satisfy those gates. No real data qualification is
part of this batch.

## Decoded-member truncation correction

A synthetic independent native-parser comparison found that Python's
`ZipExtFile` clips emitted bytes to the central directory's declared decoded
size. A two-row stored or deflate member with a forged one-row size and matching
prefix CRC could pass inspection, freeze and verification while omitting the
second valid row. A zero-size/CRC declaration could hide both rows. Retained
original ZIP bytes were intact, but the decoded-member integrity claim was wrong.

The correction keeps the original member metadata unchanged and uses the public
`copy.copy` API plus a copied `ZipInfo.file_size` set to the independent decoded
byte budget. The canonical-name probe and associated overlap checks remain;
actual bounded output must match the original size and CRC before parsing.
There is no new dependency, private ZIP field access or custom container parser.
The frozen v1 envelope and valid-source output stay compatible; old malformed
bundles are rejected on re-verification without modifying their retained bytes.

Regressions cover both compression methods, hidden first-row/empty prefixes,
freeze refusal before output publication, rejection of self-consistent legacy
bundles, oversized decoded data and retained overlap rejection. Existing tests
still cover valid empty/full/sparse data, exact values, original clocks and
accepted ZIP64/trailing variants. Exact compressed-stream termination and
canonical container validation remain explicitly outside this stdlib profile.
Independent review and final-head hosted checks are still required for this fix.
