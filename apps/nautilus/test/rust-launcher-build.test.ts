import assert from 'node:assert/strict';
import { test } from 'node:test';
import { verifyLauncherIdentity, verifyOwnerNativeResolution, launcherFeatures } from '../src/rust-launcher-build.ts';
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
      if (['/src', '/tests', '/examples'].some(suffix => path.endsWith(suffix))) { await mkdir(join(glue, path), { recursive: true }); await writeFile(join(glue, path, 'lib.rs'), ''); }
      else { await mkdir(dirname(join(glue, path)), { recursive: true }); await writeFile(join(glue, path), 'fixture'); }
    }
    await writeFile(join(glue, 'apps/job/Cargo.toml'), '[dependencies]\nnautilus-model = { git = \"https://old\", rev = \"old\", default-features = false }\n');
    await writeFile(join(glue, 'apps/job/src/native_runner.rs'), '// fixture, never executed');
    const caps = { identity: { nautilusSource: resolved().packages[0]!.source, nautilusVersion: '0.69.0', precision: 'standard' }, protocol: 'quazonai-native-process-v1', running: false, research: { configuredRunFactory: true }, account: { registration: false } };
    const discovery = { workspace_root: owner, packages: [{ name: 'owner', manifest_path: manifestPath, targets: [{ name: 'owner', kind: ['lib'] }], dependencies: [], features: {}, rust_version: null }] };

    const cargo = join(root, 'cargo');
    await writeFile(join(root, 'rustc'), '#!/bin/sh\necho "host: x86_64-unknown-linux-gnu"\n', { mode: 0o755 });
    await writeFile(cargo, `#!/usr/bin/env node
const fs=require('node:fs'),p=require('node:path');
const args=process.argv.slice(2);fs.appendFileSync(${JSON.stringify(join(root, 'calls'))},JSON.stringify(args)+'\\n');
if(args[0]==='metadata') { const m=args[args.indexOf('--manifest-path')+1]; const full=${JSON.stringify(resolved())}; full.packages.push({id:'root',name:'root',version:'0',source:null,manifest_path:m});full.resolve.nodes[0].deps=[];full.resolve.nodes.push({id:'root',features:[],deps:[{pkg:'model',dep_kinds:[{kind:null}]}]}); console.log(JSON.stringify(args.includes('--no-deps')?${JSON.stringify(discovery)}:full)); }
else if(args[0]==='build' && process.env.CARGO_TARGET_DIR.includes('interrupt-build')) { console.error('build-in-progress');setInterval(()=>{},1000); }
else if(args[0]==='build') { const dest=p.join(process.env.CARGO_TARGET_DIR,args[args.indexOf('--target')+1],'debug');fs.mkdirSync(dest,{recursive:true});const cap={...${JSON.stringify(caps)},compiledFrom:process.cwd()}; const text='#!/usr/bin/env node\\nif(process.argv[2]!=="capabilities")process.exit(22);console.log('+JSON.stringify(JSON.stringify(cap))+');'; fs.writeFileSync(p.join(dest,args[args.indexOf('--bin')+1]),text,{mode:0o755}); }
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
    assert.ok(calls.slice(1).filter(a => a[0] === 'metadata').every(a => a.includes('--filter-platform')));
    assert.ok(calls[1]!.includes('--locked'));
    assert.ok(calls[3]!.includes('--frozen'));
    assert.equal(calls[3]![calls[3]!.indexOf('--target') + 1], 'x86_64-unknown-linux-gnu');
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

test('actual generated observation launcher compiles without an account hook', async t => {
  const cargo = process.env.QZ_TEST_CARGO;
  const root = process.env.QZ_TEST_OBSERVATION_BUILD;
  if (!cargo || !root) { t.skip('requires explicit actual Cargo and an isolated retained build directory'); return; }
  const { mkdir, readFile, writeFile } = await import('node:fs/promises');
  const { join, resolve } = await import('node:path');
  const { execFile } = await import('node:child_process');
  const { promisify } = await import('node:util');
  const execute = promisify(execFile);
  const { buildRustLauncher } = await import('../src/rust-launcher-build.ts');
  await mkdir(root); // Never overwrite a previous compilation or acceptance artifact.
  const owner = join(root, 'owner'); await mkdir(join(owner, 'src'), { recursive: true });
  const glue = resolve(import.meta.dirname, '../../..');
  const manifest = await readFile(join(glue, 'apps/job/Cargo.toml'), 'utf8');
  const nativeNames = ['nautilus-backtest', 'nautilus-common', 'nautilus-core', 'nautilus-execution', 'nautilus-model', 'nautilus-persistence'];
  const dependencies = nativeNames.map(name => {
    const line = manifest.split('\n').find(line => line.startsWith(`${name} = `));
    assert.ok(line, name);
    return line.replace(/, optional = true/g, '');
  });
  await writeFile(join(owner, 'Cargo.toml'), `[package]\nname="source-observation-owner"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nanyhow="=1.0.104"\nserde_json="=1.0.154"\n${dependencies.join('\n')}\n`);
  // An ordinary test-owned native factory, not a research strategy or replacement runtime.
  // Retained output allows the same actually compiled launcher to be exercised on an
  // explicitly selected training catalog without uploading the user's source records.
  await writeFile(join(owner, 'src/lib.rs'), `
use anyhow::{Context, Result, ensure};
use nautilus_backtest::{config::{BacktestRunConfig, BacktestVenueConfig}, engine::BacktestEngine};
use nautilus_common::{actor::{DataActor, DataActorConfig, DataActorCore}, nautilus_actor};
use nautilus_core::UnixNanos;
use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
use nautilus_model::{data::{CustomData, Data, DataType}, enums::{AccountType, BookType, OmsType}, identifiers::ActorId};
use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
use serde_json::{Value, json};
use std::path::Path;
fn time(value: &Value, key: &str) -> Result<UnixNanos> {
    Ok(value[key].as_str().context("explicit nanosecond string required")?.parse::<u64>()?.into())
}
pub mod observer {
    use super::*;
    pub fn config(root: &Path, name: &str, value: &Value) -> Result<BacktestRunConfig> {
        ensure!(name == "source-observer", "UNKNOWN_FACTORY");
        ensure!(root.canonicalize()? == Path::new(value["catalog"].as_str().context("catalog required")?).canonicalize()?, "CATALOG_MUST_MATCH_BOUND_ROOT");
        let venue = BacktestVenueConfig::builder().name("SIM").oms_type(OmsType::Netting)
            .account_type(AccountType::Margin).book_type(BookType::L1_MBP)
            .starting_balances(vec!["1000 USD".into()])
            .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero())).build()?;
        Ok(BacktestRunConfig::builder().venues(vec![venue]).data(Vec::new())
            .start(time(value,"start_ns")?).end(time(value,"end_ns")?)
            .raise_exception(true).dispose_on_completion(false).build()?)
    }
    pub fn install(engine: &mut BacktestEngine, _: &str, value: &Value) -> Result<()> {
        let mut catalog = ParquetDataCatalog::new(Path::new(value["catalog"].as_str().context("catalog required")?), None, None, None, None);
        // Registration belongs to the generated launcher. This owner does not redeclare
        // the concrete source type or import job to get its implementation.
        let data = catalog.query_custom_data_dynamic("ChainticksFillObservation", None,
            Some(time(value,"start_ns")?), Some(time(value,"end_ns")?), None, None, true)?;
        let Data::Custom(first) = data.first().context("EMPTY_SELECTED_SOURCE")? else { anyhow::bail!("EXPECTED_CUSTOM") };
        let data_type = first.data_type.clone();
        let selected = data.len();
        let report = value["report"].as_str().context("explicit new report required")?.to_owned();
        ensure!(!Path::new(&report).exists(), "REPORT_ALREADY_EXISTS");
        engine.add_data(data, None, true, true)?;
        engine.add_actor(Observer { core: DataActorCore::new(DataActorConfig {
            actor_id: Some(ActorId::from("SOURCE-OBSERVER")), ..Default::default()
        }), data_type, selected, count: 0, report, first: None, last: None })?;
        Ok(())
    }
}
#[derive(Debug)]
struct Observer { core: DataActorCore, data_type: DataType, selected: usize, count: usize,
    report: String, first: Option<Value>, last: Option<Value> }
nautilus_actor!(Observer);
impl DataActor for Observer {
    fn on_start(&mut self) -> Result<()> { self.subscribe_data(self.data_type.clone(), None, None); Ok(()) }
    fn on_data(&mut self, data: &CustomData) -> Result<()> {
        let value: Value = serde_json::from_str(&data.data.to_json()?)?;
        ensure!(self.clock().timestamp_ns().as_u64() == value["ts_init"].as_u64().context("integer clock required")?, "CALLBACK_CLOCK_MISMATCH");
        self.count += 1;
        if self.first.is_none() { self.first = Some(value.clone()); }
        self.last = Some(value);
        Ok(())
    }
    fn on_stop(&mut self) -> Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&self.report)?;
        serde_json::to_writer(&mut file, &json!({"selected":self.selected,"callbacks":self.count,
            "first":self.first,"last":self.last,"scope":"SOURCE_OBSERVATION_REPLAY_NOT_STRATEGY_RESEARCH"}))?;
        Ok(())
    }
}
`);
  await execute(cargo, ['generate-lockfile', '--offline', '--manifest-path', join(owner, 'Cargo.toml')], { cwd: owner, maxBuffer: 8 * 1024 * 1024 });
  const result = await buildRustLauncher({
    source: { cargoBinary: cargo, manifestPath: join(owner, 'Cargo.toml'), packageName: 'source-observation-owner' },
    factories: { backtestFactory: 'observer', chainticksFillObservations: true },
    glueSourceRoot: glue, directory: join(root, 'build'),
    ...(process.env.QZ_TEST_NATIVE_TARGET ? { targetDirectory: process.env.QZ_TEST_NATIVE_TARGET } : {}),
  });
  const caps = result.capabilities as { research: { configuredRunFactory: boolean }; account: { compiled: boolean; registration: boolean }; running: boolean };
  assert.equal(caps.research.configuredRunFactory, true);
  assert.equal(caps.account.compiled, false);
  assert.equal(caps.account.registration, false);
  assert.equal(caps.running, false);
  await writeFile(join(root, 'build-result.json'), JSON.stringify(result), { flag: 'wx' });
});

test('official Cargo offline resolver repairs stale owner lock without running build scripts', {skip: !process.env.QZ_TEST_CARGO}, async () => {
  const {mkdtemp,mkdir,writeFile,readFile,rm,symlink} = await import('node:fs/promises');
  const {tmpdir} = await import('node:os'); const {join} = await import('node:path');
  const {execFile} = await import('node:child_process'); const {promisify} = await import('node:util');
  const {discoverRustEnvironment} = await import('../src/rust-environment.ts');
  const {prepareResearchOwnerLock} = await import('../src/rust-launcher-build.ts');
  const run=promisify(execFile), root=await mkdtemp(join(tmpdir(),'qz-offline-lock-'));
  try {
    const owner=join(root,'owner'), dependency=join(root,'dependency');
    for(const [path,name] of [[owner,'owner'],[dependency,'local_dependency']]) {
      await mkdir(join(path!,'src'),{recursive:true});
      await writeFile(join(path!,'src/lib.rs'),'pub fn value() {}');
      await writeFile(join(path!,'Cargo.toml'),`[package]\nname="${name}"\nversion="0.1.0"\nedition="2021"\n[workspace]\n`);
    }
    await writeFile(join(owner,'build.rs'),'fn main() { panic!("metadata must not execute build.rs"); }');
    const source={cargoBinary:process.env.QZ_TEST_CARGO!,manifestPath:join(owner,'Cargo.toml'),packageName:'owner'};
    const discovery=await discoverRustEnvironment(source);
    await prepareResearchOwnerLock(discovery,root);
    const original=await readFile(join(owner,'Cargo.lock'),'utf8');
    await writeFile(source.manifestPath,(await readFile(source.manifestPath,'utf8'))+'\n[dependencies]\nlocal_dependency={path="../dependency"}\n');
    await assert.rejects(run(source.cargoBinary,['metadata','--offline','--locked','--format-version','1','--manifest-path',source.manifestPath],{cwd:owner}), /lock/);
    assert.equal(await readFile(join(owner,'Cargo.lock'),'utf8'),original);
    const priorPath=process.env.PATH, priorRustc=process.env.RUSTC;
    let prepared;
    try {process.env.PATH='/nonexistent';process.env.RUSTC='/nonexistent/rustc';prepared=await prepareResearchOwnerLock(discovery,root);}
    finally {process.env.PATH=priorPath;if(priorRustc===undefined)delete process.env.RUSTC;else process.env.RUSTC=priorRustc;}
    assert.match(prepared.stderr,/local_dependency/);
    const repaired=await readFile(join(owner,'Cargo.lock'),'utf8');assert.notEqual(repaired,original);
    await run(source.cargoBinary,['metadata','--offline','--locked','--format-version','1','--manifest-path',source.manifestPath],{cwd:owner});
    await prepareResearchOwnerLock(discovery,root);assert.equal(await readFile(join(owner,'Cargo.lock'),'utf8'),repaired);
    await assert.rejects(prepareResearchOwnerLock(discovery,dependency),/outside_research_workspace/);
    await rm(join(owner,'Cargo.lock'));await symlink(join(root,'absent.lock'),join(owner,'Cargo.lock'));
    await assert.rejects(prepareResearchOwnerLock(discovery,root),/symlink/);
    await rm(join(owner,'Cargo.lock'));await writeFile(join(owner,'Cargo.lock'),repaired);
    await writeFile(source.manifestPath,(await readFile(source.manifestPath,'utf8'))+'qz_nonexistent_dependency_897236="=99.99.99"\n');
    await assert.rejects(prepareResearchOwnerLock(discovery,root),error=>{
      assert.match(String(error),/owner_lock_preparation/);assert.match(String((error as Error).cause),/offline|no matching package/);return true;
    });
  } finally {await rm(root,{recursive:true,force:true});}
});

test('market preparation is opt-in and preserves explicit owner precision', () => {
  assert.deepEqual(launcherFeatures(resolved(), { backtestFactory: 'research' }), []);
  assert.deepEqual(launcherFeatures(resolved(), { backtestFactory: 'research', marketImport: false }), []);
  assert.deepEqual(launcherFeatures(resolved(undefined, ['high-precision']), { backtestFactory: 'research' }), ['high-precision']);
  assert.throws(() => launcherFeatures(resolved(), { marketImport: true }), /requires_high_precision_owner/);
  assert.deepEqual(launcherFeatures(resolved(undefined, ['high-precision']), { backtestFactory: 'research', marketImport: true }), ['market-history', 'high-precision']);
});
