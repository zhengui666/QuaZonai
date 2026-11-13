import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
const manager = fileURLToPath(new URL('./manage.sh', import.meta.url));
async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'qz-runtime-release-')); t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, 'work')); await mkdir(join(root, 'data'));
  const original = 'services:\n  nautilus:\n    image: old@sha256:original\n    environment: {PRIVATE_TOKEN: unchanged}\n    volumes: [./data:/data]\n    cpus: 2\n';
  await writeFile(join(root, 'compose.override.yaml'), original); await writeFile(join(root, 'data', 'original'), 'original receipts');
  const config = { root, project: 'test', port: 8081, uid: 123, gid: 123, bundle: root, image: 'sha256:' + 'a'.repeat(64), runtime_image: 'sha256:' + 'b'.repeat(64), local_runtime_service: 'nautilus' };
  await writeFile(join(root, 'installation.json'), JSON.stringify(config));
  const run = script => spawnSync('bash', ['-c', 'source "$1"; ROOT=$2; QZ_WORK="$ROOT/work"; '+script, 'test', manager, root], { encoding: 'utf8', timeout: 10000 });
  return { root, original, run };
}
test('old runtime enrollment never injects an image overlay or changes owner files', async t => {
  const f = await fixture(t);
  const result = f.run(`docker() { printf '%s\\n' "$@" > "$ROOT/args"; }; qz_compose "$ROOT/installation.json" ps`);
  assert.equal(result.status, 0, result.stderr);
  const args = await readFile(join(f.root, 'args'), 'utf8');
  assert.ok(args.includes('compose.override.yaml')); assert.ok(!args.includes('runtime-image.yaml'));
  await assert.rejects(readFile(join(f.root, 'work/runtime-image.yaml')), { code: 'ENOENT' });
  assert.equal(await readFile(join(f.root, 'compose.override.yaml'), 'utf8'), f.original);
  assert.equal(await readFile(join(f.root, 'data/original'), 'utf8'), 'original receipts');
});
test('start explicitly scopes Compose to app without starting dependencies', async t => {
  const f = await fixture(t);
  const result = f.run(`qz_verify() { :; }; qz_compose() { printf '%s\\n' "$*" >> "$ROOT/calls"; }; qz_start "$ROOT/installation.json"`);
  assert.equal(result.status, 0, result.stderr);
  const calls = await readFile(join(f.root, 'calls'), 'utf8');
  assert.match(calls, /up -d --no-deps --wait --wait-timeout 120 app/);
  assert.ok(!calls.includes('nautilus')); assert.ok(!calls.includes(' down'));
});
test('installer rejects retired local-runtime management before doing any work', () => {
  const result = spawnSync('bash', [fileURLToPath(new URL('../install.sh', import.meta.url)), '--local-runtime'], { encoding: 'utf8' });
  assert.notEqual(result.status, 0); assert.match(result.stderr, /management has been removed/);
});

for (const services of ['app\nnautilus', 'inspection-failed', 'app']) test(`restore refuses unsafe or unknown native scope before changing files (${services})`, async t => {
  const f = await fixture(t);
  if (services === 'app') await mkdir(join(f.root, 'data/nautilus'));
  const result = f.run(`qz_configuration() { QZ_CONFIG="$ROOT/installation.json"; }; qz_manifest() { :; };
    qz_compose() { ${services === 'inspection-failed' ? 'return 1' : `printf '%s\\n' '${services}'`}; };
    qz_stop() { touch "$ROOT/stopped"; }; qz_restore "$ROOT" "$ROOT"`);
  assert.notEqual(result.status, 0); assert.match(result.stderr, /restore scope|external Compose services|Native account data/);
  await assert.rejects(readFile(join(f.root, 'stopped')), { code: 'ENOENT' });
  assert.equal(await readFile(join(f.root, 'compose.override.yaml'), 'utf8'), f.original);
  assert.equal(await readFile(join(f.root, 'data/original'), 'utf8'), 'original receipts');
});
