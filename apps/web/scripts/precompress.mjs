import { readdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { brotliCompressSync, gzipSync } from 'node:zlib';

// Build-time only: avoid compressing large bundles on each HTTP request.
const assets = resolve('dist/assets');
for (const name of await readdir(assets)) {
  if (!/\.(js|css)$/.test(name)) continue;
  const file = resolve(assets, name);
  const original = await readFile(file);
  for (const [extension, compress] of [['br', brotliCompressSync], ['gz', gzipSync]]) {
    const compressed = compress(original);
    if (compressed.length < original.length) await writeFile(`${file}.${extension}`, compressed);
  }
}
