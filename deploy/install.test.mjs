import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
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
const bundleFiles = ['manage.sh', 'json.awk', 'codex.sh', 'deploy.sh', 'update.sh', 'compose.yaml',
  'release.json', 'README.md', 'codex-update.sh', 'codex-login.sh', 'runtime.sh', 'codex.apparmor', '.env.example'];
const releaseAssets = ['install.sh', 'install.ps1', 'release.json', 'quazonai-deploy.tar.gz', 'README.md',
  'quazonai-cli-linux-x86_64.tar.gz', 'quazonai-cli-macos-x86_64.tar.gz', 'quazonai-cli-macos-aarch64.tar.gz',
  'quazonai-cli-windows-x86_64.zip', ...['application', 'runtime', 'codex', 'database'].map(x => `quazonai-image-${x}.tar.gz`)];

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

test('documented deployment entrypoints expose shell help without creating an installation', async t => {
  const root = await mkdtemp(join(tmpdir(), 'quazonai-guide-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const options = { cwd: root, env: { ...process.env, HOME: root, XDG_CONFIG_HOME: join(root, 'config') } };
  for (const script of ['deploy.sh', 'update.sh', 'codex-login.sh', 'codex-update.sh', 'runtime.sh']) {
    assert.ok(guide.includes(script), `Missing documented entrypoint: ${script}`);
    const help = succeeds('bash', [join(bundle, script), '--help'], options);
    assert.match(help, /usage:/i);
    assert.match(help, /--directory/);
  }
  for (const operation of ['apply-update', 'status']) {
    assert.ok(guide.includes(`manage.sh ${operation}`));
    const help = succeeds('bash', [join(bundle, 'manage.sh'), operation, '--help'], options);
    assert.match(help, /usage:/i);
  }
  assert.deepEqual(await readdir(root), [], 'Help must not initialize state or credentials');
});

test('deployment routes use prebuilt images and shell without local image production', () => {
  for (const document of documents) {
    for (const [, block] of document.matchAll(/```sh\n([\s\S]*?)\n```/g)) {
      assert.doesNotMatch(block, /\bgit\s+clone\b|\bcargo\s+(?:build|run|install)\b|\bdocker\s+(?:build|buildx|image\s+(?:build|save|load))\b|\btarget\/release\/runtime\b/);
    }
  }
  assert.doesNotMatch(readme + guide, /\bpython3\b|manage\.py|codex\.py/);
  assert.match(guide, /GHCR/);
  assert.match(guide, /runtime\.sh/);
  assert.doesNotMatch(guide, /project\.md#runtime-image-build/);
});

async function releaseFixture(t, { system = 'Linux', architecture = 'x86_64', bootstrap = false } = {}) {
  const root = await realpath(await mkdtemp(join(tmpdir(), 'quazonai-release-')));
  t.after(() => rm(root, { recursive: true, force: true }));
  const home = join(root, 'home with spaces');
  const assets = join(root, 'assets');
  const commands = join(root, 'commands');
  const source = join(root, 'source');
  for (const path of [home, assets, commands, source]) await mkdir(path);
  const version = 'v2.0.0-dev.20260928123456.100';
  const script = join(root, 'install.sh');
  const template = await readFile(new URL('./install.sh', import.meta.url), 'utf8');
  await writeFile(script, bootstrap ? template : template.replaceAll('@QUAZONAI_VERSION@', version));
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
if [[ "$url" == https://api.github.com/repos/zhengui666/QuaZonai/releases\\?per_page=100* ]]; then
  if [[ "$url" == *\\&page=* ]]; then
    page="\u0024{url##*page=}"
    cp "$TEST_ASSETS/catalog-page$page.json" "$output"
  else
    cp "$TEST_ASSETS/catalog.json" "$output"
  fi
elif [[ "$url" == "https://raw.githubusercontent.com/zhengui666/QuaZonai/dev/deploy/install.sh" ]]; then
  cp "$TEST_BOOTSTRAP" "$output"
else
  [[ "$url" == "https://github.com/zhengui666/QuaZonai/releases/download/$TEST_VERSION/"* ]]
  name=$(basename "$url")
  cp "$TEST_ASSETS/$name" "$output"
fi
[ "\u0024{TEST_CURL_FAILURE:-0}" = 0 ] || exit 22
`);
  await stub('uname', 'if [ "$1" = -s ]; then echo "$TEST_SYSTEM"; else echo "$TEST_ARCH"; fi\n');
  for (const name of ['python', 'python3', 'node', 'jq', 'sha256sum', 'shasum', 'cargo', 'rustc', 'npm', 'docker', 'git', 'make', 'cmake', 'apt', 'apt-get', 'brew', 'sudo']) {
    await stub(name, `printf 'FORBIDDEN %s\\n' "$0 $*" >> "$TEST_COMMAND_LOG"; exit 91\n`);
  }
  const binary = `#!/bin/sh\n[ "$1" = --version ] && printf "quazonai ${version}\\n"\n`;
  await writeFile(join(source, 'quazonai'), binary, { mode: 0o755 });
  for (const name of ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md']) await writeFile(join(source, name), name + '\n');
  const platform = system === 'Linux' ? 'linux-x86_64' : architecture === 'arm64' ? 'macos-aarch64' : 'macos-x86_64';
  const archive = `quazonai-cli-${platform}.tar.gz`;
  const packCli = (members = ['quazonai', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md']) => succeeds('tar', ['-czf', join(assets, archive), '-C', source, ...members]);
  packCli();
  for (const name of bundleFiles) await writeFile(join(source, name), `fixture ${name}\n`);
  await writeFile(join(source, 'release.json'), JSON.stringify({ version }));
  // The manager is a shell dispatch fixture, not a simulated Docker deployment.
  await writeFile(join(source, 'manage.sh'), `#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' "$@" > "$TEST_MANAGER_LOG"\nexit "\u0024{TEST_MANAGER_FAILURE:-0}"\n`);
  const packStack = (members = bundleFiles) => succeeds('tar', ['-czf', join(assets, 'quazonai-deploy.tar.gz'), '-C', source, ...members]);
  packStack();
  const release = (tag = version, changes = {}) => ({ tag_name: tag, draft: false, assets: releaseAssets.map(name => ({ name, state: 'uploaded', size: 1 })), ...changes });
  const catalog = async value => writeFile(join(assets, 'catalog.json'), typeof value === 'string' ? value : JSON.stringify(value));
  await catalog([release()]);
  const env = {
    ...process.env, HOME: home, PATH: commands + ':' + process.env.PATH,
    TEST_VERSION: version, TEST_ASSETS: assets, TEST_SYSTEM: system, TEST_ARCH: architecture,
    TEST_COMMAND_LOG: join(root, 'commands.log'), TEST_MANAGER_LOG: join(root, 'manager.args'), TEST_BOOTSTRAP: script,
  };
  const run = (args = [], extraEnv = {}) => spawnSync('bash', [script, ...args], { encoding: 'utf8', timeout: 20_000, env: { ...env, ...extraEnv } });
  const destination = join(home, '.local/bin/quazonai');
  const directory = join(home, 'custom stack');
  return { root, home, source, script, version, assets, archive, binary, destination, directory, env, run, packCli, packStack, release, catalog };
}

function installed(result) {
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Installed QuaZonai CLI/);
}

async function managerArgs(f) { return (await readFile(f.env.TEST_MANAGER_LOG, 'utf8')).trimEnd().split('\n'); }

test('prebuilt CLI installs atomically on supported platforms without Python, hashes or builds', async t => {
  for (const [system, architecture] of [['Linux', 'x86_64'], ['Darwin', 'x86_64'], ['Darwin', 'arm64']]) {
    await t.test(`${system}/${architecture}`, async t => {
      const f = await releaseFixture(t, { system, architecture });
      const args = system === 'Linux' ? ['--cli-only'] : [];
      installed(f.run(args));
      assert.equal(await readFile(f.destination, 'utf8'), f.binary);
      assert.equal(await readFile(join(f.home, '.local/share/quazonai-cli/licenses', f.version, 'LICENSE'), 'utf8'), 'LICENSE\n');
      await writeFile(join(f.source, 'quazonai'), f.binary + '# updated\n');
      f.packCli();
      installed(f.run(args));
      const previous = await readFile(f.destination);
      await writeFile(join(f.assets, f.archive), 'truncated download');
      const failure = f.run(args);
      assert.notEqual(failure.status, 0);
      assert.match(failure.stderr, /Invalid or incomplete archive/);
      assert.deepEqual(await readFile(f.destination), previous);
      assert.doesNotMatch(await readFile(f.env.TEST_COMMAND_LOG, 'utf8'), /FORBIDDEN|SHA256SUMS|api.github.com/);
      assert.deepEqual(await readdir(join(f.home, '.local/bin')), ['quazonai']);
    });
  }
});

test('Linux installer dispatches new, legacy and interrupted installations to NEW shell manager', async t => {
  const f = await releaseFixture(t);
  const args = ['--directory', f.directory, '--port', '18080', '--database-port', '55433'];
  const dispatch = async expected => {
    installed(f.run(args));
    assert.deepEqual(await managerArgs(f), [expected, '--directory', f.directory, '--port', '18080', '--database-port', '55433']);
  };
  await dispatch('deploy');
  await mkdir(f.directory);
  const state = '{"schema_version":1,"identity":"original","unknown":"preserved"}';
  await writeFile(join(f.directory, 'installation.json'), state);
  await writeFile(join(f.directory, '.env'), 'CODEX_VERSION=1.2.3\n');
  await writeFile(join(f.directory, 'master.key'), 'keep-original-key');
  await dispatch('deploy');
  await writeFile(join(f.directory, 'pending.json'), '{"operation":"install","phase":"starting"}');
  await dispatch('deploy');
  await symlink(join(f.directory, 'old-release'), join(f.directory, 'current'));
  await mkdir(join(f.directory, 'old-release', 'deployment'), { recursive: true });
  await writeFile(join(f.directory, 'old-release', 'deployment', 'update.sh'), '#!/bin/sh\nexit 99\n');
  await rm(join(f.directory, 'pending.json'));
  await dispatch('apply-update');
  await writeFile(join(f.directory, 'pending.json'), '{"operation":"update","phase":"starting"}');
  await dispatch('apply-update');
  const previous = await readFile(f.destination);
  await writeFile(join(f.source, 'quazonai'), f.binary + '# candidate\n');
  f.packCli();
  assert.notEqual(f.run(args, { TEST_MANAGER_FAILURE: '42' }).status, 0);
  assert.deepEqual(await readFile(f.destination), previous);
  assert.equal(await readFile(join(f.directory, 'installation.json'), 'utf8'), state);
  assert.equal(await readFile(join(f.directory, '.env'), 'utf8'), 'CODEX_VERSION=1.2.3\n');
  assert.equal(await readFile(join(f.directory, 'master.key'), 'utf8'), 'keep-original-key');
  assert.doesNotMatch(await readFile(f.env.TEST_COMMAND_LOG, 'utf8'), /FORBIDDEN/);
});

test('default Linux installation reaches shell manager without optional stack arguments', async t => {
  const f = await releaseFixture(t);
  installed(f.run());
  assert.deepEqual(await managerArgs(f), ['deploy', '--directory', join(f.home, '.local/share/quazonai')]);
});

test('installer rejects invalid tags, wrong manifests and wrong native versions without replacement', async t => {
  const f = await releaseFixture(t);
  installed(f.run(['--cli-only']));
  const previous = await readFile(f.destination);
  for (const version of ['../main', 'v02.0.0', 'v2.0.0-dev.01', 'v2.0.0+metadata', 'v2.0.0\n']) {
    assert.notEqual(f.run(['--cli-only', '--version', version]).status, 0);
  }
  for (const manifest of ['{"version":"v9.0.0"}', '{"version":"' + f.version + '","version":"' + f.version + '"}', '{"version":"' + f.version + '"}junk']) {
    await writeFile(join(f.source, 'release.json'), manifest); f.packStack();
    assert.notEqual(f.run(['--directory', f.directory]).status, 0);
  }
  await writeFile(join(f.source, 'quazonai'), '#!/bin/sh\necho "quazonai v9.0.0"\n'); f.packCli();
  assert.notEqual(f.run(['--cli-only']).status, 0);
  assert.deepEqual(await readFile(f.destination), previous);
});

test('installer refuses symlinks, duplicate members and path traversal in archives', async t => {
  const f = await releaseFixture(t);
  installed(f.run(['--cli-only']));
  const previous = await readFile(f.destination);
  for (const members of [ ['quazonai', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md', 'LICENSE'], ['../install.sh', 'quazonai', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md'] ]) {
    f.packCli(members); assert.notEqual(f.run(['--cli-only']).status, 0);
  }
  await rm(join(f.source, 'LICENSE')); await symlink(join(f.source, 'NOTICE'), join(f.source, 'LICENSE')); f.packCli();
  assert.notEqual(f.run(['--cli-only']).status, 0);
  assert.deepEqual(await readFile(f.destination), previous);
});

test('download failure after partial bytes never executes or replaces anything', async t => {
  const f = await releaseFixture(t);
  const failure = f.run(['--cli-only'], { TEST_CURL_FAILURE: '22' });
  assert.notEqual(failure.status, 0);
  assert.deepEqual(await readdir(f.home), []);
  const command = readme.split('\n').find(line => line.startsWith("sh -c '") && line.includes('curl'));
  assert.ok(command, 'README must download to a temporary file and only execute after curl success');
  await writeFile(f.script, `#!/bin/sh\nprintf unsafe > "$HOME/partial-executed"\n`);
  const result = spawnSync('sh', ['-c', command], { encoding: 'utf8', env: { ...f.env, TMPDIR: f.home, TEST_CURL_FAILURE: '22' } });
  assert.notEqual(result.status, 0);
  assert.deepEqual(await readdir(f.home), [], 'Failed bootstrap cleans up the partial script');
});

test('documented one-line bootstrap forwards explicit version and custom paths then cleans up', async t => {
  const f = await releaseFixture(t, { bootstrap: true });
  const temporary = join(f.root, 'downloads'); await mkdir(temporary);
  const command = readme.split('\n').find(line => line.startsWith("sh -c '") && line.includes('curl'));
  const result = spawnSync('sh', ['-c', command + ' --cli-only --version "$TEST_VERSION" --bin-dir "$HOME/custom bin"'], {
    encoding: 'utf8', env: { ...f.env, TMPDIR: temporary },
  });
  installed(result);
  assert.equal(await readFile(join(f.home, 'custom bin/quazonai'), 'utf8'), f.binary);
  assert.deepEqual(await readdir(temporary), []);
  assert.doesNotMatch(await readFile(f.env.TEST_COMMAND_LOG, 'utf8'), /api.github.com|FORBIDDEN/);
});

test('latest bootstrap chooses newest complete dev release independently of publication order', async t => {
  const f = await releaseFixture(t, { bootstrap: true });
  const catalog = [
    f.release('v2.0.0-dev.20260927123456.999'),
    f.release('v2.0.0-dev.20260929123456.101', { draft: true }),
    f.release('v2.0.0-dev.20260929123456.102', { assets: [] }),
    f.release('v2.0.0-dev.20260929123456.103', { assets: releaseAssets.map(name => ({ name, state: 'new', size: 0 })) }),
    f.release('v2.0.0-dev.999'), f.release('v2.0.0'), f.release(),
    f.release('v2.0.0-dev.20260928123456.99'),
  ];
  for (const value of [catalog, [...catalog].reverse()]) { await f.catalog(value); installed(f.run(['--cli-only'])); }
  assert.doesNotMatch(await readFile(f.env.TEST_COMMAND_LOG, 'utf8'), /FORBIDDEN/);
});

test('bootstrap rejects malformed catalogs and parses escaped nested JSON without code evaluation', async t => {
  const f = await releaseFixture(t, { bootstrap: true });
  for (const value of ['{}', '[]', '[{"tag_name":', JSON.stringify([f.release()]) + 'trailing',
    JSON.stringify([f.release()]).replace('"draft":false', '"draft":false,"draft":false'),
    JSON.stringify([f.release()]).replace('"draft":false', '"draft":false,"dr\\u0061ft":false'),
    JSON.stringify([f.release()]).replace('"draft":false', '"draft":01'),
    JSON.stringify([f.release()]).replace('"draft":false', '"draft":"\\uD800"')]) {
    await f.catalog(value); assert.notEqual(f.run(['--cli-only']).status, 0, value);
  }
  const value = [f.release(undefined, { body: '"tag_name":"v9.0.0-dev.20269999123456.100" and $(false)', nested: { assets: [], draft: true }, unicode: '跨平台 🦊' })];
  await f.catalog(JSON.stringify(value).replace('"tag_name"', '"tag_\\u006eame"'));
  installed(f.run(['--cli-only']));
});

test('explicit versions skip release discovery even from raw bootstrap', async t => {
  const f = await releaseFixture(t, { bootstrap: true });
  await f.catalog('invalid JSON'); installed(f.run(['--cli-only', '--version', f.version]));
  assert.doesNotMatch(await readFile(f.env.TEST_COMMAND_LOG, 'utf8'), /api.github.com/);
});

test('malformed or unrelated pending state is preserved and rejected before manager runs', async t => {
  const f = await releaseFixture(t);
  await mkdir(f.directory);
  await writeFile(join(f.directory, 'installation.json'), '{}');
  for (const pending of ['{"operation":"restore"}', '{"operation":"update","operation":"install"}', 'null']) {
    await writeFile(join(f.directory, 'pending.json'), pending);
    assert.notEqual(f.run(['--directory', f.directory]).status, 0);
    assert.equal(await readFile(join(f.directory, 'pending.json'), 'utf8'), pending);
  }
  await assert.rejects(readFile(f.env.TEST_MANAGER_LOG));
});

test('CLI-only accepts a custom bin directory and help is side-effect-free', async t => {
  const f = await releaseFixture(t);
  const help = f.run(['--help']);
  assert.equal(help.status, 0, help.stderr); assert.match(help.stdout, /--bin-dir/);
  assert.deepEqual(await readdir(f.home), []);
  const bin = join(f.home, 'custom bin'); installed(f.run(['--cli-only', '--bin-dir', bin]));
  assert.equal(await readFile(join(bin, 'quazonai'), 'utf8'), f.binary);
});


test('latest bootstrap scans additional pages before choosing the newest complete tag', async t => {
  const f = await releaseFixture(t, { bootstrap: true });
  await f.catalog(Array.from({ length: 100 }, (_, index) => f.release(`v2.0.0-dev.20260927123456.${index + 1}`)));
  await writeFile(join(f.assets, 'catalog-page2.json'), JSON.stringify([f.release()]));
  installed(f.run(['--cli-only']));
  assert.match(await readFile(f.env.TEST_COMMAND_LOG, 'utf8'), /&page=2/);
  await writeFile(join(f.assets, 'catalog-page2.json'), 'partial catalog');
  const previous = await readFile(f.destination);
  assert.notEqual(f.run(['--cli-only']).status, 0);
  assert.deepEqual(await readFile(f.destination), previous);
});

test('bootstrap rejects malformed raw UTF-8 in catalog strings', async t => {
  const f = await releaseFixture(t, { bootstrap: true });
  const template = JSON.stringify([f.release(undefined, { body: 'INVALID_BYTES' })]);
  const [before, after] = template.split('INVALID_BYTES');
  for (const bytes of [[0x80], [0xc0, 0xaf], [0xe0, 0x80, 0x80], [0xed, 0xa0, 0x80], [0xf4, 0x90, 0x80, 0x80], [0xf0, 0x9f]]) {
    await writeFile(join(f.assets, 'catalog.json'), Buffer.concat([Buffer.from(before), Buffer.from(bytes), Buffer.from(after)]));
    assert.notEqual(f.run(['--cli-only']).status, 0, `accepted ${bytes}`);
  }
  assert.deepEqual(await readdir(f.home), []);
});

test('quoted current-user tilde paths are expanded before inspecting legacy state', async t => {
  const f = await releaseFixture(t);
  await mkdir(f.directory);
  await writeFile(join(f.directory, 'installation.json'), '{"schema_version":1}');
  await symlink(join(f.directory, 'old-release'), join(f.directory, 'current'));
  installed(f.run(['--directory', '~/custom stack', '--bin-dir', '~/custom bin']));
  assert.deepEqual(await managerArgs(f), ['apply-update', '--directory', f.directory]);
  assert.equal(await readFile(join(f.home, 'custom bin/quazonai'), 'utf8'), f.binary);
});

test('simulated old Bash rejects Linux stack before downloads but permits CLI-only', async t => {
  const f = await releaseFixture(t);
  await writeFile(f.script, (await readFile(f.script, 'utf8')).replaceAll('"${BASH_VERSINFO[0]}"', '"3"'));
  const failed = f.run();
  assert.notEqual(failed.status, 0);
  assert.match(failed.stderr, /requires Bash 4\.4/);
  await assert.rejects(readFile(f.env.TEST_COMMAND_LOG));
  assert.deepEqual(await readdir(f.home), []);
  installed(f.run(['--cli-only']));
});
