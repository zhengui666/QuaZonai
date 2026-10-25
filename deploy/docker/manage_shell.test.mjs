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
  assert.deepEqual(config.nautilus, {}); assert.equal(config.jobBinary, undefined);
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

test('restore an override-free backup preserves the newer override only in pre-restore recovery, including starting retry', async (t) => {
  const root = await directory(t); const bundle = join(root, 'bundle');
  for (const name of ['data', 'work', 'bundle']) await mkdir(join(root, name));
  const image = `sha256:${'a'.repeat(64)}`;
  const config = { schema_version: 4, version: 'v0.0.0-ci.1', revision: 'b'.repeat(40), root, uid: process.getuid(),
    project: 'restore-test', password: 'test-only', image, runtime_image: image, database_image: image, bundle };
  await writeFile(join(bundle, 'release.json'), JSON.stringify(config));
  await writeFile(join(root, 'installation.json'), JSON.stringify(config));
  await writeFile(join(root, 'data/original'), 'selected backup data');
  // Files, archives, recovery markers and backup/restore code are real; only
  // the Docker process boundary and activation failure are controlled here.
  const shell = (body, backup = '') => spawnSync('bash', ['-c', `source "$1"
ROOT=$2; QZ_WORK="$ROOT/work"
qz_stop() { :; }; qz_idle() { :; }
qz_compose() {
  case " $* " in
    *" pg_dump "*) printf 'test-only PG dump';;
    *" pg_restore "*) cat >/dev/null; printf 'restore\\n' >> "$ROOT/pg-restores";;
    *) return 1;;
  esac
}
${body}`, 'qz-restore-test', fileURLToPath(new URL('./manage.sh', import.meta.url)), root, backup], { encoding: 'utf8', timeout: 20_000 });
  let result = shell('qz_backup "$ROOT/installation.json"; printf %s "$QZ_BACKUP" > "$ROOT/selected-backup"');
  assert.equal(result.status, 0, result.stderr);
  const selected = await readFile(join(root, 'selected-backup'), 'utf8');
  await assert.rejects(readFile(join(selected, 'compose.override.yaml')), { code: 'ENOENT' });
  const newerOverride = 'services:\n  app:\n    environment:\n      NEWER_SETTING: newer\n';
  await writeFile(join(root, 'compose.override.yaml'), newerOverride);
  await writeFile(join(root, 'data/original'), 'pre-restore data');
  result = shell('qz_start() { return 23; }; qz_restore "$ROOT" "$3"', selected);
  assert.equal(result.status, 23, result.stderr);
  const pending = JSON.parse(await readFile(join(root, 'pending.json'), 'utf8'));
  assert.equal(pending.phase, 'starting'); assert.equal(pending.source, selected);
  assert.equal(await readFile(join(pending.backup, 'compose.override.yaml'), 'utf8'), newerOverride);
  assert.equal(await readFile(join(pending.backup, 'data.original/original'), 'utf8'), 'pre-restore data');
  await assert.rejects(readFile(join(root, 'compose.override.yaml')), { code: 'ENOENT' });
  result = shell('qz_start() { [[ ! -e "$ROOT/compose.override.yaml" ]]; }; qz_restore "$ROOT" "$3"', selected);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(await readFile(join(root, 'data/original'), 'utf8'), 'selected backup data');
  assert.equal(await readFile(join(root, 'pg-restores'), 'utf8'), 'restore\n');
  assert.equal((await readdir(join(root, 'backups'))).length, 2, 'starting retry must not replace the pre-restore backup');
  await assert.rejects(readFile(join(root, 'pending.json')), { code: 'ENOENT' });
});

test('scientific scratch uses the writable data volume on fresh and existing configuration', async (t) => {
  const root = await directory(t);
  const path = configure(root, 'http://localhost:8081');
  const scientificScratch = join(root, 'data/scratch');
  assert.deepEqual(await readdir(scientificScratch), []);
  const original = await readFile(path, 'utf8');
  await rm(scientificScratch, { recursive: true });
  configure(root, 'http://localhost:9999');
  assert.deepEqual(await readdir(scientificScratch), []);
  assert.equal(await readFile(path, 'utf8'), original);
  const compose = await readFile(new URL('./compose.yaml', import.meta.url), 'utf8');
  assert.match(compose, /QZ_SCRATCH_DIR: \$\{INSTALL_DIR:\?\}\/data\/scratch/);
  assert.match(compose, /TMPDIR: \$\{INSTALL_DIR:\?\}\/data\/scratch/);
});

test('official existing-install update creates scientific scratch at service start without configure', async (t) => {
  const root = await directory(t);
  for (const name of ['data/research', 'bundle', 'work', 'commands']) await mkdir(join(root, name), { recursive: true });
  const image = `sha256:${'a'.repeat(64)}`;
  const old = { schema_version: 4, version: 'v0.0.0-ci.1', revision: 'b'.repeat(40), root,
    uid: process.getuid(), gid: process.getgid(), project: 'scratch-update-test', password: 'test-only',
    port: 18081, database_port: 15433, image, runtime_image: image, database_image: image, bundle: join(root, 'bundle') };
  const candidate = { ...old, version: 'v0.0.0-ci.2', revision: 'c'.repeat(40) };
  await writeFile(join(root, 'installation.json'), JSON.stringify(old));
  await writeFile(join(root, 'candidate.json'), JSON.stringify(candidate));
  await writeFile(join(root, 'bundle/release.json'), JSON.stringify(candidate));
  await writeFile(join(root, 'bundle/json.awk'), await readFile(new URL('./json.awk', import.meta.url)));
  const config = '{"research":{"database":"original.sqlite"}}\n';
  await writeFile(join(root, 'data/service.json'), config);
  await writeFile(join(root, 'data/research/original.sqlite'), 'preserved original bytes');
  await writeFile(join(root, 'data/research/provider.env'), '# test-only unchanged provider file\n');
  await writeFile(join(root, 'commands/node'), '#!/bin/sh\n[ -d "$QZ_SCRATCH_DIR" ] || exit 23\nprintf started > "$QZ_SCRATCH_DIR/node-started"\n', { mode: 0o700 });
  // Real update/backup/entrypoint handling; only image preparation, Docker,
  // Node process and HTTP boundaries are controlled. Never pre-run configure.
  const result = spawnSync('bash', ['-c', `source "$1"
ROOT=$2; ENTRYPOINT=$3; QZ_WORK="$ROOT/work"; QZ_BUNDLE="$ROOT/bundle"
qz_prepare() { QZ_PREPARED="$ROOT/candidate.json"; }
qz_stop() { :; }
qz_verify() { :; }
qz_compose() {
  case " $* " in
    *" configure "*) printf 'configure must not run in this update regression' >&2; return 91;;
    *" pg_dump "*) printf 'controlled PG dump';;
    *" maintenance "*) :;;
    *" up "*) PATH="$ROOT/commands:$PATH" QZ_CONFIG="$ROOT/data/service.json" \\
      QZ_RESEARCH_PROVIDER_ENV="$ROOT/data/research/provider.env" \\
      QZ_SCRATCH_DIR="$ROOT/data/scratch" TMPDIR="$ROOT/data/scratch" bash "$ENTRYPOINT" serve;;
    *) return 1;;
  esac
}
qz_apply_update "$ROOT"`, 'qz-scratch-update-test', fileURLToPath(new URL('./manage.sh', import.meta.url)), root,
    fileURLToPath(new URL('./entrypoint.sh', import.meta.url))], { encoding: 'utf8', timeout: 20_000 });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(await readFile(join(root, 'data/scratch/node-started'), 'utf8'), 'started');
  assert.equal(JSON.parse(await readFile(join(root, 'installation.json'), 'utf8')).version, candidate.version);
  assert.equal(await readFile(join(root, 'data/service.json'), 'utf8'), config);
  assert.equal(await readFile(join(root, 'data/research/original.sqlite'), 'utf8'), 'preserved original bytes');
  assert.equal(await readFile(join(root, 'data/research/provider.env'), 'utf8'), '# test-only unchanged provider file\n');
  assert.equal((await readdir(join(root, 'backups'))).length, 1);
  await assert.rejects(readFile(join(root, 'pending.json')), { code: 'ENOENT' });
});
