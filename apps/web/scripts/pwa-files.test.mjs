// Native Linux filesystem regressions: real directories, rename barriers and reads.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { constants } from 'node:fs';
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
  await fs.mkdir(join(root, 'one/two'), { recursive: true });
  await fs.writeFile(join(root, 'one/two/deep.js'), 'real nested bytes');
  assert.equal((await readStaticAsset(root, '/one/two/deep.js')).bytes.toString(), 'real nested bytes');
  for (const pathname of ['/missing.js', '/directory', '/../outside.js', '/%2e%2e%2foutside.js', '/%00', '/%zz', 'relative.js', `/${'one/'.repeat(33)}deep.js`]) {
    await assert.rejects(readStaticAsset(root, pathname), `must reject ${pathname}`);
  }
});

test('rejects final, intermediate and root symlinks, including trailing slash roots', async (t) => {
  const { directory, root } = await fixture(t);
  await fs.symlink(join(root, 'asset.js'), join(root, 'alias.js'));
  await assert.rejects(readStaticAsset(root, '/alias.js'), { code: 'ELOOP' });
  const outside = join(directory, 'outside');
  await fs.mkdir(outside);
  await fs.writeFile(join(outside, 'secret.js'), 'must not be served');
  await fs.symlink(outside, join(root, 'linked'));
  await assert.rejects(readStaticAsset(root, '/linked/secret.js'));
  await assert.rejects(readStaticAsset(root, '/%6cinked/secret.js'));
  await fs.mkdir(join(root, 'inside'));
  await fs.writeFile(join(root, 'inside/file.js'), 'inside build');
  await fs.symlink(join(root, 'inside'), join(root, 'inside-link'));
  await assert.rejects(readStaticAsset(root, '/inside-link/file.js'));
  const rootAlias = join(directory, 'root-alias');
  await fs.symlink(root, rootAlias);
  await assert.rejects(readStaticAsset(rootAlias, '/asset.js'));
  await assert.rejects(readStaticAsset(rootAlias + '/', '/asset.js'));
});

test('a pathname replacement after fstat cannot substitute the opened bytes', async (t) => {
  const { root } = await fixture(t);
  const target = join(root, 'asset.js');
  const open = fs.open.bind(fs);
  const opened = [];
  let swapped = false;
  t.mock.method(fs, 'open', async (...args) => {
    const file = await open(...args);
    opened.push(file);
    if ((args[1] & constants.O_DIRECTORY) === 0) {
      const stat = file.stat.bind(file);
      t.mock.method(file, 'stat', async (...statArgs) => {
        const metadata = await stat(...statArgs);
        await fs.rename(target, join(root, 'previous.js'));
        await fs.writeFile(target, 'replacement-bytes');
        swapped = true;
        return metadata;
      });
    }
    return file;
  });
  const asset = await readStaticAsset(root, '/asset.js');
  assert.equal(swapped, true);
  assert.equal(asset.bytes.toString(), 'original-bytes');
  assert.equal(await fs.readFile(target, 'utf8'), 'replacement-bytes');
  assert.equal(opened.length, 2);
  assert.ok(opened.every(file => file.fd === -1));
});

test('renaming an opened intermediate directory cannot redirect its child open', async (t) => {
  const { directory, root } = await fixture(t);
  const nested = join(root, 'nested');
  const outside = join(directory, 'outside');
  await fs.mkdir(nested); await fs.mkdir(outside);
  await fs.writeFile(join(nested, 'asset.js'), 'safe original child');
  await fs.writeFile(join(outside, 'asset.js'), 'outside replacement');
  const open = fs.open.bind(fs);
  const opened = [];
  let swapped = false;
  t.mock.method(fs, 'open', async (...args) => {
    const file = await open(...args);
    opened.push(file);
    if (!swapped && String(args[0]).endsWith('/nested')) {
      await fs.rename(nested, join(root, 'previous-nested'));
      await fs.symlink(outside, nested);
      swapped = true;
    }
    return file;
  });
  const asset = await readStaticAsset(root, '/nested/asset.js');
  assert.equal(swapped, true);
  assert.equal(asset.bytes.toString(), 'safe original child');
  assert.ok(opened.every(file => file.fd === -1));
  await assert.rejects(readStaticAsset(root, '/nested/asset.js'));
  assert.ok(opened.every(file => file.fd === -1), 'failed subsequent opens close the root');
});

test('closes every acquired descriptor on validation, reading and close failures', async (t) => {
  const { root } = await fixture(t);
  await fs.mkdir(join(root, 'directory'));
  const open = fs.open.bind(fs);
  const opened = [];
  let failRead = false;
  let failClose = false;
  t.mock.method(fs, 'open', async (...args) => {
    const file = await open(...args);
    opened.push(file);
    if (failRead && (args[1] & constants.O_DIRECTORY) === 0) {
      t.mock.method(file, 'readFile', async () => { throw Object.assign(new Error('injected read failure'), { code: 'EIO' }); });
    }
    if (failClose && (args[1] & constants.O_DIRECTORY) === 0) {
      const close = file.close.bind(file);
      t.mock.method(file, 'close', async () => { await close(); throw new Error('injected close result'); });
    }
    return file;
  });
  await assert.rejects(readStaticAsset(root, '/directory'), /regular file/);
  failRead = true;
  await assert.rejects(readStaticAsset(root, '/asset.js'), { code: 'EIO' });
  failRead = false; failClose = true;
  await assert.rejects(readStaticAsset(root, '/asset.js'), /injected close result/);
  assert.equal(opened.length, 6);
  assert.ok(opened.every(file => file.fd === -1), 'close failure must not skip parent handles');
});
