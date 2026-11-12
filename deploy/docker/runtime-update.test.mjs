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
test('local runtime image overlay follows owner configuration without rewriting its settings/data', async t => {
  const f = await fixture(t);
  const result = f.run(`docker() { if [[ "$*" == *"config --services" ]]; then printf 'app\\nnautilus\\n'; else printf '%s\\n' "$@" > "$ROOT/args"; fi; }; qz_compose "$ROOT/installation.json" up -d --wait`);
  assert.equal(result.status, 0, result.stderr);
  const args = await readFile(join(f.root, 'args'), 'utf8');
  assert.ok(args.indexOf('compose.override.yaml') < args.indexOf('runtime-image.yaml'));
  assert.equal(await readFile(join(f.root, 'work/runtime-image.yaml'), 'utf8'), `services:\n  nautilus:\n    image: "sha256:${'b'.repeat(64)}"\n`);
  assert.equal(await readFile(join(f.root, 'compose.override.yaml'), 'utf8'), f.original);
  assert.equal(await readFile(join(f.root, 'data/original'), 'utf8'), 'original receipts');
});
for (const running of ['true', 'false']) test(`runtime update requires an already stopped original worker (${running})`, async t => {
  const f = await fixture(t);
  const result = f.run(`qz_compose() { if [[ "$*" == *'config --services' ]]; then echo nautilus; elif [[ "$*" == *'config --format json' ]]; then echo '{"services":{"nautilus":{"environment":{}}}}'; else echo original-container; fi; }; docker() { if [[ "$*" == *'.State.Running'* ]]; then echo ${running}; elif [[ "$*" == *'.Config.Entrypoint'* ]]; then echo '["/usr/bin/tini","--","node","--experimental-strip-types","/opt/quazonai/apps/nautilus/src/main.ts"]'; fi; }; qz_runtime_stopped "$ROOT/installation.json"`);
  assert.equal(result.status === 0, running === 'false', result.stderr);
  if (running === 'true') assert.match(result.stderr, /never cancels remote work/);
});
for (const actual of ['matching', 'wrong']) test(`runtime verification checks actual container image (${actual})`, async t => {
  const f = await fixture(t);
  const result = f.run(`qz_compose() { echo runtime-container; }; docker() { case "$*" in *'image inspect'*) echo expected-id;; *'.Image'*) echo ${actual === 'matching' ? 'expected-id' : 'other-id'};; *) echo true;; esac; }; qz_runtime_verify "$ROOT/installation.json"`);
  assert.equal(result.status === 0, actual === 'matching', result.stderr);
});
test('managed runtime cannot silently create a replacement when owner service is missing', async t => {
  const f = await fixture(t);
  const result = f.run(`docker() { echo app; }; qz_compose "$ROOT/installation.json" up -d`);
  assert.notEqual(result.status, 0); assert.match(result.stderr, /service is missing/);
});
for (const mismatch of ['none', 'revision', 'version']) test(`runtime prepare verifies same-release labels (${mismatch})`, async t => {
  const f = await fixture(t); const revision = 'c'.repeat(40), version = 'v3.0.0-dev.2';
  await mkdir(join(f.root, 'bundle'));
  await writeFile(join(f.root, 'bundle/json.awk'), await readFile(new URL('./json.awk', import.meta.url)));
  await writeFile(join(f.root, 'bundle/release.json'), JSON.stringify({ schema_version: 5, version, revision, image: 'sha256:'+'a'.repeat(64), runtime_image: 'sha256:'+'b'.repeat(64) }));
  const result = f.run(`QZ_BUNDLE="$ROOT/bundle"; QZ_BUNDLE_FILES=(); docker() {
    if [[ "$*" == *'org.opencontainers.image.version'* && "$*" == *"${'b'.repeat(64)}"* ]]; then echo ${mismatch === 'version' ? 'v0.0.0' : version};
    elif [[ "$*" == *'org.opencontainers.image.version'* ]]; then echo ${version};
    elif [[ "$*" == *"${'b'.repeat(64)}"* ]]; then echo ${mismatch === 'revision' ? 'wrong' : revision};
    else echo ${revision}; fi;
  }; qz_prepare "$ROOT" "$ROOT/installation.json"; qz_get "$QZ_PREPARED" local_runtime_service`);
  assert.equal(result.status === 0, mismatch === 'none', result.stderr);
  if (mismatch === 'none') assert.match(result.stdout, /nautilus/);
  else assert.match(result.stderr, /Runtime (revision|version) differs/);
  assert.equal(await readFile(join(f.root, 'compose.override.yaml'), 'utf8'), f.original);
});
test('running runtime blocks update before app stop or activation and creates no pending operation', async t => {
  const f = await fixture(t);
  const config = JSON.parse(await readFile(join(f.root, 'installation.json'), 'utf8'));
  const old = { ...config, schema_version: 5, version: 'v3.0.0-dev.1', revision: 'c'.repeat(40) };
  const candidate = { ...old, version: 'v3.0.0-dev.2' };
  await writeFile(join(f.root, 'installation.json'), JSON.stringify(old));
  await writeFile(join(f.root, 'candidate.json'), JSON.stringify(candidate));
  const result = f.run(`qz_configuration() { QZ_CONFIG="$ROOT/installation.json"; }; qz_manifest() { :; };
    qz_prepare() { QZ_PREPARED="$ROOT/candidate.json"; };
    qz_compose() { if [[ "$*" == *'config --services' ]]; then echo nautilus; else echo original-container; fi; };
    docker() { echo true; }; qz_stop() { touch "$ROOT/stopped"; }; qz_activate() { touch "$ROOT/activated"; };
    qz_apply_update "$ROOT"`);
  assert.notEqual(result.status, 0); assert.match(result.stderr, /never cancels remote work/);
  for (const name of ['stopped', 'activated', 'pending.json']) await assert.rejects(readFile(join(f.root, name)), { code: 'ENOENT' });
  assert.equal(await readFile(join(f.root, 'installation.json'), 'utf8'), JSON.stringify(old));
});
test('installer rejects local-runtime enrollment for CLI-only operations', () => {
  const result = spawnSync('bash', [fileURLToPath(new URL('../install.sh', import.meta.url)), '--cli-only', '--local-runtime'], { encoding: 'utf8' });
  assert.notEqual(result.status, 0); assert.match(result.stderr, /requires a Docker stack update/);
});
test('restore also refuses to replace a managed runtime while it is running', async t => {
  const f = await fixture(t);
  const result = f.run(`qz_configuration() { QZ_CONFIG="$ROOT/installation.json"; }; qz_manifest() { :; };
    qz_compose() { if [[ "$*" == *'config --services' ]]; then echo nautilus; else echo original-container; fi; };
    docker() { echo true; }; qz_restore "$ROOT" "$ROOT"`);
  assert.notEqual(result.status, 0); assert.match(result.stderr, /never cancels remote work/);
  assert.equal(await readFile(join(f.root, 'data/original'), 'utf8'), 'original receipts');
});

for (const configured of ['old', 'candidate', 'invalid', 'unknown-entrypoint']) test(`stopped runtime is not automatically safe to replace (${configured})`, async t => {
  const f = await fixture(t);
  const effective = { services: { nautilus: { environment: configured === 'candidate' ? { QZ_NAUTILUS_ACCOUNTS: '[{"mode":"live"}]' } : {} } } };
  await writeFile(join(f.root, 'effective.json'), JSON.stringify(effective));
  const result = f.run(`qz_compose() { if [[ "$*" == *'config --services' ]]; then echo nautilus; elif [[ "$*" == *'config --format json' ]]; then cat "$ROOT/effective.json"; else echo original-container; fi; };
    docker() { if [[ "$*" == *'.State.Running'* ]]; then echo false;
      elif [[ "$*" == *'.Config.Entrypoint'* ]]; then echo '${configured === 'unknown-entrypoint' ? '["custom"]' : '["/usr/bin/tini","--","node","--experimental-strip-types","/opt/quazonai/apps/nautilus/src/main.ts"]'}';
      else echo '${configured === 'old' ? 'QZ_NAUTILUS_ACCOUNTS=[{"mode":"paper"}]' : configured === 'invalid' ? 'QZ_NAUTILUS_ACCOUNTS=invalid' : ''}'; fi; };
    qz_runtime_stopped "$ROOT/installation.json"`);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /[Aa]ccount|recovery plan/);
  assert.equal(await readFile(join(f.root, 'data/original'), 'utf8'), 'original receipts');
  assert.equal(await readFile(join(f.root, 'compose.override.yaml'), 'utf8'), f.original);
});

for (const actualVersion of ['v3.0.0-dev.2', 'v3.0.0-dev.1']) test(`application prepare checks release version as well as matching revision (${actualVersion})`, async t => {
  const f = await fixture(t), revision = 'c'.repeat(40), version = 'v3.0.0-dev.2';
  await mkdir(join(f.root, 'bundle'));
  await writeFile(join(f.root, 'bundle/json.awk'), await readFile(new URL('./json.awk', import.meta.url)));
  await writeFile(join(f.root, 'bundle/release.json'), JSON.stringify({ schema_version: 5, version, revision, image: 'sha256:'+'a'.repeat(64), runtime_image: 'sha256:'+'b'.repeat(64) }));
  const config = JSON.parse(await readFile(join(f.root, 'installation.json'), 'utf8'));
  delete config.local_runtime_service;
  await writeFile(join(f.root, 'installation.json'), JSON.stringify(config));
  const result = f.run(`QZ_BUNDLE="$ROOT/bundle"; QZ_BUNDLE_FILES=(); docker() {
    if [[ "$*" == *'org.opencontainers.image.version'* ]]; then echo ${actualVersion}; else echo ${revision}; fi;
  }; qz_prepare "$ROOT" "$ROOT/installation.json"`);
  assert.equal(result.status === 0, actualVersion === version, result.stderr);
  if (actualVersion !== version) assert.match(result.stderr, /Image version differs/);
  assert.equal(await readFile(join(f.root, 'data/original'), 'utf8'), 'original receipts');
});

for (const actual of ['matching', 'wrong', 'absent', 'multiple', 'stopped']) test(`application verification checks actual container image and state (${actual})`, async t => {
  const f = await fixture(t);
  const result = f.run(`qz_compose() { printf '%s\\n' '${actual === 'absent' ? '' : actual === 'multiple' ? 'app-one\napp-two' : 'app-container'}'; };
    docker() { case "$*" in *'image inspect'*) echo expected-id;; *'.Image'*) echo ${actual === 'wrong' ? 'other-id' : 'expected-id'};; *) echo ${actual === 'stopped' ? 'false' : 'true'};; esac; };
    qz_app_verify "$ROOT/installation.json"`);
  assert.equal(result.status === 0, actual === 'matching', result.stderr);
  if (actual === 'wrong') assert.match(result.stderr, /Running application differs from the release image/);
  assert.equal(await readFile(join(f.root, 'compose.override.yaml'), 'utf8'), f.original);
  assert.equal(await readFile(join(f.root, 'data/original'), 'utf8'), 'original receipts');
});
test('application image mismatch refuses activation before publishing the new installation identity', async t => {
  const f = await fixture(t);
  const original = await readFile(join(f.root, 'installation.json'), 'utf8');
  const result = f.run(`qz_compose() { echo app-container; };
    docker() { case "$*" in *'image inspect'*) echo expected-id;; *'.Image'*) echo old-id;; *) echo true;; esac; };
    qz_start() { qz_verify "$1"; };
    qz_save() { touch "$ROOT/saved"; };
    qz_activate "$ROOT" "$ROOT/installation.json"`);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Running application differs from the release image/);
  await assert.rejects(readFile(join(f.root, 'saved')), { code: 'ENOENT' });
  assert.equal(await readFile(join(f.root, 'installation.json'), 'utf8'), original);
  assert.equal(await readFile(join(f.root, 'compose.override.yaml'), 'utf8'), f.original);
});
