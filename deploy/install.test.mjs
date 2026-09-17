import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir, mkdtemp, readFile, readdir, readlink, rm, stat, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

// Execute the actual guide snippets with native Git/coreutils in disposable paths.
// Compilation is an explicit fixture; sudo executes as the current user. This
// verifies shell semantics, not a QZ build, privileged installation or deployment.
const guide = await readFile(new URL('../docs/user-guide.md', import.meta.url), 'utf8');
const section = guide.split('### 1. Build a reviewed revision\n')[1]?.split('### 2.')[0];
assert.ok(section, 'Release installation section must exist');
const blocks = [...section.matchAll(/```sh\n([\s\S]*?)\n```/g)].map(match => match[1]);
assert.equal(blocks.length, 2, 'Inspect changed build/install and first-selection snippets');
const [prepare, select] = blocks;
const hooks = `
umask 077
sudo() { "$@"; }
rustup() {
  printf 'rustup\\n' >> "$QZ_TEST_ROOT/build-invocations"
  if [ "$QZ_TEST_MODE" = build-failure ]; then return 9; fi
  mkdir -p target/release
  printf 'binary-fixture\\n' > target/release/server
}
npm() {
  printf 'npm\\n' >> "$QZ_TEST_ROOT/build-invocations"
  if [ "$3" = run ]; then
    mkdir -p apps/web/dist
    printf 'web-fixture\\n' > apps/web/dist/index.html
    case "$QZ_TEST_MODE" in
      tracked-after) printf 'changed\\n' >> fixture.txt ;;
      untracked-after) printf 'untracked\\n' > extra.txt ;;
      head-after) git -c user.name=Fixture -c user.email=fixture@example.invalid commit --allow-empty -m moved ;;
    esac
  fi
}
`;

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'quazonai-install-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const cwd = join(root, 'checkout');
  await mkdir(cwd);
  const env = { ...process.env, HOME: root, LC_ALL: 'C', GIT_CONFIG_NOSYSTEM: '1',
    GIT_CONFIG_GLOBAL: '/dev/null', QZ_TEST_ROOT: root, QZ_TEST_MODE: 'clean' };
  const git = (...args) => execFileSync('git', args, { cwd, env, encoding: 'utf8', timeout: 10_000 }).trim();
  git('init', '-q');
  await writeFile(join(cwd, '.gitignore'), '/target/\n/apps/web/dist/\n');
  await writeFile(join(cwd, 'fixture.txt'), 'original\n');
  git('add', '.');
  git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture');
  const revision = git('rev-parse', 'HEAD');
  const prefix = join(root, 'opt', 'quazonai');
  const release = join(prefix, 'releases', revision);
  const current = join(prefix, 'current');
  const run = (script, mode = 'clean') => {
    const result = spawnSync('/bin/sh', ['-c', hooks + '\n' + script.replaceAll('/opt/quazonai', prefix)], {
      cwd, env: { ...env, QZ_TEST_MODE: mode, QZ_TEST_RELEASE: release }, encoding: 'utf8', timeout: 10_000,
    });
    assert.ifError(result.error);
    assert.equal(result.signal, null, result.stderr);
    return result;
  };
  return { root, cwd, prefix, release, current, git, run, revision };
}

function passed(result) { assert.equal(result.status, 0, result.stderr); }
function refused(result) { assert.notEqual(result.status, 0, 'Expected a failed command, not a successful partial install'); }

test('native counterexamples reproduce the original directory, link and dirty-revision defects', async t => {
  const f = await fixture(t);
  passed(f.run('mkdir "$QZ_TEST_ROOT/implicit-mode"'));
  assert.equal((await stat(join(f.root, 'implicit-mode'))).mode & 0o777, 0o700);
  await mkdir(f.prefix, { recursive: true });
  const old = join(f.prefix, 'old'); const next = join(f.prefix, 'next');
  await mkdir(old); await mkdir(next); await symlink(old, f.current);
  passed(f.run('ln -s /opt/quazonai/next /opt/quazonai/current'));
  assert.equal(await readlink(f.current), old);
  assert.equal(await readlink(join(old, 'next')), next);
  await writeFile(join(f.cwd, 'fixture.txt'), 'modified\n');
  assert.equal(f.git('rev-parse', 'HEAD'), f.revision);
  assert.notEqual(f.git('status', '--porcelain=v1', '--untracked-files=all'), '');
});

test('the documented clean install is traversable under umask 077 and selects exactly its new release', async t => {
  const f = await fixture(t);
  passed(f.run(`${prepare}\n${select}`));
  for (const path of [f.prefix, join(f.prefix, 'releases'), f.release, join(f.release, 'bin'), join(f.release, 'web')]) {
    assert.equal((await stat(path)).mode & 0o777, 0o755, path);
  }
  assert.equal(await readlink(f.current), f.release);
  assert.equal(await readFile(join(f.release, 'bin', 'server'), 'utf8'), 'binary-fixture\n');
  assert.equal(await readFile(join(f.release, 'web', 'index.html'), 'utf8'), 'web-fixture\n');
  assert.equal((await stat(join(f.release, 'bin', 'server'))).mode & 0o777, 0o755);
  assert.equal(f.git('status', '--porcelain=v1', '--untracked-files=all'), '');
});

test('an existing release directory is refused rather than overwritten', async t => {
  const f = await fixture(t);
  passed(f.run(`${prepare}\n${select}`));
  await writeFile(join(f.release, 'bin', 'server'), 'existing-release\n');
  const before = await readdir(f.release);
  refused(f.run(prepare));
  assert.equal(await readFile(join(f.release, 'bin', 'server'), 'utf8'), 'existing-release\n');
  assert.deepEqual(await readdir(f.release), before);
  assert.equal(await readlink(f.current), f.release);
});

for (const kind of ['directory', 'file', 'symlink', 'dangling-symlink']) {
  test(`first selection refuses an existing ${kind} without changing it or nesting a link`, async t => {
    const f = await fixture(t);
    passed(f.run(prepare));
    const old = join(f.prefix, 'old-release');
    if (kind === 'directory') await mkdir(f.current);
    else if (kind === 'file') await writeFile(f.current, 'original-selection\n');
    else {
      if (kind === 'symlink') { await mkdir(old); await writeFile(join(old, 'sentinel'), 'unchanged\n'); }
      await symlink(old, f.current);
    }
    refused(f.run(`set -eu\nrelease="$QZ_TEST_RELEASE"\n${select}`));
    if (kind === 'directory') assert.deepEqual(await readdir(f.current), []);
    else if (kind === 'file') assert.equal(await readFile(f.current, 'utf8'), 'original-selection\n');
    else {
      assert.equal(await readlink(f.current), old);
      if (kind === 'symlink') {
        assert.deepEqual(await readdir(old), ['sentinel']);
        assert.equal(await readFile(join(old, 'sentinel'), 'utf8'), 'unchanged\n');
      } else assert.equal(existsSync(old), false);
    }
  });
}

for (const kind of ['tracked', 'staged', 'untracked']) {
  test(`a ${kind} change stops the guide before compilation or installation and remains untouched`, async t => {
    const f = await fixture(t);
    const changed = join(f.cwd, kind === 'untracked' ? 'extra.txt' : 'fixture.txt');
    await writeFile(changed, 'uncommitted-work\n');
    if (kind === 'staged') f.git('add', 'fixture.txt');
    const before = f.git('status', '--porcelain=v1', '--untracked-files=all');
    refused(f.run(`${prepare}\n${select}`));
    assert.equal(existsSync(join(f.root, 'build-invocations')), false);
    assert.equal(existsSync(f.prefix), false);
    assert.equal(await readFile(changed, 'utf8'), 'uncommitted-work\n');
    assert.equal(f.git('status', '--porcelain=v1', '--untracked-files=all'), before);
  });
}

for (const mode of ['tracked-after', 'untracked-after', 'head-after', 'build-failure']) {
  test(`${mode} prevents installation and never selects the old HEAD's artifacts`, async t => {
    const f = await fixture(t);
    refused(f.run(`${prepare}\n${select}`, mode));
    assert.equal(existsSync(join(f.root, 'build-invocations')), true);
    assert.equal(existsSync(f.prefix), false);
    if (mode === 'tracked-after') assert.equal(await readFile(join(f.cwd, 'fixture.txt'), 'utf8'), 'original\nchanged\n');
    if (mode === 'untracked-after') assert.equal(await readFile(join(f.cwd, 'extra.txt'), 'utf8'), 'untracked\n');
    if (mode === 'head-after') assert.notEqual(f.git('rev-parse', 'HEAD'), f.revision);
  });
}
