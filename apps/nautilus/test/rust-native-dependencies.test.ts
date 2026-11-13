import assert from 'node:assert/strict';
import { test } from 'node:test';
import { alignCopiedNativeManifest, selectedCargoGraph, type CargoResolvedMetadata } from '../src/rust-native-dependencies.ts';
const graph = (source: string | null): CargoResolvedMetadata => ({ packages: [
  { id: 'owner', name: 'owner', version: '1', source: null, manifest_path: '/owner/Cargo.toml' },
  { id: 'model', name: 'nautilus-model', version: '0.70.0', source, manifest_path: '/native/model/Cargo.toml' },
  { id: 'unused', name: 'nautilus-model', version: '0.1.0', source: 'unused', manifest_path: '/unused/Cargo.toml' },
], resolve: { nodes: [
  { id: 'owner', features: [], deps: [{ pkg: 'model', dep_kinds: [{ kind: null }] }, { pkg: 'unused', dep_kinds: [{ kind: 'dev' }] }] },
  { id: 'model', features: [], deps: [] }, { id: 'unused', features: [], deps: [] },
] } });
const manifest = '[dependencies]\nnautilus-model = { git = "https://old", rev = "old", default-features = false, features = ["test-support"] }\n[dependencies.nautilus-model]\ngit = "https://old"\nrev = "old"\noptional = true\n';
test('selected graph excludes unrelated and dev-only workspace package identities', () => {
  const original = graph(null); const before = JSON.stringify(original);
  const chosen = selectedCargoGraph(original, 'owner');
  assert.deepEqual(chosen.packages.map(p => p.id), ['owner', 'model']);
  assert.equal(JSON.stringify(original), before);
  assert.throws(() => selectedCargoGraph(original, 'missing'), /node_missing/);
});
test('copied manifest preserves flags while aligning exact git reference, registry version, or actual local path', () => {
  for (const [source, expected] of [
    ['git+https://github.com/nautechsystems/nautilus_trader.git?branch=develop#abcdef', 'git = "https://github.com/nautechsystems/nautilus_trader.git", branch = "develop"'],
    ['registry+https://github.com/rust-lang/crates.io-index', 'version = "=0.70.0"'],
    [null, 'path = "/native/model"'],
  ] as const) {
    const owner = selectedCargoGraph(graph(source), 'owner'); const before = JSON.stringify(owner);
    const result = alignCopiedNativeManifest(manifest, owner);
    assert.ok(result.includes(expected)); assert.ok(result.includes('optional = true')); assert.ok(result.includes('features = ["test-support"]'));
    assert.ok(!result.includes('https://old')); assert.equal(JSON.stringify(owner), before);
  }
});
test('missing registry/path package and ambiguous owner identities are explicit errors, not guessed versions', () => {
  const request = manifest.replaceAll('nautilus-model', 'nautilus-live');
  for (const source of [null, 'registry+https://github.com/rust-lang/crates.io-index']) {
    assert.throws(() => alignCopiedNativeManifest(request, selectedCargoGraph(graph(source), 'owner')), /owner_package_source_missing/);
  }
  assert.throws(() => alignCopiedNativeManifest(manifest, graph(null)), /ambiguous_owner_package/);
});
