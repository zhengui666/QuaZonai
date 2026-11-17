import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, writeFile, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { discoverRustEnvironment, materializeRustLauncher, publicRustPath, renderRustLauncher } from '../src/rust-environment.ts';

test('manifest discovery is explicit offline/no-deps; generation preserves original lock and source, never invokes factory', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-rust-source-'));
  try {
    const owner = join(root, 'owner'); await mkdir(owner);
    const manifestPath = join(owner, 'Cargo.toml'); await writeFile(manifestPath, 'owner-original');
    await writeFile(join(owner, 'Cargo.lock'), 'original-lock');
    const cargo = join(root, 'cargo');
    const metadata = { workspace_root: owner, packages: [{ name: 'user-strategy', manifest_path: manifestPath, targets: [{ name: 'user_strategy', kind: ['lib'] }], dependencies: [{ name: 'nautilus-live', source: 'git+https://example.test/official?rev=pinned', req: '*', features: ['high-precision'], uses_default_features: false }], features: { custom: [] }, rust_version: '1.99.1' }] };
    await writeFile(cargo, `#!/usr/bin/env node\nconst fs=require('node:fs'); fs.writeFileSync(${JSON.stringify(join(root,'args'))},JSON.stringify({argv:process.argv.slice(2),cwd:process.cwd()})); console.log(${JSON.stringify(JSON.stringify(metadata))});`, { mode: 0o755 });
    const result = await discoverRustEnvironment({ cargoBinary: cargo, manifestPath, packageName: 'user-strategy', features: ['custom'] });
    const call = JSON.parse(await readFile(join(root, 'args'), 'utf8'));
    assert.deepEqual(call.argv.slice(0, 4), ['metadata', '--no-deps', '--offline', '--locked']);
    assert.equal(call.cwd, owner);
    assert.equal(result.qualification, 'MANIFEST_ONLY_NOT_COMPILE_OR_PRECISION_VERIFIED');
    assert.equal(result.nativeDependencies[0]?.features[0], 'high-precision');
    const artifact = await materializeRustLauncher(result, join(root, 'launcher'), 'fn main() {}', '');
    assert.equal(artifact.qualification, 'SOURCE_ONLY_NOT_BUILT');
    assert.equal(await readFile(join(root, 'launcher/Cargo.lock'), 'utf8'), 'original-lock');
    assert.equal(await readFile(join(owner, 'Cargo.lock'), 'utf8'), 'original-lock');
    assert.equal(await readFile(manifestPath, 'utf8'), 'owner-original');
    await assert.rejects(materializeRustLauncher(result, join(root, 'launcher'), '', ''), /EEXIST/);
    await assert.rejects(materializeRustLauncher(result, join(owner, 'new'), '', ''), /outside_user_workspace/);
    await symlink(owner, join(root, 'alias'));
    await assert.rejects(materializeRustLauncher(result, join(root, 'alias/new'), '', ''), /outside_user_workspace/);
    await assert.rejects(materializeRustLauncher({ ...result, libraryTarget: null }, join(root, 'binonly'), '', ''), /public_library_required/);
    await assert.rejects(discoverRustEnvironment({ ...result.source, features: ['missing'] }), /unknown_feature/);
    await assert.rejects(discoverRustEnvironment({ ...result.source, packageName: 'missing' }), /package_not_unique/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('factory paths are Rust identifiers, not generated source fragments', () => {
  assert.equal(publicRustPath('factory::backtest_config'), 'factory::backtest_config');
  assert.throws(() => publicRustPath('factory(); panic!()'), /invalid_factory_path/);
});

test('real Cargo no-deps discovery does not run project build script, rustc wrapper, or credential provider', async t => {
  const cargo = process.env.QZ_TEST_CARGO;
  if (!cargo) { t.skip('set QZ_TEST_CARGO to an installed actual Cargo binary'); return; }
  const root = await mkdtemp(join(tmpdir(), 'qz-real-cargo-'));
  try {
    await mkdir(join(root, 'src')); await mkdir(join(root, '.cargo'));
    await writeFile(join(root, 'Cargo.toml'), '[package]\nname="owner-fixture"\nversion="0.0.0"\nedition="2021"\n[dependencies]\nnonexistent-probe={version="1",registry="probe"}\n');
    await writeFile(join(root, 'src/lib.rs'), 'pub fn factory() {}\n');
    const marker = join(root, 'executed');
    await writeFile(join(root, 'build.rs'), `fn main() { std::fs::write(${JSON.stringify(marker)}, "build").unwrap(); }`);
    const executable = join(root, 'provider-wrapper');
    await writeFile(executable, `#!/bin/sh\necho invoked >> ${JSON.stringify(marker)}\nexit 77\n`, { mode: 0o755 });
    await writeFile(join(root, '.cargo/config.toml'), `[build]\nrustc-wrapper=${JSON.stringify(executable)}\n[registries.probe]\nindex="sparse+http://127.0.0.1:9/"\ncredential-provider=[${JSON.stringify(executable)}]\n`);
    await writeFile(join(root, 'Cargo.lock'), '# owner original lock\nversion=4\n');
    const manifestPath = join(root, 'Cargo.toml');
    const before = await readFile(join(root, 'Cargo.lock'), 'utf8');
    const result = await discoverRustEnvironment({ cargoBinary: cargo, manifestPath, packageName: 'owner-fixture' });
    assert.equal(result.libraryTarget, 'owner_fixture');
    assert.equal(result.qualification, 'MANIFEST_ONLY_NOT_COMPILE_OR_PRECISION_VERIFIED');
    await assert.rejects(readFile(marker), { code: 'ENOENT' });
    assert.equal(await readFile(join(root, 'Cargo.lock'), 'utf8'), before);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('explicit hook modules render only selected capabilities and never boot nodes in generated main', () => {
  const backtest = renderRustLauncher({ backtestFactory: 'factory::backtest' });
  assert.match(backtest, /owner_strategy::factory::backtest::config/);
  assert.match(backtest, /main_with_hooks\(None, Some\(configured\), None\)/);
  assert.doesNotMatch(backtest, /fn account|BOOT|LiveNode/);
  const account = renderRustLauncher({ accountFactory: 'account' });
  assert.match(account, /owner_strategy::account::strategy\(\s*name, config, registrar.trader_id\(\), registrar.account_id\(\),\s*registrar.execution_client_id\(\), registrar.instrument_id\(\)\?, registrar.instruments\(\),\s*\)\?/);
  assert.match(account, /registrar.add\(strategy\)/);
  assert.doesNotMatch(account, /QZ-PAPER|serde_json::json|config\[/);
  assert.match(account, /main_with_hooks\(None, None, Some\(account\)\)/);
  assert.throws(() => renderRustLauncher({}), /factory_required/);
  assert.throws(() => renderRustLauncher({ accountFactory: 'bad();' }), /invalid_factory_path/);
});

test('account compatibility is an explicit declaration, never inferred from a factory',()=>{
 const ordinary=renderRustLauncher({accountFactory:'account'});
 assert.doesNotMatch(ordinary,/TrustedStrategyOnlySingleNode/);
 const declared=renderRustLauncher({accountFactory:'account',accountCompatibility:'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'});
 assert.match(declared,/main_with_hooks_and_account_compatibility/);
 assert.match(declared,/AccountCompatibility::TrustedStrategyOnlySingleNode/);
 assert.throws(()=>renderRustLauncher({backtestFactory:'backtest',accountCompatibility:'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'}),/declaration_invalid/);
});

test('source observations are explicit, registered before factory configuration, and never add an account hook', () => {
  const selected = renderRustLauncher({ backtestFactory: 'research', chainticksFillObservations: true });
  assert.ok(selected.indexOf('job::chainticks_fill::register_observation_type()') < selected.indexOf('owner_strategy::research::config'));
  assert.doesNotMatch(selected, /fn account|LiveNode/);
  assert.doesNotMatch(renderRustLauncher({ backtestFactory: 'research' }), /chainticks_fill/);
  assert.doesNotMatch(renderRustLauncher({ backtestFactory: 'research', chainticksFillObservations: false }), /chainticks_fill/);
  assert.throws(() => renderRustLauncher({ accountFactory: 'account', chainticksFillObservations: true }), /observation_backtest_factory_required/);
});


test('successful completion is explicitly selected without changing legacy factory source',()=>{
 const legacy=renderRustLauncher({backtestFactory:'observer'});
 assert.equal(renderRustLauncher({backtestFactory:'observer',backtestCompletion:false}),legacy);
 assert.doesNotMatch(legacy,/CompletedConfiguredNativeRun|with_completion|finalize_after_node_run/);
 const selected=renderRustLauncher({backtestFactory:'observer',backtestCompletion:true});
 assert.match(selected,/Result<job::native_configured::CompletedConfiguredNativeRun>/);
 assert.match(selected,/let completion_name = name.clone\(\);/);
 assert.match(selected,/let completion_config = config.clone\(\);/);
 assert.match(selected,/with_completion\(move \|node, config_id, results\| owner_strategy::observer::finalize_after_node_run\(node, config_id, results, &completion_name, &completion_config\)\)/);
 assert.match(selected,/main_with_completion_hooks\(None, Some\(configured\), None\)/);
 const combined=renderRustLauncher({backtestFactory:'observer',backtestCompletion:true,accountFactory:'account',accountCompatibility:'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'});
 assert.match(combined,/main_with_completion_hooks_and_account_compatibility/);
 assert.match(combined,/AccountCompatibility::TrustedStrategyOnlySingleNode/);
 assert.throws(()=>renderRustLauncher({accountFactory:'account',backtestCompletion:true}),/completion_factory_required/);
 assert.throws(()=>renderRustLauncher({backtestFactory:'observer',backtestCompletion:'true' as any}),/completion_factory_required/);
});
