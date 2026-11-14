import assert from 'node:assert/strict';
import { readFileSync, lstatSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import test from 'node:test';
const root = resolve(import.meta.dirname, '../..');
test('application ships an allowlisted QZ source subset, no native binaries/toolchain/vendor', () => {
  const docker = readFileSync(resolve(root, 'deploy/docker/Dockerfile'), 'utf8');
  const copies = docker.split('\n').filter(line => line.startsWith('COPY ') && line.includes('/opt/quazonai/native-glue/'));
  assert.equal(copies.length, 10);
  const sources = copies.flatMap(line => line.split(/\s+/).slice(2, -1));
  assert.deepEqual(sources, ['Cargo.toml', 'Cargo.lock', 'apps/job/Cargo.toml', 'apps/job/build.rs', 'apps/job/build_identity.rs', 'apps/job/src', 'apps/job/examples', 'apps/job/tests', 'crates/contracts/Cargo.toml', 'crates/contracts/src', 'crates/domain/Cargo.toml', 'crates/domain/src', 'AGENTS.md']);
  function check(path) {
    const stat = lstatSync(resolve(root, path)); assert.equal(stat.isSymbolicLink(), false, path);
    if (stat.isDirectory()) for (const child of readdirSync(resolve(root, path))) check(`${path}/${child}`);
    else assert.match(path, /\.(rs|toml|lock|md)$/, path);
  }
  sources.forEach(check);
  const application = docker.slice(docker.indexOf('FROM execution-base AS application'));
  assert.doesNotMatch(application, /COPY.*--from=nautilus|rustup|cargo install|\/native\/job/);
});

test('application process builder includes its selected dependency graph module', () => {
  const docker = readFileSync(resolve(root, 'deploy/docker/Dockerfile'), 'utf8');
  const applicationSources = docker.slice(docker.indexOf('FROM scratch AS application-source'), docker.indexOf('FROM execution-base AS application'));
  const line = applicationSources.split('\n').find(line => line.startsWith('COPY ') && line.includes('apps/nautilus/src/rust-launcher-build.ts'));
  assert.ok(line);
  assert.ok(line.split(/\s+/).includes('apps/nautilus/src/rust-native-dependencies.ts'));
});

test('Docker context admits native glue COPY inputs and keeps private data excluded last', () => {
  const rules = readFileSync(resolve(root, 'deploy/docker/Dockerfile.dockerignore'), 'utf8').trim().split('\n');
  assert.equal(rules[0], '**');
  const docker = readFileSync(resolve(root, 'deploy/docker/Dockerfile'), 'utf8');
  const copies = docker.split('\n').filter(line => line.startsWith('COPY ') && line.includes('/opt/quazonai/native-glue/'));
  const sources = copies.flatMap(line => line.split(/\s+/).slice(2, -1));
  sources.push('apps/nautilus/package.json', 'apps/nautilus/src');
  for (const source of sources) {
    const directory = lstatSync(resolve(root, source)).isDirectory();
    assert.ok(rules.includes(`!${source}${directory ? '/' : ''}`), source);
    if (directory) assert.ok(rules.includes(`!${source}/**`), source);
    for (let parent = source.substring(0, source.lastIndexOf('/')); parent; parent = parent.includes('/') ? parent.substring(0, parent.lastIndexOf('/')) : '') {
      assert.ok(rules.includes(`!${parent}/`), parent);
    }
  }
  const privateRules = ['**/node_modules', '**/dist', '**/test-results', '**/.env*', '**/provider.env', '**/auth.json', '**/master.key'];
  assert.deepEqual(rules.slice(-privateRules.length), privateRules);
  assert.equal(rules.some(rule => /^!(data|target|\.git|\.cargo|\.rustup)(\/|$)/.test(rule)), false);
});


test('application normalizes source glue directories and files before the non-root user', () => {
  const docker = readFileSync(resolve(root, 'deploy/docker/Dockerfile'), 'utf8');
  const application = docker.split('FROM execution-base AS application\n')[1];
  assert.ok(application.includes('find native-glue apps/nautilus -type d -exec chmod 755 {} +'));
  assert.ok(application.includes('find native-glue apps/nautilus -type f -exec chmod 644 {} +'));
  assert.ok(application.indexOf('RUN find') < application.indexOf('USER node'));
});
