import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
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
