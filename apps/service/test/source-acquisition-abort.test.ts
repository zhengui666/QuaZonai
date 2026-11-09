// Cancellation protocol against real Python processes, not a live provider test.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { watch } from 'node:fs';
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { createRegistry, Harness } from '@earendil-works/pi-durable';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import { prepareSourceAcquisition, sourceAcquisitionExtension, SourceAcquisitionTask, SourceAcquisitionUnitTask } from '../src/source-acquisition.ts';

const context = BACKGROUND_CONTEXT;

test('aborting the acquisition drains owned units and terminates their active Python processes', { timeout: 20_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-acquisition-abort-'));
  const started = join(directory, 'started'); await mkdir(started);
  const unexpected = join(directory, 'unexpected-publication');
  const tool = join(directory, 'controlled-blocking-source.py');
  await writeFile(tool, `import sys,json,os,time\nfrom pathlib import Path\ncommand=sys.argv[1]\nif command=='plan':\n    identity='a'*64\n    print(json.dumps({'schemaVersion':1,'kind':'hub','planId':identity,'concurrency':4,'minIntervalMs':0,'units':[{'id':identity+'-'+str(i),'index':i} for i in range(3)]}))\nelif command=='execute':\n    index=sys.argv[sys.argv.index('--unit-index')+1]\n    Path(${JSON.stringify(started)},index).write_text(str(os.getpid()))\n    while True: time.sleep(1)\nelse:\n    Path(${JSON.stringify(unexpected)}).write_text('publish must never run')\n`);
  const previous = process.env.QZ_SOURCE_UNIT_TOOL;
  let harness: Harness | undefined;
  try {
    process.env.QZ_SOURCE_UNIT_TOOL = tool;
    const prepared = await prepareSourceAcquisition({ kind: 'hub', frozenPlan: { controlled: true },
      workdir: join(directory, 'units'), output: join(directory, 'published'), cacheDir: join(directory, 'cache') });
    if (previous === undefined) delete process.env.QZ_SOURCE_UNIT_TOOL; else process.env.QZ_SOURCE_UNIT_TOOL = previous;
    const registry = createRegistry(); registry.install(sourceAcquisitionExtension);
    harness = await Harness.open(await openNodeSqliteStorage(join(directory, 'tasks.sqlite')), { registry, models: createModels() }, context);
    const owner = await harness.root(context);
    const parent = await owner.commit(tx => tx.createTask(SourceAcquisitionTask, prepared, { ownership: { kind: 'conversation' } }), context);
    let signalStarted!: () => void; const allStarted = new Promise<void>(resolve => { signalStarted = resolve; });
    const checkStarted = async () => {
      if ((await readdir(started)).length !== 3) return;
      const pids = await Promise.all(['0', '1', '2'].map(async index => Number(await readFile(join(started, index), 'utf8'))));
      if (pids.every(pid => Number.isSafeInteger(pid) && pid > 0)) signalStarted();
    };
    const observer = watch(started, () => { void checkStarted(); });
    try { harness.resume(); await checkStarted(); await allStarted; } finally { observer.close(); }
    const pids = await Promise.all(['0', '1', '2'].map(async index => Number(await readFile(join(started, index), 'utf8'))));
    for (const pid of pids) assert.doesNotThrow(() => process.kill(pid, 0));
    await harness.abortTask(parent, context);
    const settled = await harness.waitForTask(parent, context);
    assert.equal(settled.state.outcome.status, 'aborted');
    const units = await harness.commit(tx => tx.scanTasks({ kind: SourceAcquisitionUnitTask.definition.name }, 100), context);
    assert.equal(units.items.length, 3);
    for (const unit of units.items) {
      assert.equal(unit.owner, parent); assert.equal(unit.state.status, 'terminal');
      if (unit.state.status === 'terminal') assert.equal(unit.state.outcome.status, 'aborted');
    }
    for (const pid of pids) assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
    await assert.rejects(readFile(unexpected), { code: 'ENOENT' });
    await assert.rejects(readFile(join(prepared.output, 'market-data.json')), { code: 'ENOENT' });
  } finally {
    if (previous === undefined) delete process.env.QZ_SOURCE_UNIT_TOOL; else process.env.QZ_SOURCE_UNIT_TOOL = previous;
    await harness?.close(context); await rm(directory, { recursive: true, force: true });
  }
});
