# Offline Binance Vision spot archive validation

## Scope

Bounded first batch of the owner's autonomous `dev` iteration, based on
`bfa3cfc625a752fdb554d9136b9b2eb4412c6f24`. Publication and independent review are
coordinator-owned and pending. No issue or PR has been published for this batch.
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
