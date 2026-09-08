// Native filesystem regression for CodeQL #56; no synthetic stat/read result.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { readStaticAsset } from '../tests/pwa-server.mjs';

async function fixture(t) {
  const directory = await fs.mkdtemp(join(tmpdir(), 'quazonai-pwa-files-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const root = join(directory, 'dist');
  await fs.mkdir(root);
  await fs.writeFile(join(root, 'index.html'), '<main>static shell</main>');
  await fs.writeFile(join(root, 'asset.js'), 'original-bytes');
  return { directory, root };
}

test('serves original build bytes and rejects missing, nonfile and escaping paths', async (t) => {
  const { directory, root } = await fixture(t);
  const home = await readStaticAsset(root, '/');
  assert.equal(home.extension, '.html');
  assert.equal(home.bytes.toString(), '<main>static shell</main>');
  assert.equal((await readStaticAsset(root, '/asset.js')).bytes.toString(), 'original-bytes');
  await fs.writeFile(join(directory, 'outside.js'), 'outside build');
  await fs.mkdir(join(root, 'directory'));
  for (const pathname of ['/missing.js', '/directory', '/../outside.js', '/%2e%2e%2foutside.js', '/%00', '/%zz', 'relative.js']) {
    await assert.rejects(readStaticAsset(root, pathname), `must reject ${pathname}`);
  }
});

test('rejects a final symlink instead of reading its target', async (t) => {
  const { root } = await fixture(t);
  await fs.symlink(join(root, 'asset.js'), join(root, 'alias.js'));
  await assert.rejects(readStaticAsset(root, '/alias.js'), { code: 'ELOOP' });
});

test('a pathname replacement after fstat cannot substitute the opened bytes', async (t) => {
  const { root } = await fixture(t);
  const target = join(root, 'asset.js');
  const open = fs.open.bind(fs);
  let opened;
  let swapped = false;
  // Synchronize a real rename between the real fstat and the real read. No fake
  // contents, metadata or successful filesystem operations are returned.
  t.mock.method(fs, 'open', async (...args) => {
    const file = await open(...args);
    opened = file;
    const stat = file.stat.bind(file);
    t.mock.method(file, 'stat', async (...statArgs) => {
      const metadata = await stat(...statArgs);
      await fs.rename(target, join(root, 'previous.js'));
      await fs.writeFile(target, 'replacement-bytes');
      swapped = true;
      return metadata;
    });
    return file;
  });
  const asset = await readStaticAsset(root, '/asset.js');
  assert.equal(swapped, true);
  assert.equal(asset.bytes.toString(), 'original-bytes');
  assert.equal(await fs.readFile(target, 'utf8'), 'replacement-bytes');
  assert.equal(opened.fd, -1, 'descriptor must close after successful read');
});

test('closes the native descriptor when validation or reading fails', async (t) => {
  const { root } = await fixture(t);
  await fs.mkdir(join(root, 'directory'));
  const open = fs.open.bind(fs);
  const opened = [];
  let failRead = false;
  t.mock.method(fs, 'open', async (...args) => {
    const file = await open(...args);
    opened.push(file);
    if (failRead) t.mock.method(file, 'readFile', async () => {
      throw Object.assign(new Error('injected read failure'), { code: 'EIO' });
    });
    return file;
  });
  await assert.rejects(readStaticAsset(root, '/directory'), /regular file/);
  failRead = true;
  await assert.rejects(readStaticAsset(root, '/asset.js'), { code: 'EIO' });
  assert.equal(opened.length, 2);
  assert.ok(opened.every(file => file.fd === -1), 'every acquired descriptor must close');
});
