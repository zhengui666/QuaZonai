import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, readFile, readdir, rm, symlink, stat } from 'node:fs/promises';
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
  for (const tool of ['bash', 'env', 'tar', 'gzip', 'awk', 'sort', 'cmp', 'mktemp', 'chmod', 'cp', 'mv', 'mkdir', 'rm', 'uname', 'dirname']) {
    const path = run('which', [tool]).trim(); await symlink(path, join(tools, tool));
  }
  await writeFile(join(tools, 'curl'), '#!/bin/sh\nwhile [ "$#" -gt 0 ]; do if [ "$1" = --output ]; then shift; output=$1; fi; shift; done\nexec cp "$QZ_TEST_CLI_ARCHIVE" "$output"\n', { mode: 0o755 });
  const configDirectory = join(home, '.config', 'quazonai');
  await mkdir(configDirectory, { recursive: true, mode: 0o700 });
  const config = join(configDirectory, 'client.json');
  const original = '{"schema_version":1,"origin":"https://original.invalid","token":"synthetic-install-token","development_http":false,"ca_certificate":null}\n';
  await writeFile(config, original, { mode: 0o600 });
  const env = { ...process.env, HOME: home, XDG_CONFIG_HOME: join(home, '.config'), TMPDIR: root, PATH: tools };
  const version = process.env.QZ_TEST_CLI_VERSION;
  for (let i = 0; i < 2; i++) run('/bin/bash', [installer, '--cli-only', '--version', version, '--bin-dir', bin], env);
  assert.equal(run(join(bin, 'quazonai'), ['--version'], env).trim(), `quazonai ${version}`);
  assert.match(run(join(bin, 'quazonai'), ['--help'], env), /research/);
  assert.ok(JSON.parse(run(join(bin, 'quazonai'), ['schema', 'list'], env)).names.includes('ResearchStart'));
  assert.ok(JSON.parse(run(join(bin, 'quazonai'), ['schema', 'list', '--domain'], env)).names.length > 0);
  assert.equal(await readFile(config, 'utf8'), original);
  assert.equal((await stat(config)).mode & 0o777, 0o600);
  assert.equal((await stat(configDirectory)).mode & 0o777, 0o700);
  const distribution = join(home, '.local/share/quazonai-cli/releases', version, 'linux-x86_64');
  assert.equal(JSON.parse(await readFile(join(distribution, 'distribution.json'), 'utf8')).platform, 'linux-x86_64');
  assert.match(await readFile(join(distribution, 'runtime/LICENSE'), 'utf8'), /Node/);
  assert.match(await readFile(join(bin, 'quazonai'), 'utf8'), /quazonai-cli\/releases/);
});
