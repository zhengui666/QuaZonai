import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import type { ProcessNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { setup } from './support.ts';

// Controlled process transport only; no market coverage or Rust build claim.
async function backend(id: string) {
  const directory = await fs.promises.mkdtemp(join(tmpdir(), 'qz-coverage-process-'));
  const binary = join(directory, 'runner.mjs'), marker = join(directory, 'calls.jsonl'), failure = join(directory, 'fail');
  await fs.promises.writeFile(binary, `#!${process.execPath}\nimport fs from 'node:fs';let text='';for await(const chunk of process.stdin)text+=chunk;const input=JSON.parse(text);fs.appendFileSync(${JSON.stringify(marker)},JSON.stringify({command:process.argv[2],args:process.argv.slice(3),input})+'\\n');if(fs.existsSync(${JSON.stringify(failure)})){process.stderr.write('CONTROLLED_DATASET_BACKEND_FAILURE');process.exit(1);}process.stdout.write(JSON.stringify({backend:${JSON.stringify(id)},request:input}));`, { mode: 0o755 });
  const target: ProcessNativeTarget = { kind: 'RUST_PROCESS', id, binary, capabilities: { schemaVersion: 1, protocol: 'quazonai-native-process-v1', identity: { nautilusVersion: 'protocol-only', precision: 'high' }, research: { singleStrategyRegistration: false, configuredRunFactory: true }, account: { compiled: false, registration: false, command: 'account-runtime', requiresExplicitStart: true }, running: false } };
  return { target, async calls(): Promise<Array<{command:string;args:string[];input:unknown}>> { try { return (await fs.promises.readFile(marker,'utf8')).trim().split('\n').map(line=>JSON.parse(line)); } catch(error) { if((error as NodeJS.ErrnoException).code==='ENOENT')return [];throw error; } },
    async fail() { await fs.promises.writeFile(failure,'fail'); }, async close() { await fs.promises.rm(directory,{recursive:true,force:true}); } };
}

for (const failing of [false, true]) test(`coverage freezes the original dataset backend across an asynchronous instance switch (failure=${failing})`, { timeout: 20000 }, async () => {
  const state = await setup(); const dataset = await backend('dataset-A'); const other = await backend('global-B');
  const originalMkdtemp = fs.promises.mkdtemp;
  let release!: () => void; let entered!: () => void;
  const barrier = new Promise<void>(resolve => { release = resolve; });
  const waiting = new Promise<void>(resolve => { entered = resolve; });
  try {
    const original = (await state.service.app.inject({url:'/api/v3/settings/instances',headers:state.headers})).json();
    await fs.promises.writeFile(join(state.directory,'instances.json'),JSON.stringify({...original,runner:dataset.target}),{mode:0o600});
    await state.restart();
    assert.deepEqual((await state.service.app.inject({url:'/api/v3/settings/instances',headers:state.headers})).json().runner,dataset.target,'original selected runner survives restart');
    if (failing) await dataset.fail();
    const catalog = state.options.config.datasets[0]!.data.catalog;
    // Pause the actual existing I/O boundary, not the target resolver or backend.
    // The native route has already read its dataset/catalog but has not dispatched.
    fs.promises.mkdtemp = (async (...args: Parameters<typeof originalMkdtemp>) => {
      if (String(args[0]).includes('catalog-coverage-')) { entered(); await barrier; }
      return originalMkdtemp(...args);
    }) as typeof originalMkdtemp;
    syncBuiltinESMExports();
    const payload = { selectors: [{ data_type: 'TradeTick', identifier: 'BTC/USDT.BINANCE' }], start_ns: '9007199254740993', end_ns: '9007199254740995' };
    const pending = state.service.app.inject({ method: 'POST', url: '/api/v3/datasets/controlled/coverage', headers: state.headers, payload });
    await waiting;
    const current = (await state.service.app.inject({ method: 'GET', url: '/api/v3/settings/instances', headers: state.headers })).json();
    const switched = await state.service.app.inject({ method: 'PUT', url: '/api/v3/settings/instances', headers: state.headers, payload: { expectedRevision: current.revision, environment: {kind:'RUST_PROCESS',id:other.target.id,cargoBinary:process.execPath,manifestPath:join(state.directory,'Cargo.toml'),packageName:'protocol_only'} } });
    assert.equal(switched.statusCode, 200, switched.body);
    assert.equal(switched.json().environment.id,other.target.id);
    assert.equal(switched.json().runner,null,'changing source clears the selected artifact without an implicit build');
    release();
    const response = await pending;
    if (failing) assert.ok(response.statusCode >= 400, response.body);
    else { assert.equal(response.statusCode, 200, response.body); assert.equal(response.json().backend, 'dataset-A'); assert.deepEqual(response.json().request, payload); }
    const calls = await dataset.calls();
    assert.equal(calls.length, 1, 'the in-flight query remains with its captured dataset');
    assert.equal((await other.calls()).length, 0, 'neither switching nor failure redirects an existing query');
    assert.equal(calls[0]!.command, 'catalog-coverage'); assert.deepEqual(calls[0]!.args, ['--catalog',String(catalog)]);
    assert.deepEqual(calls[0]!.input,payload,'nanosecond strings remain exact even beyond Number safe integers');
  } finally {
    release(); fs.promises.mkdtemp = originalMkdtemp; syncBuiltinESMExports();
    await state.cleanup(); await dataset.close(); await other.close();
  }
});
