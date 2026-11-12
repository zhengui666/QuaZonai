import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, readFile, readdir, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
const installer = fileURLToPath(new URL('./install.sh', import.meta.url));
const scratch = process.env.PI_SCRATCH_DIR ?? process.env.RUNNER_TEMP ?? tmpdir();
function run(command, args, env) {
  const result = spawnSync(command, args, { env, encoding: 'utf8', timeout: 60_000 });
  assert.equal(result.status, 0, result.stderr); return result.stdout;
}
test('help has valid shell syntax and does not initialize installation state', async (t) => {
  const home = await mkdtemp(join(scratch, 'qz-install-help-')); t.after(() => rm(home, { recursive: true, force: true }));
  for (const name of ['install.sh', 'docker/manage.sh', 'docker/deploy.sh', 'docker/update.sh']) {
    const script = fileURLToPath(new URL(name, import.meta.url));
    run('bash', ['-n', script]);
    assert.match(run('bash', [script, '--help'], { ...process.env, HOME: home }), /usage:/i);
  }
  assert.deepEqual(await readdir(home), []);
});
test('real release distribution installs/repeats with bundled Node and preserves client config', { skip: !process.env.QZ_TEST_CLI_ARCHIVE }, async (t) => {
  const root = await mkdtemp(join(scratch, 'qz-cli-install-')); t.after(() => rm(root, { recursive: true, force: true }));
  const tools = join(root, 'tools'); const home = join(root, 'home space'); const bin = join(home, 'bin');
  await mkdir(tools); await mkdir(home);
  // Host Node/npm are deliberately absent from PATH. Only download is controlled;
  // archive extraction, bundled official Node and installed CLI execute for real.
  for (const tool of ['bash', 'env', 'tar', 'gzip', 'awk', 'sort', 'cmp', 'mktemp', 'chmod', 'cp', 'mv', 'mkdir', 'rm', 'uname', 'dirname', 'readlink', 'ln', 'rmdir', 'find']) {
    const path = run('which', [tool]).trim(); await symlink(path, join(tools, tool));
  }
  await writeFile(join(tools, 'curl'), '#!/bin/sh\nwhile [ "$#" -gt 0 ]; do if [ "$1" = --output ]; then shift; output=$1; fi; shift; done\nexec cp "$QZ_TEST_CLI_ARCHIVE" "$output"\n', { mode: 0o755 });
  const env = { ...process.env, HOME: home, TMPDIR: root, PATH: tools };
  const config = join(home, 'original-client.json'); await writeFile(config, 'original device connection\n');
  const version = process.env.QZ_TEST_CLI_VERSION; const skill = join(home, 'skills/quazonai');
  for (let i = 0; i < 2; i++) run('/bin/bash', [installer, '--cli-only', '--version', version, '--bin-dir', bin, '--skill-dir', skill], env);
  assert.equal(run(join(bin, 'quazonai'), ['--version'], env).trim(), `quazonai ${version}`);
  assert.match(run(join(bin, 'quazonai'), ['--help'], env), /research/);
  assert.equal(await readFile(config, 'utf8'), 'original device connection\n');
  assert.match(await readFile(join(bin, 'quazonai'), 'utf8'), /quazonai-cli\/releases/);
  assert.equal(JSON.parse(await readFile(join(skill, 'release.json'), 'utf8')).version, version);
  assert.match(await readFile(join(skill, 'SKILL.md'), 'utf8'), /quazonai/);
});

test('unmanaged skill data is refused before any download or replacement', async t => {
  const home = await mkdtemp(join(scratch, 'qz-skill-safe-')); t.after(() => rm(home, { recursive: true, force: true }));
  const skill = join(home, 'my-skill'); await mkdir(skill); await writeFile(join(skill, 'SKILL.md'), 'owner modified instructions');
  const result = spawnSync('bash', [installer, '--cli-only', '--skill-dir', skill], { env: { ...process.env, HOME: home }, encoding: 'utf8' });
  assert.notEqual(result.status, 0); assert.match(result.stderr, /unmanaged/);
  assert.equal(await readFile(join(skill, 'SKILL.md'), 'utf8'), 'owner modified instructions');
});

for (const mode of ['owned', 'foreign', 'nested-link']) test(`skills-lock physical QuaZonai directory ownership (${mode})`, async t => {
  const home = await mkdtemp(join(scratch, 'qz-skill-lock-')); t.after(() => rm(home, { recursive: true, force: true }));
  const matching = mode !== 'foreign';
  const skill = join(home, '.codex/skills/quazonai'), source = join(home, 'release/skill');
  for (const path of [skill, source, join(home, '.agents')]) await mkdir(path, { recursive: true });
  const lock = JSON.stringify({ version: 3, skills: { quazonai: { source: matching ? 'zhengui666/QuaZonai' : 'someone/Other', sourceUrl: 'https://github.com/zhengui666/QuaZonai.git', skillPath: '.agents/skills/quazonai/SKILL.md', ref: 'dev' }, other: { source: 'owner/private' } } });
  await writeFile(join(home, '.agents/.skill-lock.json'), lock);
  await writeFile(join(skill, 'SKILL.md'), 'previous managed skill'); await writeFile(join(skill, 'owner-notes.txt'), 'preserve owner file');
  await writeFile(join(home, 'credentials.json'), 'untouched authentication');
  const original = await readFile(new URL('../.agents/skills/quazonai/SKILL.md', import.meta.url), 'utf8');
  await writeFile(join(source, 'SKILL.md'), original);
  await mkdir(join(source, 'references')); await writeFile(join(source, 'references/accounts.md'), await readFile(new URL('../.agents/skills/quazonai/references/accounts.md', import.meta.url)));
  await writeFile(join(home, 'accounts.md'), 'unrelated outside data');
  if (mode === 'nested-link') await symlink(home, join(skill, 'references'));
  const release = { version: 'v3.0.0-dev.20261112000000.1', revision: 'a'.repeat(40) };
  await writeFile(join(source, 'release.json'), JSON.stringify(release));
  // Execute the original installer functions, not a second implementation.
  const script = await readFile(installer, 'utf8'); const functions = script.slice(script.indexOf('json_read() {'), script.indexOf('fetch() {'));
  const result = spawnSync('bash', ['-c', 'set -euo pipefail; fail() { echo "$*" >&2; exit 1; };\n' + functions + '\ninstall_skill "$1" "$2"', 'skill-install-test', source, skill], { encoding: 'utf8', env: { ...process.env, HOME: home } });
  if (matching && mode !== 'nested-link') {
    assert.equal(result.status, 0, result.stderr);
    assert.equal(await readFile(join(skill, 'SKILL.md'), 'utf8'), original);
    assert.deepEqual(JSON.parse(await readFile(join(skill, 'release.json'), 'utf8')), release);
  } else {
    assert.notEqual(result.status, 0); assert.match(result.stderr, /unmanaged|symlink/);
    assert.equal(await readFile(join(skill, 'SKILL.md'), 'utf8'), 'previous managed skill');
  }
  assert.equal(await readFile(join(home, 'accounts.md'), 'utf8'), 'unrelated outside data');
  assert.equal(await readFile(join(skill, 'owner-notes.txt'), 'utf8'), 'preserve owner file');
  assert.equal(await readFile(join(home, 'credentials.json'), 'utf8'), 'untouched authentication');
  assert.equal(await readFile(join(home, '.agents/.skill-lock.json'), 'utf8'), lock);
});
