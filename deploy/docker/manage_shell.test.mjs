import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, readFile, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import test from 'node:test';
import { configure } from './configure.mjs';
import { requireIdle, context } from './maintenance.mjs';
const require = createRequire(new URL('../../apps/service/package.json', import.meta.url));
const { openNodeSqliteStorage } = await import(require.resolve('@earendil-works/pi-durable/storage/sqlite/node'));
const scratch = process.env.PI_SCRATCH_DIR ?? process.env.RUNNER_TEMP ?? tmpdir();
async function directory(t) { const root = await mkdtemp(join(scratch, 'qz-deploy-test-')); t.after(() => rm(root, { recursive: true, force: true })); return root; }

test('legacy update refuses before Docker/systemd or writes, with concrete original paths', async (t) => {
  const root = await directory(t); const original = '{"schema_version":3,"private":"preserve"}\n';
  await writeFile(join(root, 'installation.json'), original);
  const result = spawnSync('bash', [fileURLToPath(new URL('./manage.sh', import.meta.url)), 'apply-update', '--directory', root], { encoding: 'utf8' });
  assert.notEqual(result.status, 0); assert.match(result.stderr, /Legacy upgrade is not automatic/); assert.match(result.stderr, /data\/research\/config.json/);
  assert.equal(await readFile(join(root, 'installation.json'), 'utf8'), original); assert.deepEqual(await readdir(root), ['installation.json']);
});
test('adopt the original research database and private modules; configure never opens/replaces storage', async (t) => {
  const root = await directory(t); const research = join(root, 'data/research'); await mkdir(research, { recursive: true });
  const database = join(research, 'original.sqlite'); await writeFile(database, 'original bytes');
  const old = { database: 'original.sqlite', modules: ['/opt/quazonai/research/plugins/research.ts', './private.ts'], providerModules: ['./provider.ts'], tuning: { concurrency: 2 } };
  await writeFile(join(research, 'config.json'), JSON.stringify(old)); await writeFile(join(research, 'provider.env'), '# private original env\n');
  const path = configure(root, 'http://localhost:8081'); const config = JSON.parse(await readFile(path, 'utf8'));
  assert.equal(config.research.database, database); assert.equal(config.research.modules[0], '/opt/quazonai/apps/research/plugins/research.ts');
  assert.equal(config.research.modules[1], join(research, 'private.ts')); assert.deepEqual(config.research.tuning, old.tuning);
  assert.equal(await readFile(database, 'utf8'), 'original bytes'); assert.equal(await readFile(join(research, 'provider.env'), 'utf8'), '# private original env\n');
  const saved = await readFile(path, 'utf8'); configure(root, 'http://localhost:9999'); assert.equal(await readFile(path, 'utf8'), saved);
});
test('stopped official storage inspection rejects unresolved native work, never initializes an absent DB', async (t) => {
  const root = await directory(t); const config = configure(root, 'http://localhost:8081'); const database = JSON.parse(await readFile(config, 'utf8')).research.database;
  await assert.rejects(requireIdle(config));
  const storage = await openNodeSqliteStorage(database);
  await storage.close(context); await requireIdle(config);
  const writer = await openNodeSqliteStorage(database);
  const id = await writer.mintId();
  await writer.commit([{ type: 'task', value: { id, conversationId: 0, kind: 'test.native', version: 1, input: null, background: false, abortRequested: false, state: { status: 'pending', checkpoint: null } } }], context);
  await writer.close(context);
  await assert.rejects(requireIdle(config), /Unresolved durable task/);
  const readback = await openNodeSqliteStorage(database); assert.equal((await readback.task(id, context)).state.status, 'pending'); await readback.close(context);
});
