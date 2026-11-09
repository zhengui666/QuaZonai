import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { NodeSqliteDatabase } from '@earendil-works/pi-durable/storage/sqlite/node';
import { acquireDurableOwner, openDurableStorage } from '../src/durable-storage.ts';
import { openResearch } from '../src/host.ts';

const context = BACKGROUND_CONTEXT;
async function temporary(t: { after(fn: () => Promise<void>): void }) {
  const directory = await mkdtemp(join(tmpdir(), 'qz-durable-owner-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return directory;
}

test('official storage verifies FULL/WAL on its actual writer connection and owns it until close', async t => {
  const directory = await temporary(t); const path = join(directory, 'original.sqlite');
  const observed: unknown[] = [];
  const get = NodeSqliteDatabase.prototype.get;
  NodeSqliteDatabase.prototype.get = async function<T extends object>(sql: string, ...parameters: Parameters<typeof get>[1][]) {
    const value = await get.call(this, sql, ...parameters);
    if (sql === 'PRAGMA synchronous' || sql === 'PRAGMA journal_mode') observed.push(value);
    return value as T | undefined;
  };
  let storage;
  try { storage = await openDurableStorage(path); }
  finally { NodeSqliteDatabase.prototype.get = get; }
  try {
    assert.deepEqual(observed.map(value => ({ ...value as object })), [{ synchronous: 2 }, { journal_mode: 'wal' }]);
    await assert.rejects(openDurableStorage(path), /durable_database_already_owned/);
    const alias = join(directory, 'alias.sqlite'); await symlink(path, alias);
    await assert.rejects(openDurableStorage(alias), /durable_database_already_owned/);
  } finally { await storage.close(context); }
  storage = await openDurableStorage(path, { requireOriginal: true });
  await storage.close(context); await storage.close(context);
});

test('required original database is never replaced or initialized, and failed opens release ownership', async t => {
  const directory = await temporary(t); const path = join(directory, 'original.sqlite');
  await assert.rejects(openDurableStorage(path, { requireOriginal: true }));
  await assert.rejects(readFile(path), { code: 'ENOENT' });
  await writeFile(path, 'not a durable database');
  await assert.rejects(openDurableStorage(path, { requireOriginal: true }));
  assert.equal(await readFile(path, 'utf8'), 'not a durable database');
  const owner = await acquireDurableOwner(path); owner.close();
});

test('a killed owner releases the OS lock and the same original ledger reopens', { timeout: 15_000 }, async t => {
  const directory = await temporary(t); const path = join(directory, 'original.sqlite');
  const module = new URL('../src/durable-storage.ts', import.meta.url).href;
  const script = `import { openDurableStorage } from ${JSON.stringify(module)}; await openDurableStorage(process.argv[1]); process.send('opened'); setInterval(() => {}, 1000);`;
  const child = spawn(process.execPath, ['--experimental-strip-types', '--input-type=module', '-e', script, path], { stdio: ['ignore', 'ignore', 'pipe', 'ipc'] });
  let errors = ''; child.stderr?.on('data', chunk => { errors += chunk; });
  t.after(() => { child.kill('SIGKILL'); return Promise.resolve(); });
  await Promise.race([once(child, 'message'), once(child, 'exit').then(([code]) => { throw new Error(`owner exited ${code}: ${errors}`); })]);
  await assert.rejects(openDurableStorage(path), /durable_database_already_owned/);
  const exit = once(child, 'exit'); child.kill('SIGKILL'); await exit;
  const storage = await openDurableStorage(path, { requireOriginal: true });
  await storage.close(context);
});

test('host rejects a second owner before loading provider modules and releases after failed initialization', async t => {
  const directory = await temporary(t); const database = join(directory, 'original.sqlite');
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  const host = await openResearch({ database, models });
  try { await assert.rejects(openResearch({ database, providerModules: ['/missing-provider.ts'] }), /durable_database_already_owned/); }
  finally { await host.close(); }
  await assert.rejects(openResearch({ database, models, modules: ['/missing-extension.ts'] }), /missing-extension/);
  const reopened = await openResearch({ database, models, requireOriginalDatabase: true });
  await reopened.close();
});


test('a previously initialized instance refuses a missing original database on ordinary restart', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-original-ledger-'));
  const path = join(root, 'original.sqlite');
  try {
    const storage = await openDurableStorage(path);
    await storage.close(BACKGROUND_CONTEXT);
    await rm(path);
    await assert.rejects(openDurableStorage(path));
    await assert.rejects(readFile(path), { code: 'ENOENT' });
  } finally { await rm(root, { recursive: true, force: true }); }
});
