import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { copyNativeFile, nativeDestination } from './native-files.mjs';

function fixture(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'quazonai-native-copy-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const root = path.join(directory, 'image');
  fs.mkdirSync(root);
  const source = path.join(directory, 'native-source');
  return { directory, root, source };
}

test('native image copy preserves full multi-chunk bytes and executable permissions', (t) => {
  const f = fixture(t);
  const bytes = Buffer.alloc(3 * 1024 * 1024 + 17);
  for (let index = 0; index < bytes.length; index += 4093) bytes[index] = index % 251;
  fs.writeFileSync(f.source, bytes, { mode: 0o751 });
  // Creation honors the invoking user's umask; set the intended native source
  // permissions explicitly before testing preservation, without changing umask.
  fs.chmodSync(f.source, 0o751);
  copyNativeFile(f.root, f.source, '/opt/rust/bin/native');
  const output = nativeDestination(f.root, '/opt/rust/bin/native');
  const observe = () => {
    const descriptor = fs.openSync(output, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
    try {
      return { bytes: fs.readFileSync(descriptor), mode: fs.fstatSync(descriptor).mode & 0o777 };
    } finally { fs.closeSync(descriptor); }
  };
  assert.deepEqual(observe(), { bytes, mode: 0o751 });
  copyNativeFile(f.root, f.source, '/opt/rust/bin/native');
  assert.deepEqual(observe(), { bytes, mode: 0o751 });
});

test('same-size conflicting destination remains byte-for-byte unchanged', (t) => {
  const f = fixture(t);
  fs.writeFileSync(f.source, 'original');
  copyNativeFile(f.root, f.source, '/lib/native.so');
  fs.writeFileSync(f.source, 'modified');
  assert.throws(() => copyNativeFile(f.root, f.source, '/lib/native.so'), /Conflicting native library/);
  assert.equal(fs.readFileSync(nativeDestination(f.root, '/lib/native.so'), 'utf8'), 'original');
});

test('a destination symlink is rejected without reading or overwriting its target', (t) => {
  const f = fixture(t);
  fs.writeFileSync(f.source, 'library');
  const sentinel = path.join(f.directory, 'sentinel');
  fs.writeFileSync(sentinel, 'unchanged private fixture');
  fs.symlinkSync(sentinel, path.join(f.root, 'library'));
  assert.throws(() => copyNativeFile(f.root, f.source, '/library'), { code: 'ELOOP' });
  assert.equal(fs.readFileSync(sentinel, 'utf8'), 'unchanged private fixture');
  assert.ok(fs.lstatSync(path.join(f.root, 'library')).isSymbolicLink());
});

test('legitimate native source symlinks select one regular source descriptor', (t) => {
  const f = fixture(t);
  fs.writeFileSync(f.source, 'native linked library');
  const alias = path.join(f.directory, 'libnative.so');
  fs.symlinkSync(f.source, alias);
  copyNativeFile(f.root, alias, '/lib/libnative.so');
  assert.equal(fs.readFileSync(path.join(f.root, 'lib/libnative.so'), 'utf8'), 'native linked library');
});

test('directory sources and noncanonical image destinations cannot be copied', (t) => {
  const f = fixture(t);
  fs.writeFileSync(f.source, 'native');
  assert.throws(() => copyNativeFile(f.root, f.directory, '/not-a-file'), /regular file/);
  for (const target of ['relative', '/lib/../outside', '/', '//double-prefix']) {
    assert.throws(() => copyNativeFile(f.root, f.source, target), /Invalid native image path/);
  }
  assert.equal(fs.readdirSync(f.root).length, 0);
});
