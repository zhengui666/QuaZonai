import assert from 'node:assert/strict';
import { test } from 'node:test';
import { verifyLauncherIdentity, verifyOwnerNativeResolution } from '../src/rust-launcher-build.ts';
const resolved = (source = 'git+https://github.com/nautechsystems/nautilus_trader.git?rev=abc#abc', precision: string[] = []) => ({
  packages: [{ id: 'model', name: 'nautilus-model', version: '0.69.0', source }],
  resolve: { nodes: [{ id: 'model', features: precision }] },
});
test('compile preflight retains actual owner source/version/precision, not merely crate names', () => {
  verifyOwnerNativeResolution(resolved(), resolved());
  assert.throws(() => verifyOwnerNativeResolution(resolved(), resolved(undefined, ['high-precision'])), /owner_dependency_changed/);
  assert.throws(() => verifyOwnerNativeResolution(resolved(), resolved('registry+https://github.com/rust-lang/crates.io-index')), /owner_dependency_changed/);
  const other = resolved(); other.packages[0]!.version = '0.70.0';
  assert.throws(() => verifyOwnerNativeResolution(resolved(), other), /owner_dependency_changed/);
  const mixed = resolved(); mixed.packages.push({ id: 'other', name: 'nautilus-model', version: '0.68.0', source: mixed.packages[0]!.source });
  assert.throws(() => verifyOwnerNativeResolution(resolved(), mixed), /mixed_native_sources/);
  const local = resolved(); local.packages[0]!.source = null as unknown as string;
  assert.throws(() => verifyOwnerNativeResolution(local, local), /path_native_provenance_unverified/);
  assert.throws(() => verifyOwnerNativeResolution({ packages: [], resolve: { nodes: [] } }, resolved()), /native_dependencies_missing/);
});

test('explicit build command pipeline is offline, keeps original lock and inspects without BOOT (fake compiler harness)', async () => {
  const { mkdtemp, mkdir, writeFile, readFile, rm } = await import('node:fs/promises');
  const { tmpdir } = await import('node:os');
  const { join, dirname } = await import('node:path');
  const { buildRustLauncher, nativeGlueFiles } = await import('../src/rust-launcher-build.ts');
  const root = await mkdtemp(join(tmpdir(), 'qz-build-pipeline-'));
  try {
    const owner = join(root, 'owner'); await mkdir(owner);
    const manifestPath = join(owner, 'Cargo.toml');
    await writeFile(manifestPath, 'owner manifest'); await writeFile(join(owner, 'Cargo.lock'), 'original lock');
    const glue = join(root, 'glue');
    for (const path of nativeGlueFiles) {
      if (path.endsWith('/src')) { await mkdir(join(glue, path), { recursive: true }); await writeFile(join(glue, path, 'lib.rs'), ''); }
      else { await mkdir(dirname(join(glue, path)), { recursive: true }); await writeFile(join(glue, path), 'fixture'); }
    }
    await writeFile(join(glue, 'apps/job/src/native_runner.rs'), '// fixture, never executed');
    const caps = { identity: { nautilusSource: resolved().packages[0]!.source, nautilusVersion: '0.69.0', precision: 'standard' }, protocol: 'quazonai-native-process-v1', running: false, research: { configuredRunFactory: true }, account: { registration: false } };
    const discovery = { workspace_root: owner, packages: [{ name: 'owner', manifest_path: manifestPath, targets: [{ name: 'owner', kind: ['lib'] }], dependencies: [], features: {}, rust_version: null }] };

    const cargo = join(root, 'cargo');
    await writeFile(join(root, 'rustc'), '#!/bin/sh\nexit 22\n', { mode: 0o755 });
    await writeFile(cargo, `#!/usr/bin/env node
const fs=require('node:fs'),p=require('node:path');
const args=process.argv.slice(2);fs.appendFileSync(${JSON.stringify(join(root, 'calls'))},JSON.stringify(args)+'\\n');
if(args[0]==='metadata') console.log(JSON.stringify(args.includes('--no-deps')?${JSON.stringify(discovery)}:${JSON.stringify(resolved())}));
else if(args[0]==='build' && process.env.CARGO_TARGET_DIR.includes('interrupt-build')) { console.error('build-in-progress');setInterval(()=>{},1000); }
else if(args[0]==='build') { const dest=p.join(process.env.CARGO_TARGET_DIR,'debug');fs.mkdirSync(dest,{recursive:true});const cap={...${JSON.stringify(caps)},compiledFrom:process.cwd()}; const text='#!/usr/bin/env node\\nif(process.argv[2]!=="capabilities")process.exit(22);console.log('+JSON.stringify(JSON.stringify(cap))+');'; fs.writeFileSync(p.join(dest,args[args.indexOf('--bin')+1]),text,{mode:0o755}); }
else process.exit(23);
`, { mode: 0o755 });
    const result = await buildRustLauncher({ source: { cargoBinary: cargo, manifestPath, packageName: 'owner' }, factories: { backtestFactory: 'factory' }, glueSourceRoot: glue, directory: join(root, 'build') });
    assert.equal(result.qualification, 'COMPILED_NOT_CONNECTED');
    assert.deepEqual(result.capabilities, { ...caps, compiledFrom: join(root, 'build/launcher') });
    assert.equal(await readFile(join(owner, 'Cargo.lock'), 'utf8'), 'original lock');
    assert.equal(await readFile(manifestPath, 'utf8'), 'owner manifest');
    const calls = (await readFile(join(root, 'calls'), 'utf8')).trim().split('\n').map(line => JSON.parse(line) as string[]);
    assert.equal(calls.length, 4);
    assert.ok(calls.filter(a => a[0] === 'metadata').every(a => a.includes('--offline')));
    assert.ok(calls[1]!.includes('--locked'));
    assert.ok(calls[3]!.includes('--frozen'));
    assert.ok(!calls.flat().includes('run'));
    const controller = new AbortController();
    const interruptedDir = join(root, 'interrupt-build');
    const promise = buildRustLauncher({ source: { cargoBinary: cargo, manifestPath, packageName: 'owner' }, factories: { backtestFactory: 'factory' }, glueSourceRoot: glue, directory: interruptedDir, signal: controller.signal });
    const rejection = assert.rejects(promise, /rust_build_interrupted_outcome_unknown/);
    for (let i = 0; i < 100; i++) {
      const log = await readFile(join(interruptedDir, 'build.log'), 'utf8').catch(() => '');
      if (log.includes('build-in-progress')) break;
      await new Promise(resolve => setTimeout(resolve, 20));
    }
    controller.abort(); await rejection;
    const cache = join(root, 'shared-cache'); await mkdir(cache);
    const [left, right] = await Promise.all(['left', 'right'].map(name => buildRustLauncher({ source: { cargoBinary: cargo, manifestPath, packageName: 'owner' }, factories: { backtestFactory: name }, glueSourceRoot: glue, directory: join(root, name), targetDirectory: cache })));
    assert.equal((left!.capabilities as { compiledFrom: string }).compiledFrom, join(root, 'left/launcher'));
    assert.equal((right!.capabilities as { compiledFrom: string }).compiledFrom, join(root, 'right/launcher'));
    const latestCalls = (await readFile(join(root, 'calls'), 'utf8')).trim().split('\n').map(line => JSON.parse(line) as string[]);
    const binaryNames = latestCalls.filter(args => args[0] === 'build').map(args => args[args.indexOf('--bin') + 1]);
    assert.equal(new Set(binaryNames).size, binaryNames.length);

    assert.match(await readFile(join(interruptedDir, 'build.log'), 'utf8'), /build-in-progress/);
    assert.equal(await readFile(join(owner, 'Cargo.lock'), 'utf8'), 'original lock');

  } finally { await rm(root, { recursive: true, force: true }); }
});

test('actual binary identity must match resolved source/version/precision', () => {
  const identity = { nautilusSource: resolved().packages[0]!.source, nautilusVersion: '0.69.0', precision: 'standard' };
  verifyLauncherIdentity(resolved(), identity);
  for (const mismatch of [undefined, { ...identity, nautilusSource: 'other' }, { ...identity, nautilusVersion: '0.68.0' }, { ...identity, precision: 'high' }]) {
    assert.throws(() => verifyLauncherIdentity(resolved(), mismatch), /binary_identity_mismatch/);
  }
});
