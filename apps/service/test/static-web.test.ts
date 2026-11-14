import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { brotliCompressSync, gzipSync, brotliDecompressSync, gunzipSync } from 'node:zlib';
import Fastify from 'fastify';
import { registerStaticWeb } from '../src/static-web.ts';

test('static assets negotiate precompression, cache only hashed files and reject missing chunks', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-static-')); const app = Fastify();
  try {
    await mkdir(join(root, 'assets'));
    await writeFile(join(root, 'index.html'), '<html>current</html>');
    await writeFile(join(root, 'config.json'), '{}');
    const code = 'export const value = "payload";\n'.repeat(100);
    const asset = '/assets/vendor-AbCd1234.js';
    await writeFile(join(root, asset), code);
    await writeFile(join(root, `${asset}.br`), brotliCompressSync(code));
    await writeFile(join(root, `${asset}.gz`), gzipSync(code));
    await registerStaticWeb(app, root);
    for (const encoding of ['br', 'gzip', 'identity']) {
      const result = await app.inject({ url: asset, headers: { 'accept-encoding': encoding } });
      assert.equal(result.statusCode, 200);
      assert.equal(result.headers['cache-control'], 'public, max-age=31536000, immutable');
      assert.match(String(result.headers.vary), /accept-encoding/i);
      assert.equal(result.headers['content-encoding'], encoding === 'identity' ? undefined : encoding);
      assert.equal((encoding === 'br' ? brotliDecompressSync(result.rawPayload) : encoding === 'gzip' ? gunzipSync(result.rawPayload) : result.rawPayload).toString(), code);
    }
    const first = await app.inject({ url: asset });
    const cached = await app.inject({ url: asset, headers: { 'if-none-match': String(first.headers.etag) } });
    assert.equal(cached.statusCode, 304);
    for (const url of ['/', '/research/123', '/config.json']) {
      const result = await app.inject({ url }); assert.equal(result.statusCode, 200); assert.equal(result.headers['cache-control'], 'no-cache');
    }
    for (const url of ['/assets/expired-AbCd1234.js', '/assets/expired.css?x=1', '/api/v3/missing', '/missing.svg']) {
      const result = await app.inject({ url }); assert.equal(result.statusCode, 404); assert.doesNotMatch(String(result.headers['content-type']), /html/);
    }
    assert.equal((await app.inject({ method: 'POST', url: '/missing' })).statusCode, 404);
  } finally { await app.close(); await rm(root, { recursive: true, force: true }); }
});
