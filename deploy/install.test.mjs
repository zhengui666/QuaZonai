import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmod, mkdir, mkdtemp, readFile, readdir, realpath, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const bundle = fileURLToPath(new URL('./docker/', import.meta.url));
const guide = await readFile(join(bundle, 'README.md'), 'utf8');
const readme = await readFile(new URL('../README.md', import.meta.url), 'utf8');
const operations = await readFile(new URL('../.opensdlc/operations.md', import.meta.url), 'utf8');
const documents = [readme, guide, operations];

function succeeds(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: 'utf8', timeout: 10_000, ...options });
  assert.ifError(result.error);
  assert.equal(result.signal, null, result.stderr);
  assert.equal(result.status, 0, result.stderr);
  return result.stdout;
}

test('documented deployment and maintenance commands have valid Bash syntax', () => {
  for (const text of documents) {
    const blocks = [...text.matchAll(/```sh\n([\s\S]*?)\n```/g)];
    assert.ok(blocks.length > 0, 'The guide must contain executable examples');
    for (const [, script] of blocks) succeeds('bash', ['-n'], { input: script });
  }
});

test('embedded Python maintenance examples compile without executing operations', () => {
  const scripts = documents.flatMap(text => [...text.matchAll(/python3[^\n]*<<'PY'\n([\s\S]*?)\nPY/g)]);
  assert.ok(scripts.length > 0, 'Expected revision lookup and maintenance examples');
  for (const [, script] of scripts) {
    succeeds('python3', ['-B', '-c', 'import sys; compile(sys.stdin.read(), "<documented-python>", "exec")'], { input: script });
  }
});

test('documented deployment entrypoints expose native help without creating an installation', async t => {
  const root = await mkdtemp(join(tmpdir(), 'quazonai-guide-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const options = {
    cwd: root,
    env: { ...process.env, HOME: root, XDG_CONFIG_HOME: join(root, 'config'), PYTHONDONTWRITEBYTECODE: '1' },
  };
  // Only native --help is executed. Docker, login, migration and the guide's
  // privileged commands are never invoked; real lifecycle tests live in smoke.py.
  for (const script of ['deploy.sh', 'update.sh', 'codex-login.sh', 'codex-update.sh', 'runtime.sh']) {
    assert.ok(guide.includes(script), `Missing documented entrypoint: ${script}`);
    const help = succeeds('bash', [join(bundle, script), '--help'], options);
    assert.match(help, /usage:/i);
    assert.match(help, /--directory/);
  }
  for (const operation of ['apply-update', 'status']) {
    assert.ok(guide.includes(`manage.py ${operation}`));
    const help = succeeds('python3', ['-B', join(bundle, 'manage.py'), operation, '--help'], options);
    assert.match(help, /usage:/i);
  }
  assert.deepEqual(await readdir(root), [], 'Help must not initialize state or credentials');
});

test('deployment routes use prebuilt images without checkout or local image production', () => {
  for (const document of documents) {
    for (const [, block] of document.matchAll(/```sh\n([\s\S]*?)\n```/g)) {
      assert.doesNotMatch(block, /\bgit\s+clone\b|\bcargo\s+(?:build|run|install)\b|\bdocker\s+(?:build|buildx|image\s+(?:build|save|load))\b|\btarget\/release\/runtime\b/);
    }
  }
  assert.match(guide, /GHCR/);
  assert.match(guide, /runtime\.sh/);
  assert.doesNotMatch(guide, /project\.md#runtime-image-build/);
});

async function releaseFixture(t, { system = 'Linux', architecture = 'x86_64', noPython = false } = {}) {
  const root = await realpath(await mkdtemp(join(tmpdir(), 'quazonai-release-')));
  t.after(() => rm(root, { recursive: true, force: true }));
  const home = join(root, 'home with spaces');
  const assets = join(root, 'assets');
  const commands = join(root, 'commands');
  const source = join(root, 'source');
  for (const path of [home, assets, commands, source]) await mkdir(path);
  const version = 'v2.0.0-dev.20260928000101';
  const script = join(root, 'install.sh');
  await writeFile(script, (await readFile(new URL('./install.sh', import.meta.url), 'utf8')).replaceAll('@QUAZONAI_VERSION@', version));
  const stub = async (name, contents) => {
    const file = join(commands, name);
    await writeFile(file, '#!/usr/bin/env bash\nset -euo pipefail\n' + contents);
    await chmod(file, 0o755);
  };
  await stub('curl', `
printf 'curl %s\\n' "$*" >> "$TEST_COMMAND_LOG"
while [ "$1" != --output ]; do shift; done
output=$2
url=$3
[[ "$url" == "https://github.com/zhengui666/QuaZonai/releases/download/$TEST_VERSION/"* ]]
name=$(basename "$url")
cp "$TEST_ASSETS/$name" "$output"
`);
  await stub('uname', 'if [ "$1" = -s ]; then echo "$TEST_SYSTEM"; else echo "$TEST_ARCH"; fi\n');
  await stub('python3', `
printf 'python3 %s\\n' "$*" >> "$TEST_COMMAND_LOG"
[ "$TEST_NO_PYTHON" = 0 ] || exit 90
exec "$TEST_REAL_PYTHON" "$@"
`);
  for (const name of ['cargo', 'rustc', 'npm', 'docker', 'git', 'make', 'cmake', 'apt', 'brew']) {
    await stub(name, `printf 'FORBIDDEN %s\\n' "$0 $*" >> "$TEST_COMMAND_LOG"; exit 91\n`);
  }
  const binary = '#!/bin/sh\n[ "$1" = --version ] && printf "quazonai fixture\\n"\n';
  await writeFile(join(source, 'quazonai'), binary, { mode: 0o755 });
  for (const name of ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md']) await writeFile(join(source, name), name + '\n');
  const platform = system === 'Linux' ? 'linux-x86_64' : architecture === 'arm64' ? 'macos-aarch64' : 'macos-x86_64';
  const archive = `quazonai-cli-${platform}.tar.gz`;
  const packCli = () => succeeds('tar', ['-czf', join(assets, archive), '-C', source, 'quazonai', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md']);
  packCli();
  await writeFile(join(source, 'release.json'), JSON.stringify({ version }));
  // Dispatch fixture: the real manager's lifecycle/recovery checks are exercised by
  // deploy/docker/smoke.py; this records only which target entrypoint is selected.
  await writeFile(join(source, 'manage.py'), `import json, os, pathlib, sys
pathlib.Path(os.environ['TEST_MANAGER_LOG']).write_text(json.dumps(sys.argv[1:]))
sys.exit(int(os.environ.get('TEST_MANAGER_FAILURE', '0')))
`);
  const packStack = () => succeeds('tar', ['-czf', join(assets, 'quazonai-deploy.tar.gz'), '-C', source, 'manage.py', 'release.json']);
  packStack();
  const checksums = async () => {
    const files = [archive, 'quazonai-deploy.tar.gz'];
    const entries = await Promise.all(files.map(async name => `${createHash('sha256').update(await readFile(join(assets, name))).digest('hex')}  ${name}\n`));
    await writeFile(join(assets, 'SHA256SUMS'), entries.join(''));
  };
  await checksums();
  const env = {
    ...process.env, HOME: home, PATH: commands + ':' + process.env.PATH,
    TEST_VERSION: version, TEST_ASSETS: assets, TEST_SYSTEM: system, TEST_ARCH: architecture,
    TEST_NO_PYTHON: noPython ? '1' : '0', TEST_REAL_PYTHON: succeeds('python3', ['-c', 'import sys; print(sys.executable)']).trim(),
    TEST_COMMAND_LOG: join(root, 'commands.log'), TEST_MANAGER_LOG: join(root, 'manager.json'),
  };
  const run = (args = [], extraEnv = {}) => spawnSync('bash', [script, ...args], { encoding: 'utf8', timeout: 20_000, env: { ...env, ...extraEnv } });
  const destination = join(home, '.local/bin/quazonai');
  const directory = join(home, 'custom stack');
  return { root, home, source, version, assets, archive, binary, destination, directory, env, run, checksums, packCli, packStack };
}

function installed(result) {
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Installed QuaZonai CLI/);
}

test('prebuilt CLI installers verify archives and atomically update without Python or builds', async t => {
  for (const [system, architecture] of [['Linux', 'x86_64'], ['Darwin', 'x86_64'], ['Darwin', 'arm64']]) {
    await t.test(`${system}/${architecture}`, async t => {
      const f = await releaseFixture(t, { system, architecture, noPython: true });
      const args = system === 'Linux' ? ['--cli-only'] : [];
      installed(f.run(args));
      assert.equal(await readFile(f.destination, 'utf8'), f.binary);
      assert.equal(await readFile(join(f.home, '.local/share/quazonai-cli/licenses', f.version, 'LICENSE'), 'utf8'), 'LICENSE\n');
      await writeFile(join(f.source, 'quazonai'), f.binary.replace('fixture', 'updated'));
      f.packCli();
      await f.checksums();
      installed(f.run(args));
      const previous = await readFile(f.destination);
      await writeFile(join(f.assets, f.archive), 'truncated download');
      const failure = f.run(args);
      assert.notEqual(failure.status, 0);
      assert.match(failure.stderr, /Checksum mismatch/);
      assert.deepEqual(await readFile(f.destination), previous);
      assert.doesNotMatch(await readFile(f.env.TEST_COMMAND_LOG, 'utf8'), /python3|FORBIDDEN/);
      assert.deepEqual(await readdir(join(f.home, '.local/bin')), ['quazonai']);
    });
  }
});

test('Linux installer dispatches new, legacy, pending install and pending update to the target manager', async t => {
  const f = await releaseFixture(t);
  const args = ['--directory', f.directory, '--port', '18080', '--database-port', '55433'];
  const dispatch = async expected => {
    installed(f.run(args));
    assert.deepEqual(JSON.parse(await readFile(f.env.TEST_MANAGER_LOG, 'utf8')), [expected, '--directory', f.directory, '--port', '18080', '--database-port', '55433']);
  };
  await dispatch('deploy');
  await mkdir(f.directory);
  await writeFile(join(f.directory, 'installation.json'), '{"schema_version":1}');
  await dispatch('deploy'); // installation.json persisted immediately before a crash
  await writeFile(join(f.directory, 'pending.json'), '{"operation":"install","phase":"starting"}');
  await dispatch('deploy');
  await symlink(join(f.directory, 'old-release'), join(f.directory, 'current'));
  await rm(join(f.directory, 'pending.json'));
  await dispatch('apply-update'); // target installer handles schema_version=1 migration
  await writeFile(join(f.directory, 'pending.json'), '{"operation":"update","phase":"starting"}');
  await dispatch('apply-update');
  const previous = await readFile(f.destination);
  await writeFile(join(f.source, 'quazonai'), f.binary.replace('fixture', 'candidate'));
  f.packCli();
  await f.checksums();
  assert.notEqual(f.run(args, { TEST_MANAGER_FAILURE: '42' }).status, 0);
  assert.deepEqual(await readFile(f.destination), previous);
  assert.doesNotMatch(await readFile(f.env.TEST_COMMAND_LOG, 'utf8'), /FORBIDDEN/);
});

test('default Linux installation reaches the manager without optional stack arguments', async t => {
  const f = await releaseFixture(t);
  installed(f.run());
  assert.deepEqual(JSON.parse(await readFile(f.env.TEST_MANAGER_LOG, 'utf8')), [
    'deploy', '--directory', join(f.home, '.local/share/quazonai'),
  ]);
});

test('install rejects invalid tags, wrong target bundles and broken executables before replacement', async t => {
  const f = await releaseFixture(t);
  installed(f.run(['--cli-only']));
  const previous = await readFile(f.destination);
  for (const version of ['../main', 'v02.0.0', 'v2.0.0-dev.01', 'v2.0.0+metadata']) {
    assert.notEqual(f.run(['--cli-only', '--version', version]).status, 0);
  }
  await writeFile(join(f.source, 'release.json'), '{"version":"v9.0.0"}');
  f.packStack();
  await f.checksums();
  const wrong = f.run(['--directory', f.directory]);
  assert.notEqual(wrong.status, 0, wrong.stderr);
  assert.match(wrong.stderr, /does not match the requested tag/);
  await writeFile(join(f.source, 'quazonai'), '#!/bin/sh\nexit 1\n');
  f.packCli();
  await f.checksums();
  assert.notEqual(f.run(['--cli-only']).status, 0);
  assert.deepEqual(await readFile(f.destination), previous);
});

test('CLI-only accepts a custom binary directory and help is side effect free', async t => {
  const f = await releaseFixture(t, { noPython: true });
  const help = f.run(['--help']);
  assert.equal(help.status, 0, help.stderr);
  assert.match(help.stdout, /--bin-dir/);
  assert.deepEqual(await readdir(f.home), []);
  const bin = join(f.home, 'custom bin');
  installed(f.run(['--cli-only', '--bin-dir', bin]));
  assert.equal(await readFile(join(bin, 'quazonai'), 'utf8'), f.binary);
});
