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
