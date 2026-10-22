import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import test from 'node:test';
import { configure } from './configure.mjs';
import { context, requireIdle } from './maintenance.mjs';
const require = createRequire(new URL('../../apps/service/package.json', import.meta.url));
const { openNodeSqliteStorage } = await import(require.resolve('@earendil-works/pi-durable/storage/sqlite/node'));

async function fixture(t) {
  const root = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-pm-maintenance-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const config = configure(root, 'http://localhost:8081');
  const database = JSON.parse(await readFile(config, 'utf8')).research.database;
  const storage = await openNodeSqliteStorage(database);
  const id = await storage.mintId();
  const value = { mandates: { 'mandate:original': {
    mandate: { id: 'mandate:original', revision: '7', capital: '3000.000000000000000001' },
    commands: {}, outcomes: {}, retirements: {},
  } }, datasets: {}, commands: { 'request:original': { mandateId: 'mandate:original', resourceId: 'cycle:original' } } };
  await storage.commit([{ type: 'document.create', record: { id, kind: 'qz.pm', scope: { kind: 'session' } }, content: { kind: 'base', version: 1, value } }], context);
  await storage.close(context);
  async function read() {
    const storage = await openNodeSqliteStorage(database);
    try { return await storage.document(id, 'current', context); }
    finally { await storage.close(context); }
  }
  async function replace(value) {
    const storage = await openNodeSqliteStorage(database);
    try { await storage.commit([{ type: 'document.change', id, content: { kind: 'base', version: 1, value } }], context); }
    finally { await storage.close(context); }
  }
  return { root, config, database, id, value, read, replace };
}

async function quiescedFixture(t, monitorCount = 1) {
  const state = await fixture(t); const storage = await openNodeSqliteStorage(state.database);
  const conversationId = await storage.mintId(); const gateId = await storage.mintId(); const appId = await storage.mintId();
  const gate = { id: gateId, conversationId, kind: 'qz.maintenance.gate', version: 1, input: { epoch: 'original-epoch' }, background: true,
    abortRequested: false, state: { status: 'running', checkpoint: { phase: 'hold' } } };
  const maintenance = { schemaVersion: 1, epoch: 'original-epoch', state: 'QUIESCED', barrierTaskId: gateId,
    enteredAtNs: '100', quiescedAtNs: '101', resumedAtNs: null, parked: {}, commands: {} };
  const tasks = [];
  for (let index = 0; index < monitorCount; index++) {
    const id = await storage.mintId(); const kind = index % 2 ? 'qz.account-adjustment' : 'qz.pm.mandate-monitor';
    const resumeCheckpoint = { phase: 'wait', wakeAt: 1234 + index };
    tasks.push({ id, conversationId, kind, version: 1, input: { original: 'controlled-original-input' }, background: true, abortRequested: false,
      state: { status: 'waiting', on: [gateId], policy: 'allSettled', checkpoint: { phase: 'maintenance', maintenanceEpoch: maintenance.epoch, resumeCheckpoint } } });
    maintenance.parked[String(id)] = { taskId: id, kind, epoch: maintenance.epoch, resumeCheckpoint };
  }
  const application = { observations: {}, accountControls: {}, maintenance };
  await storage.commit([{ type: 'conversation', value: { id: conversationId } }, { type: 'task', value: gate }, ...tasks.map(value => ({ type: 'task', value })),
    { type: 'document.create', record: { id: appId, kind: 'qz.application', scope: { kind: 'session' } }, content: { kind: 'base', version: 1, value: application } }], context);
  await storage.close(context);
  async function replace(application, gate, tasks) {
    const storage = await openNodeSqliteStorage(state.database);
    try { await storage.commit([{ type: 'document.change', id: appId, content: { kind: 'base', version: 1, value: application } },
      { type: 'task', value: gate }, ...tasks.map(value => ({ type: 'task', value }))], context); }
    finally { await storage.close(context); }
  }
  async function read() {
    const storage = await openNodeSqliteStorage(state.database);
    try { return { application: (await storage.document(appId, 'current', context)).value, gate: await storage.task(gateId, context),
      tasks: await Promise.all(tasks.map(task => storage.task(task.id, context))) }; }
    finally { await storage.close(context); }
  }
  return { ...state, application, gate, tasks, updateQuiesced: replace, readQuiesced: read };
}

test('PM idle gate reads real official storage and retains unresolved external facts after local tasks end', async t => {
  const state = await fixture(t);
  await requireIdle(state.config);
  for (const [field, fact, error] of [
    ['commands', { id: 'command:original', state: 'PENDING' }, /Unresolved PM target command/],
    ['commands', { id: 'command:original', state: 'UNCERTAIN' }, /Unresolved PM target command/],
    ['outcomes', { id: 'outcome:original', receiptState: 'PENDING' }, /Unresolved PM outcome receipt/],
    ['outcomes', { id: 'outcome:original', receiptState: 'UNCERTAIN' }, /Unresolved PM outcome receipt/],
    ['retirements', { id: 'retirement:original', reconciliationRequired: true }, /Unresolved PM retirement/],
  ]) {
    const pending = structuredClone(state.value);
    pending.mandates['mandate:original'][field].original = fact;
    await state.replace(pending);
    await assert.rejects(requireIdle(state.config), error);
    assert.deepEqual((await state.read()).value, pending, 'maintenance must not acknowledge, cancel, or rewrite the fact');
  }
  const reconciled = structuredClone(state.value);
  reconciled.mandates['mandate:original'].commands.original = { id: 'command:original', state: 'ACKNOWLEDGED' };
  reconciled.mandates['mandate:original'].outcomes.original = { id: 'outcome:original', receiptState: 'ACKNOWLEDGED' };
  reconciled.mandates['mandate:original'].retirements.original = { id: 'retirement:original', reconciliationRequired: false };
  await state.replace(reconciled);
  await requireIdle(state.config);
  assert.deepEqual((await state.read()).value, reconciled);
});

test('an original command receipt resolves historical uncertainty without rewriting the observed outcome', async t => {
  const state = await fixture(t);
  const history = structuredClone(state.value);
  const mandate = history.mandates['mandate:original'];
  mandate.outcomes.original = { id: 'outcome:original', decisionId: 'decision:original', targetCommandId: 'command:original', receiptState: 'UNCERTAIN' };
  mandate.commands['command:original'] = { id: 'command:original', decisionId: 'different-decision', state: 'ACKNOWLEDGED' };
  await state.replace(history);
  await assert.rejects(requireIdle(state.config), /Unresolved PM outcome receipt/, 'Another decision cannot resolve the historical receipt.');
  mandate.commands['command:original'].decisionId = 'decision:original';
  for (const status of ['ACKNOWLEDGED', 'REJECTED', 'NOT_OFFERED']) {
    mandate.commands['command:original'].state = status;
    await state.replace(history); await requireIdle(state.config);
    assert.deepEqual((await state.read()).value, history, 'The original historical uncertainty is retained.');
  }
  delete mandate.commands['command:original'];
  await state.replace(history);
  await assert.rejects(requireIdle(state.config), /Unresolved PM outcome receipt/, 'Missing current command evidence cannot be assumed resolved.');
});

test('only formally quiesced original monitor tasks with an exact epoch, gate and recovery checkpoint pass maintenance', async t => {
  const state = await quiescedFixture(t);
  const original = await state.readQuiesced(); await requireIdle(state.config);
  assert.deepEqual(await state.readQuiesced(), original);
  const changes = [
    ['draining', value => { value.application.maintenance.state = 'DRAINING'; }],
    ['running intent', value => { value.application.maintenance.state = 'RUNNING'; }],
    ['missing timestamp', value => { value.application.maintenance.quiescedAtNs = null; }],
    ['other gate epoch', value => { value.gate.input.epoch = 'other-epoch'; }],
    ['gate executing business', value => { value.gate.state.checkpoint.phase = 'deliver'; }],
    ['aborting gate', value => { value.gate.abortRequested = true; }],
    ['other task epoch', value => { value.tasks[0].state.checkpoint.maintenanceEpoch = 'other-epoch'; }],
    ['different recorded resume point', value => { value.application.maintenance.parked[String(value.tasks[0].id)].resumeCheckpoint.wakeAt++; }],
    ['other join policy', value => { value.tasks[0].state.policy = 'allCompleted'; }],
    ['other dependency', value => { value.tasks[0].state.on = [value.tasks[0].id]; }],
    ['unknown registered kind', value => { value.tasks[0].kind = 'unknown-business-task'; value.application.maintenance.parked[String(value.tasks[0].id)].kind = value.tasks[0].kind; }],
    ['foreground task', value => { value.tasks[0].background = false; }],
    ['aborting task', value => { value.tasks[0].abortRequested = true; }],
    ['missing registration', value => { value.application.maintenance.parked = {}; }],
    ['missing original task', value => { value.application.maintenance.parked['999999'] = { taskId: 999999, kind: 'qz.pm.mandate-monitor', epoch: 'original-epoch', resumeCheckpoint: { phase: 'start' } }; }],
  ];
  for (const [name, mutate] of changes) {
    const next = structuredClone(original); mutate(next);
    await state.updateQuiesced(next.application, next.gate, next.tasks);
    await assert.rejects(requireIdle(state.config), /Maintenance is not formally QUIESCED|Unresolved durable task|Incomplete maintenance epoch/, name);
    assert.deepEqual(await state.readQuiesced(), next, `${name}: rejected maintenance must leave original facts and tasks unchanged`);
  }
  await state.updateQuiesced(original.application, original.gate, original.tasks); await requireIdle(state.config);
});

test('quiescence never hides an unknown task on a later official storage page', async t => {
  const state = await quiescedFixture(t, 101);
  await requireIdle(state.config);
  const value = await state.readQuiesced(); value.tasks[100].kind = 'unknown-late-page-task';
  value.application.maintenance.parked[String(value.tasks[100].id)].kind = 'unknown-late-page-task';
  await state.updateQuiesced(value.application, value.gate, value.tasks);
  await assert.rejects(requireIdle(state.config), /Unresolved durable task/);
  assert.deepEqual(await state.readQuiesced(), value);
});

test('a formally quiesced monitor cannot discard an original queued submission', async t => {
  const state = await quiescedFixture(t); const storage = await openNodeSqliteStorage(state.database);
  const id = await storage.mintId();
  const submission = { id, conversationId: state.gate.conversationId, requestId: 'original-queued-request', type: 'input', status: 'queued' };
  try { await storage.commit([{ type: 'submission', value: submission }], context); }
  finally { await storage.close(context); }
  await assert.rejects(requireIdle(state.config), /Unresolved research submission/);
  const reopened = await openNodeSqliteStorage(state.database);
  try { assert.deepEqual(await reopened.submission(id, context), submission); }
  finally { await reopened.close(context); }
});

test('quiesced monitors cannot hide unresolved automatic allocation or capital-control receipts', async t => {
  const state = await quiescedFixture(t); const original = await state.readQuiesced();
  for (const kind of ['allocation-receipt', 'allocation-state', 'capital-command']) {
    const value = structuredClone(original);
    if (kind === 'capital-command') value.application.accountControls.account = { commands: { original: { state: 'UNCERTAIN' } } };
    else value.application.observations.original = { view: { id: 'original', status: 'OBSERVING', receipt: { status: 'ACKNOWLEDGED' },
      allocation: kind === 'allocation-receipt' ? { status: 'OBSERVING', receipt: { status: 'UNCERTAIN' } } : { status: 'PENDING', receipt: null } } };
    await state.updateQuiesced(value.application, value.gate, value.tasks);
    await assert.rejects(requireIdle(state.config), /Unresolved downstream allocation|Unresolved capital command/);
    assert.deepEqual(await state.readQuiesced(), value);
  }
});

test('stopped PM SQLite and referenced artifact restore preserve original document IDs, exact capital and command replay facts', async t => {
  const state = await fixture(t);
  const bundle = join(state.root, 'bundle'); const work = join(state.root, 'work');
  await mkdir(bundle); await mkdir(work); await mkdir(join(state.root, 'data/artifacts'));
  const artifact = join(state.root, 'data/artifacts/original.json');
  const originalArtifact = '{"basisRevision":"basis:original","capital":"3000.000000000000000001"}\n';
  await writeFile(artifact, originalArtifact);
  const image = `sha256:${'a'.repeat(64)}`;
  const installation = { schema_version: 4, version: 'v0.0.0-ci.1', revision: 'b'.repeat(40), root: state.root,
    uid: process.getuid(), project: 'pm-restore-test', password: 'test-only', image, runtime_image: image, database_image: image, bundle };
  await writeFile(join(bundle, 'release.json'), JSON.stringify(installation));
  await writeFile(join(state.root, 'installation.json'), JSON.stringify(installation));
  // SQLite, tar archives, recovery markers and replacement are real. Docker/PG
  // process calls and app activation are fixtures, not a deployed-image claim.
  const shell = (body, backup = '') => spawnSync('bash', ['-c', `source "$1"
ROOT=$2; QZ_WORK="$ROOT/work"
qz_stop() { :; }; qz_idle() { :; }; qz_start() { :; }
qz_compose() { case " $* " in
 *" pg_dump "*) printf 'isolated test-only PG boundary';;
 *" pg_restore "*) cat >/dev/null;;
 *) return 1;; esac; }
${body}`, 'qz-pm-restore', fileURLToPath(new URL('./manage.sh', import.meta.url)), state.root, backup], { encoding: 'utf8', timeout: 20_000 });
  await requireIdle(state.config);
  let result = shell('qz_backup "$ROOT/installation.json"; printf %s "$QZ_BACKUP" > "$ROOT/selected-backup"');
  assert.equal(result.status, 0, result.stderr);
  const backup = await readFile(join(state.root, 'selected-backup'), 'utf8');
  const newer = structuredClone(state.value); newer.mandates['mandate:original'].mandate.revision = '8';
  await state.replace(newer); await writeFile(artifact, 'newer artifact to preserve before restore');
  await requireIdle(state.config);
  result = shell('qz_restore "$ROOT" "$3"', backup);
  assert.equal(result.status, 0, result.stderr);
  const restored = await state.read();
  assert.equal(restored.record.id, state.id);
  assert.deepEqual(restored.value, state.value);
  assert.equal(await readFile(artifact, 'utf8'), originalArtifact);
  await requireIdle(state.config);
});
