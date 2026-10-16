# Decode SII raw historical fills

## Purpose and scope

The existing explicit HF acquisition/native bridge cannot decode SII's raw
`orderfilled.parquet`: its snake_case names and binary uint256 amounts differ
from Moose and TimeSeventeen. Add one operator-only `sii-order-filled` format
without changing Nautilus, the business model, scientific admission or orders.
The format reuses the existing selection/snapshot converter, BAR builder,
identity conflict rules and offline catalog preparation path.

## Evidence and interpretation

- [Dataset owner's current schema](https://huggingface.co/datasets/SII-WANGZJ/Polymarket_data):
  uint64 timestamp/block, uint32 log_index, 32-byte little-endian raw uint256
  amounts/fees, canonical hashes, explicit V1/V2 contract names
- [Pinned collector](https://github.com/SII-WANGZJ/Polymarket_data/blob/188eee28f09ba83d79c125bfb367f72ac93962c4/polymarket/fetchers/rpc.py):
  log_index preserves RPC logIndex; this older collector also has a timestamp
  estimation fallback, so historical event/receipt semantics remain unverified
- Native IDs cannot contain full transaction hashes (36-character limit).
  Keep chain/block/log IDs, reject independent transaction/log conflicts and
  preserve the original mapping in detached source evidence
- Raw per-fill fees, latest market state and export clocks do not establish
  original fee schedules, historical membership, settlement or PIT

No raw SII file or row group has been retrieved/decoded in this slice. Current
original-instrument prerequisites are absent. Synthetic typed Parquet tests are
protocol checks only.

## Acceptance and remaining work

Run the full Python source suite, installed HF dispatcher checks, native operator
unit tests and scoped Rust format/checks. Independent review must inspect the
final candidate. Keep final source and test evidence separate from publication.

Before claiming real import, acquire an authorized bounded original SII input,
check physical/logical schema, decode original rows and identity mapping, supply
source-backed native instruments/fees/clocks, prepare and read back the actual
catalog, register through the existing Runtime/CLI path, and run fresh validation.
No full 127GB download, remote row-group capability, coverage proof, PIT
qualification, Alpha, Cycle or profitability is claimed by this change.
