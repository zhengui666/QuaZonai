// Real Pi/SQLite phases and NodeExecutionEnv processes with controlled source bytes.
// The transfer fixtures exercise source verification, not live provider coverage.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { createRegistry, Harness, type JsonObject, type TaskGraph, type TaskId } from '@earendil-works/pi-durable';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import { prepareSourceAcquisition, SourceAcquisitionTask, SourceAcquisitionUnitTask, sourceAcquisitionExtension } from '../src/source-acquisition.ts';
import { DataSourceState } from '../src/data-sources.ts';
import { setup } from './support.ts';

const context = BACKGROUND_CONTEXT;
const tools = fileURLToPath(new URL('../../../runtimes/data', import.meta.url));

async function fixture(kind: 'http' | 'hub' | 'snapshot', count = 3, fail = false) {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-source-task-'));
  const log = join(directory, 'transfers.jsonl');
  const wrapper = join(directory, 'controlled-source.py');
  await writeFile(wrapper, `import sys, json, io, time\nfrom pathlib import Path\nsys.path.insert(0, ${JSON.stringify(tools)})\nimport source_units, acquire, snapshot\nlog = Path(${JSON.stringify(log)})\ndef record(event, index=-1):\n    with log.open('a') as out: out.write(json.dumps({'event':event,'index':index,'at':time.time()})+'\\n')\nif sys.argv[1] == 'execute':\n    plan = json.loads(Path(sys.argv[sys.argv.index('--plan')+1]).read_text())\n    items = plan.get('requests', plan.get('files'))\n    def transfer(url, **kwargs):\n        index = next(i for i,item in enumerate(items) if item['url'] == url)\n        record('start',index)\n        if ${fail ? 'True' : 'False'} and index == 0:\n            record('failed',index)\n            raise RuntimeError('private-token=source-secret https://private.example/source?token=source-secret')\n        time.sleep(0.12)\n        record('end',index)\n        if 'requests' in plan:\n            return json.dumps([[items[index]['start_seconds'],1,3,2,2,1]]).encode(), {'status':200,'headers':{},'request_started_at':acquire.now(),'retrieved_at':acquire.now()}\n        return io.BytesIO(b'data')\n    acquire.fetch = transfer\n    snapshot.urllib.request.urlopen = transfer\nif sys.argv[1] == 'publish': record('publish')\nsource_units.main()\n`);
  const environment = new NodeExecutionEnv({ cwd: directory }); const output: string[] = [];
  try {
    const code = kind === 'http'
      ? `import acquire,providers; print(json.dumps(acquire.plan('coinbase-candles',providers.Selection('BTC-USD',0,36000,60))))`
      : kind === 'snapshot'
      ? `import snapshot,hashlib; digest=hashlib.sha256(b'data').hexdigest(); blob=hashlib.sha1(b'blob 4\\0data').hexdigest(); files={f'data/{i}.bin':{'size':4,**({'blobId':blob} if i%2==0 else {'lfs':{'sha256':digest,'size':4}})} for i in range(${count})}; snapshot.repository_metadata=lambda dataset,revision: ('a'*40,files,{'cardData':{'license':'cc0'}},'https://huggingface.co/api/datasets/example/history'); print(json.dumps(snapshot.plan('example/history',['data/*.bin'])))`
      : `import snapshot; files=[{'path':f'data/{i}.bin','size':4,'url':snapshot.repository_file_url('example/history','a'*40,f'data/{i}.bin')} for i in range(${count})]; print(json.dumps({'schema_version':1,'repository':'example/history','revision':'a'*40,'license':'cc0','total_bytes':4*len(files),'files':files}))`;
    const result = getOrThrow(await environment.exec(['python3', '-c', `import sys,json; sys.path.insert(0,${JSON.stringify(tools)}); ${code}`], {
      onOutput(chunk, _context, info) { if (info.stream === 'stdout') output.push(chunk); },
    }, context));
    assert.equal(result.exitCode, 0);
  } finally { await environment.cleanup(context); }
  const plan = JSON.parse(output.join('')) as JsonObject;
  const terms = join(directory, 'owner-terms.txt'); await writeFile(terms, 'Original source terms');
  const previous = process.env.QZ_SOURCE_UNIT_TOOL; process.env.QZ_SOURCE_UNIT_TOOL = wrapper;
  const prepared = await prepareSourceAcquisition({ kind, frozenPlan: plan, workdir: join(directory, 'units'), output: join(directory, 'published'),
    ...(kind === 'http' ? { termsFile: terms } : { cacheDir: join(directory, 'cache') }) });
  if (previous === undefined) delete process.env.QZ_SOURCE_UNIT_TOOL; else process.env.QZ_SOURCE_UNIT_TOOL = previous;
  const registry = createRegistry(); registry.install(sourceAcquisitionExtension);
  const database = join(directory, 'durable.sqlite');
  const open = async () => Harness.open(await openNodeSqliteStorage(database), { registry, models: createModels() }, context);
  const harness = await open();
  return { directory, plan, terms, prepared, harness, open,
    async events() { return (await readFile(log, 'utf8')).trim().split('\n').filter(Boolean).map(line => JSON.parse(line) as { event: string; index: number; at: number }); },
    async clean() { await rm(directory, { recursive: true, force: true }); } };
}

async function start(harness: Harness, prepared: Awaited<ReturnType<typeof prepareSourceAcquisition>>) {
  const owner = await harness.root(context);
  return owner.commit(tx => tx.createTask(SourceAcquisitionTask, prepared, { ownership: { kind: 'conversation' } }), context);
}

test('source units resume original official children after a SQLite reopen and freeze owner inputs', { timeout: 30_000 }, async () => {
  const state = await fixture('http'); let harness = state.harness;
  try {
    const id = await start(harness, state.prepared);
    // Neither future owner edits nor mutation of the caller's object can change the admitted bytes.
    await writeFile(state.terms, 'Changed after admission'); state.plan.provider = { id: 'not-the-original-plan' };
    let sleeping!: () => void; const asleep = new Promise<void>(resolve => { sleeping = resolve; });
    const check = (graph: TaskGraph) => { if (graph.tasks[String(id)]?.state.status === 'sleeping') sleeping(); };
    const observer = await harness.observeTaskGraph({ advance(call) { if (call.kind === 'value') check(call.value); }, end() {} }, context);
    check(observer.value); harness.resume(); await asleep; observer.detach();
    const checkpoint = await harness.getTask(id, context);
    assert.equal(checkpoint?.state.status, 'sleeping');
    if (checkpoint?.state.status !== 'sleeping') throw new Error('expected the durable source throttle checkpoint');
    assert.equal((checkpoint.state.checkpoint as { nextIndex: number }).nextIndex, 1);
    const before = await harness.commit(tx => tx.scanTasks({ kind: SourceAcquisitionUnitTask.definition.name }, 100), context);
    assert.equal(before.items.length, 1); assert.equal(before.items[0]!.state.status, 'terminal');
    const originalChild = before.items[0]!.id;
    await harness.close(context); harness = await state.open();
    const settled = await harness.waitForTask(id, context);
    assert.equal(settled.state.outcome.status, 'completed');
    if (settled.state.outcome.status !== 'completed') throw new Error(JSON.stringify(settled.state.outcome));
    assert.deepEqual(Object.keys(settled.state.outcome.result).sort(), ['manifestPath', 'planId']);
    const children = await harness.commit(tx => tx.scanTasks({ kind: SourceAcquisitionUnitTask.definition.name }, 100), context);
    assert.equal(children.items.length, 3); assert.equal(children.items[0]!.id, originalChild);
    for (const child of children.items) {
      assert.equal(child.owner, id); assert.equal(child.state.status, 'terminal');
      assert.equal('frozenPlan' in (child.input as JsonObject), false);
    }
    assert.equal('frozenPlan' in (settled.input as unknown as JsonObject), false);
    const events = await state.events();
    assert.deepEqual(events.filter(event => event.event === 'start').map(event => event.index), [0, 1, 2]);
    assert.equal(events.filter(event => event.event === 'publish').length, 1);
    assert.equal(await readFile(join(state.directory, 'published', 'source-terms.bin'), 'utf8'), 'Original source terms');
    const manifest = JSON.parse(await readFile(settled.state.outcome.result.manifestPath, 'utf8'));
    assert.equal(manifest.record_count, 3);
  } finally { await harness.close(context); await state.clean(); }
});

test('Hub file batches are capped at four owned tasks and publish only verified unit artifacts', { timeout: 30_000 }, async () => {
  const state = await fixture('hub', 6);
  try {
    const id = await start(state.harness, state.prepared);
    const settled = await state.harness.waitForTask(id, context);
    assert.equal(settled.state.outcome.status, 'completed');
    if (settled.state.outcome.status !== 'completed') throw new Error(JSON.stringify(settled.state.outcome));
    const events = await state.events(); let active = 0; let maximum = 0;
    for (const event of events) {
      if (event.event === 'start') maximum = Math.max(maximum, ++active);
      if (event.event === 'end') active--;
    }
    assert.equal(active, 0); assert.ok(maximum >= 2 && maximum <= 4);
    const fourthDone = Math.max(...events.filter(event => event.event === 'end' && event.index < 4).map(event => event.at));
    assert.ok(events.find(event => event.event === 'start' && event.index === 4)!.at >= fourthDone);
    const manifest = JSON.parse(await readFile(settled.state.outcome.result.manifestPath, 'utf8'));
    assert.equal(manifest.files.length, 6);
    assert.equal(await readFile(join(manifest.source_root, 'data/5.bin'), 'utf8'), 'data');
  } finally { await state.harness.close(context); await state.clean(); }
});

test('an allSettled batch retains every unit outcome and never publishes a partial acquisition', { timeout: 30_000 }, async () => {
  const state = await fixture('hub', 6, true);
  try {
    const id = await start(state.harness, state.prepared);
    const settled = await state.harness.waitForTask(id, context);
    assert.equal(settled.state.outcome.status, 'failed');
    assert.doesNotMatch(JSON.stringify(settled.state.outcome), /source-secret|private\.example|controlled-source\.py/);
    const children = await state.harness.commit(tx => tx.scanTasks({ kind: SourceAcquisitionUnitTask.definition.name }, 100), context);
    assert.equal(children.items.length, 4);
    assert.ok(children.items.every(child => child.state.status === 'terminal'));
    assert.doesNotMatch(JSON.stringify(children.items), /source-secret|private\.example/);
    assert.equal(children.items.filter(child => child.state.status === 'terminal' && child.state.outcome.status === 'completed').length, 3);
    const events = await state.events();
    assert.equal(events.filter(event => event.event === 'end').length, 3);
    assert.equal(events.some(event => event.event === 'publish' || event.index >= 4), false);
    await assert.rejects(readFile(join(state.directory, 'published', 'market-data.json')), { code: 'ENOENT' });
  } finally { await state.harness.close(context); await state.clean(); }
});

for (const kind of ['http', 'snapshot'] as const) test(`the owner ${kind} source refresh executes the acquisition task graph and publishes only manifest references`, { timeout: 30_000 }, async () => {
  const source = await fixture(kind); const owner = await setup();
  const previous = process.env.QZ_SOURCE_UNIT_TOOL;
  try {
    const planPath = join(owner.directory, 'owner-plan.json');
    await writeFile(planPath, JSON.stringify(source.plan));
    const url = '/api/v3/data-sources/qz.data.acquisition-plan';
    const saved = await owner.service.app.inject({ method: 'PUT', url, headers: owner.headers,
      payload: { enabled: true, config: { kind, planPath, ...(kind === 'http' ? { termsFile: source.terms } : {}) } } });
    assert.equal(saved.statusCode, 200, saved.body);
    process.env.QZ_SOURCE_UNIT_TOOL = source.prepared.toolPath;
    const requested = await owner.service.app.inject({ method: 'POST', url: `${url}/refresh`, headers: owner.headers });
    assert.equal(requested.statusCode, 202, requested.body);
    const refreshId = requested.json().taskId as TaskId;
    const refreshed = await owner.service.research.harness.waitForTask(refreshId, context);
    assert.equal(refreshed.state.outcome.status, 'completed');
    if (refreshed.state.outcome.status !== 'completed') throw new Error(JSON.stringify(refreshed.state.outcome));
    const document = await owner.service.research.harness.snapshot(DataSourceState, context);
    const provider = document!.sources['qz.data.acquisition-plan'] as JsonObject;
    const acquired = provider.acquisition as { manifestPath: string; planId: string };
    assert.deepEqual(Object.keys(acquired).sort(), ['manifestPath', 'planId']);
    const manifest = JSON.parse(await readFile(acquired.manifestPath, 'utf8'));
    if (kind === 'http') assert.equal(manifest.record_count, 3);
    else {
      assert.ok(acquired.manifestPath.endsWith('/snapshot.json'));
      assert.equal(manifest.files.length, 3); assert.equal(manifest.revision, 'a'.repeat(40));
      for (const file of manifest.files) assert.equal(file.sha256, createHash('sha256').update('data').digest('hex'));
      assert.equal(await readFile(join(acquired.manifestPath, '..', 'data/0.bin'), 'utf8'), 'data');
      assert.equal(await readFile(join(acquired.manifestPath, '..', 'data/1.bin'), 'utf8'), 'data');
    }
    assert.deepEqual(provider.datasets, []);
    assert.deepEqual(owner.service.config.datasets.map(dataset => dataset.id), ['controlled']);
    const acquisitions = await owner.service.research.harness.commit(tx => tx.scanTasks({ kind: SourceAcquisitionTask.definition.name }, 100), context);
    assert.equal(acquisitions.items.length, 1); assert.equal(acquisitions.items[0]!.owner, refreshId);
    const units = await owner.service.research.harness.commit(tx => tx.scanTasks({ kind: SourceAcquisitionUnitTask.definition.name }, 100), context);
    assert.equal(units.items.length, 3);
    assert.ok(units.items.every(unit => unit.owner === acquisitions.items[0]!.id && unit.state.status === 'terminal'));
    assert.deepEqual((await source.events()).filter(event => event.event === 'start').map(event => event.index).sort((a, b) => a - b), [0, 1, 2]);
  } finally {
    if (previous === undefined) delete process.env.QZ_SOURCE_UNIT_TOOL; else process.env.QZ_SOURCE_UNIT_TOOL = previous;
    await owner.cleanup(); await source.harness.close(context); await source.clean();
  }
});
