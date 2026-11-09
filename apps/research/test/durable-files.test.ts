import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, readdir, rm, stat, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { atomicWriteFile, syncTree } from '../src/durable-files.ts';

test('atomic publication streams complete bytes and preserves the previous version on producer failure', async t => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-durable-files-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const path = join(directory, 'nested', 'artifact.json');
  await atomicWriteFile(path, '{"generation":1}');
  const failure = new Error('producer_failed');
  async function* broken() { yield '{"generation":'; throw failure; }
  await assert.rejects(atomicWriteFile(path, broken()), error => error === failure);
  assert.equal(await readFile(path, 'utf8'), '{"generation":1}');
  assert.deepEqual(await readdir(join(directory, 'nested')), ['artifact.json']);
  async function* complete() { yield '{"text":"'; yield Buffer.from('研究'); yield '"}'; }
  await atomicWriteFile(path, complete());
  assert.deepEqual(JSON.parse(await readFile(path, 'utf8')), { text: '研究' });
  assert.equal((await stat(path)).mode & 0o777, 0o600);
});

test('private tree flushing includes files and directories without following external symlinks', async t => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-sync-tree-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await writeFile(join(directory, 'result'), 'original bytes');
  await symlink('/missing-external-source', join(directory, 'external'));
  await syncTree(directory);
  assert.equal(await readFile(join(directory, 'result'), 'utf8'), 'original bytes');
});
