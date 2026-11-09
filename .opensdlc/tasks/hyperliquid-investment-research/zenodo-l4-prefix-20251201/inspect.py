from pathlib import Path
import json
import zlib

raw = Path(__file__).with_name('archive-range-0-4095.bin').read_bytes()
assert len(raw) == 4096
assert raw[512:528] == b'20251201/ex22.gz'
decoder = zlib.decompressobj(31)
decoded = decoder.decompress(raw[1024:])
assert not decoder.eof
lines = [line for line in decoded.splitlines(keepends=True) if line.endswith(b'\n')]
records = [json.loads(line) for line in lines]
print(f'archive_bytes={len(raw)} decoded_bytes={len(decoded)} complete_records={len(records)} gzip_complete={decoder.eof}')
print(f'coins={sorted({row["coin"] for row in records})}; fields={sorted(records[0])}')
