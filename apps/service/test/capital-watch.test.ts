import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:http';
import Fastify from 'fastify';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { createRegistry, defineExtension, Harness } from '@earendil-works/pi-durable';
import type { TaskId } from '@earendil-works/pi-durable';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import type { CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';
import type { ConnectionConfig } from '../src/config.ts';
import { accountKey, control, readCapitalControl, recordCapitalControl, transportBindingKey } from '../src/capital-exit.ts';
import { Application, json, nowNs } from '../src/facts.ts';
import { CAPITAL_WATCH_INTERVAL_MS, CAPITAL_WATCH_READ_TIMEOUT_MS, CapitalExitWatch, capitalWatchNeeded, ensureCapitalWatch } from '../src/capital-watch.ts';

const context = BACKGROUND_CONTEXT;
const money = (amount: string) => ({ amount, currency: 'USDC' });
function exitView(): CapitalExitView {
  return {
    schema_version: 2, id: 'original-exit', revision: '1', account_source_id: 'native-source', environment: 'PAPER', state: 'READY', preparation_state: 'READY',
    account_control_revision: '7', command_id: 'original-command', reason_codes: [], policy: { kind: 'NATURAL_RELEASE', deadline: new Date(Date.now() + 60_000).toISOString() },
    scope: { kind: 'AMOUNT', amount: '100', currency: 'USDC' }, remaining_trading: 'BUDGET_AWARE',
    funds: { requested_amount: money('100'), reserved_amount: money('80'), staged_for_withdrawal_amount: money('20'), waived_amount: money('0'),
      released_cash_amount: money('80'), verified_withdrawable_amount: money('20'), reconciled_withdrawal_amount: money('0'), withdrawability: 'VERIFIED',
      evidence_asof: new Date().toISOString(), evidence_valid_until: new Date(Date.now() + 60_000).toISOString() },
    action_availability: {}, progress: [], events: [], created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
  };
}
async function until(read: () => Promise<boolean>, message: string, duration = 12_000) {
  const deadline = Date.now() + duration;
  while (Date.now() < deadline) {
    if (await read()) return;
    await new Promise(resolve => setTimeout(resolve, 15));
  }
  assert.fail(message);
}

test('capital preparation READY retains external reconciliation responsibility', () => {
  assert.equal(CAPITAL_WATCH_INTERVAL_MS, 5000);
  assert.equal(capitalWatchNeeded({ commands: {}, status: { capability: 'SUPPORTED', reason: null, accountSourceId: 's', observationId: 'o', controlRevision: '7', reserved: '80', exits: [exitView()] } }), true);
  const closed = exitView(); closed.state = 'CLOSED'; closed.preparation_state = 'CLOSED';
  assert.equal(capitalWatchNeeded({ commands: {}, status: { capability: 'SUPPORTED', reason: null, accountSourceId: 's', observationId: 'o', controlRevision: '8', reserved: '0', exits: [closed] } }), false);
  assert.equal(capitalWatchNeeded({ commands: { original: { original: {}, body: {}, path: 'capital-exits', reserved: '0', state: 'UNCERTAIN', result: null, error: null, taskId: 1 } } }), true);
});

test('an account-owned reader survives SQLite reopen, aliases and detached observations without financial POSTs', { timeout: 30_000 }, async () => {
  // The actual owner HTTP and Durable/SQLite paths below verify monitoring and lifetime, not exchange execution.
  const directory = await mkdtemp(join(tmpdir(), 'qz-capital-watch-')); const database = join(directory, 'account.sqlite');
  const remote = Fastify(); const view = exitView(); let revision = '7'; let reads = 0; const posts: string[] = [];
  const envelope = { clientId: 'original-client', accountId: 'original-account', mode: 'paper' as const, currency: 'USDC' };
  const snapshot = { ...envelope, accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: revision,
    sequence: '1', nativeClockNs: nowNs(), equity: '1000', cash: '100', positions: [] };
  remote.get('/capabilities', () => ({ ...envelope, capitalExit: { version: 2, supported: true, readable: true,
    policies: ['CASH_ONLY', 'NATURAL_RELEASE', 'BOUNDED_LIMIT'], actions: ['START', 'PAUSE', 'CANCEL', 'RESUME', 'RELEASE_RESERVATION', 'RECONCILE_WITHDRAWAL'] } }));
  remote.get('/capital-exits', () => { reads++; return { ...envelope, accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: revision,
    reserved: view.funds.reserved_amount.amount, exits: [view] }; });
  remote.get('/account', () => snapshot);
  remote.post('/*', request => { posts.push(request.url); throw new Error('A capital watcher cannot submit a financial command.'); });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  const connection: ConnectionConfig = { id: 'original-binding', name: 'Original account', url, ...envelope };
  const alias: ConnectionConfig = { ...connection, id: 'alias-binding' };
  async function open() {
    const registry = createRegistry(); registry.install(defineExtension({ name: 'test.capital-account-reader', tasks: [CapitalExitWatch] }));
    const storage = await openNodeSqliteStorage(database);
    const harness = await Harness.open(storage, { models: createModels(), registry, settings: { extensions: registry.snapshot().installed() } }, context);
    harness.resume(); return harness;
  }
  let harness = await open();
  try {
    await harness.commit(async tx => {
      const app = await tx.doc(Application); app.accountControls = {};
      const key = accountKey(connection); app.accountControls[key] = json({ commands: {}, bindings: [key],
        status: { capability: 'SUPPORTED', reason: null, accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: '7', reserved: '80', exits: [view] } });
    }, context);
    view.revision = '2'; view.events.push({ id: 'native-progress', at: new Date().toISOString(), kind: 'AVAILABILITY', message: 'The original account refreshed availability.', reference: 'original-native-report' });
    const first = await harness.commit(tx => ensureCapitalWatch(tx, connection, 'USDC'), context);
    const second = await harness.commit(tx => ensureCapitalWatch(tx, alias, 'USDC'), context);
    assert.equal(first.taskId, second.taskId); assert.equal(first.conversationId, second.conversationId);
    const task = await harness.getTask(first.taskId, context); assert.equal(task!.owner, undefined); assert.equal(task!.conversationId, first.conversationId); assert.equal(task!.background, true);
    const owner = await harness.commit(tx => tx.conversation(first.conversationId), context); assert.equal(owner!.owner, undefined);
    await until(async () => {
      const app = await harness.snapshot(Application, context); const saved = app?.accountControls?.[first.key];
      return !!saved?.watchSnapshot && control(saved).status?.exits[0]?.revision === '2';
    }, 'The account reader must record native progress without changing the financial control epoch or requiring a portfolio/browser.');
    const before = (await harness.snapshot(Application, context))!; assert.deepEqual(before.observations, {}); assert.deepEqual(before.portfolios, {});
    assert.equal(control(before.accountControls![first.key]).status!.exits[0]!.state, 'READY');
    const firstRead = reads;
    await harness.close(context); harness = await open();
    await until(async () => reads > firstRead, 'Reopening the same SQLite store must resume the original account reader.');
    const saved = (await harness.snapshot(Application, context))!; assert.equal(saved.accountControls![first.key]!.monitorTaskId, first.taskId);
    revision = '8'; snapshot.controlRevision = revision; snapshot.sequence = '2'; snapshot.nativeClockNs = nowNs();
    view.state = 'CLOSED'; view.preparation_state = 'CLOSED'; view.revision = '3'; view.account_control_revision = revision;
    view.funds.reserved_amount = money('0'); view.funds.staged_for_withdrawal_amount = money('0'); view.funds.waived_amount = money('100');
    view.funds.released_cash_amount = money('0'); view.funds.verified_withdrawable_amount = null; view.funds.withdrawability = 'UNVERIFIED';
    await until(async () => (await harness.getTask(first.taskId as TaskId, context))?.state.status === 'terminal', 'The reader ends after the authoritative account reports all obligations closed.');
    assert.deepEqual(posts, []);
    assert.equal(control((await harness.snapshot(Application, context))!.accountControls![first.key]).status!.reserved, '0');
  } finally { await harness.close(context); await remote.close(); await rm(directory, { recursive: true, force: true }); }
});

test('the original reader follows a verified replacement binding after credential failure and ignores a late old response', { timeout: 40_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-capital-binding-')); const remote = Fastify(); const view = exitView();
  let failOld = false; let holdOld = false; let oldHeld = false; let releaseOld = () => {}; let replacementAccounts = 0; const posts: string[] = [];
  const snapshot = { accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: '7', sequence: '1', nativeClockNs: nowNs(), equity: '1000', cash: '100', positions: [] };
  for (const prefix of ['/original', '/replacement']) {
    const original = prefix === '/original';
    const envelope = { clientId: original ? 'original-client' : 'replacement-client', accountId: original ? 'original-account' : 'replacement-account', mode: 'paper' as const, currency: 'USDC' };
    remote.get(prefix + '/capabilities', (request, reply) => {
      if (original && failOld || request.headers.authorization !== 'Bearer ' + (original ? 'original-credential' : 'replacement-credential')) return reply.code(403).send({ error: 'credential_revoked' });
      return { ...envelope, capitalExit: { version: 2, supported: true, readable: true, policies: ['CASH_ONLY', 'NATURAL_RELEASE', 'BOUNDED_LIMIT'], actions: ['START', 'PAUSE', 'CANCEL'] } };
    });
    remote.get(prefix + '/capital-exits', async (_request, reply) => {
      if (original && failOld) return reply.code(403).send({ error: 'credential_revoked' });
      if (original && holdOld) { oldHeld = true; await new Promise<void>(resolve => { releaseOld = resolve; }); }
      return { ...envelope, ...snapshot, reserved: '80', exits: [view] };
    });
    remote.get(prefix + '/account', (_request, reply) => {
      if (original && failOld) return reply.code(403).send({ error: 'credential_revoked' });
      if (!original) replacementAccounts++;
      return { ...envelope, ...snapshot };
    });
  }
  remote.post('/*', request => { posts.push(request.url); throw new Error('Changing the monitoring binding cannot initiate a command.'); });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  const original: ConnectionConfig = { id: 'original', name: 'Original', url: url + '/original', clientId: 'original-client', accountId: 'original-account', mode: 'paper', token: 'original-credential' };
  const replacement: ConnectionConfig = { id: 'replacement', name: 'Replacement', url: url + '/replacement', clientId: 'replacement-client', accountId: 'replacement-account', mode: 'paper', token: 'replacement-credential' };
  const registry = createRegistry(); registry.install(defineExtension({ name: 'test.capital-binding-reader', tasks: [CapitalExitWatch] }));
  const harness = await Harness.open(await openNodeSqliteStorage(join(directory, 'account.sqlite')), { models: createModels(), registry, settings: { extensions: registry.snapshot().installed() } }, context); harness.resume();
  try {
    const fresh = await readCapitalControl(original, 'USDC');
    await harness.commit(async tx => { const app = await tx.doc(Application); app.accountControls = {}; recordCapitalControl(app.accountControls, original, fresh); }, context);
    const owner = await harness.commit(tx => ensureCapitalWatch(tx, original, 'USDC'), context);
    await until(async () => control((await harness.snapshot(Application, context))?.accountControls?.[owner.key]).watchSnapshot?.clientId === original.clientId,
      'The original binding must first produce a saved physical observation.');
    failOld = true;
    await until(async () => {
      const app = (await harness.snapshot(Application, context))!;
      return !!control(app.accountControls![owner.key]).refreshFailuresByBinding?.[transportBindingKey(original)];
    }, 'A revoked monitoring credential must retain a visible read failure.');

    // A successful old read started before the replacement was verified is held until after that change.
    failOld = false; holdOld = true;
    await until(async () => oldHeld, 'The old transport must have an in-flight read before the replacement is recorded.');
    const verifiedReplacement = await readCapitalControl(replacement, 'USDC');
    await harness.commit(async tx => {
      const app = await tx.doc(Application); recordCapitalControl(app.accountControls!, replacement, verifiedReplacement);
    }, context);
    failOld = true; holdOld = false; releaseOld();
    await until(async () => {
      const app = (await harness.snapshot(Application, context))!; const saved = control(app.accountControls![owner.key]);
      return replacementAccounts > 0 && saved.watchSnapshot?.clientId === replacement.clientId;
    }, 'Without a browser or another ensure call, the same task must switch to the last verified account binding.');
    const app = (await harness.snapshot(Application, context))!; const saved = control(app.accountControls![owner.key]);
    assert.equal(saved.monitorTaskId, owner.taskId); assert.equal(saved.monitorConnection?.url, replacement.url);
    assert.equal(saved.watchSnapshot!.sequence, '1', 'A transport rename preserves the original physical account sequence.');
    assert.equal(saved.status!.reserved, '80'); assert.equal(saved.refreshFailuresByBinding?.[transportBindingKey(replacement)], undefined);
    assert.deepEqual((await harness.getTask(owner.taskId, context))!.input, { connection: original, currency: 'USDC' }, 'The task identity and original input are not rewritten.');
    assert.deepEqual(posts, []);
  } finally { releaseOld(); await harness.close(context); await remote.close(); await rm(directory, { recursive: true, force: true }); }
});

test('bounded read rounds abort missing headers and unfinished bodies, retain reservations and resume the same task', { timeout: 20_000 }, async t => {
  assert.equal(CAPITAL_WATCH_READ_TIMEOUT_MS, 10_000);
  const requestedTimeouts: number[] = []; const realTimeout = AbortSignal.timeout.bind(AbortSignal);
  t.mock.method(AbortSignal, 'timeout', (milliseconds: number) => {
    requestedTimeouts.push(milliseconds); return realTimeout(milliseconds === 10_000 ? 80 : milliseconds);
  });
  const directory = await mkdtemp(join(tmpdir(), 'qz-capital-read-timeout-')); const view = exitView();
  const envelope = { clientId: 'timeout-client', accountId: 'timeout-account', mode: 'paper' as const, currency: 'USDC' };
  const snapshot = { ...envelope, accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: '7', sequence: '1', nativeClockNs: nowNs(), equity: '1000', cash: '100', positions: [] };
  let hang = true; let interrupted = 0; let posts = 0;
  const server = createServer((request, response) => {
    if (request.method === 'POST') { posts++; response.writeHead(405).end(); return; }
    response.on('close', () => { if (!response.writableEnded) interrupted++; });
    if (request.url === '/capabilities') {
      response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ ...envelope, capitalExit: { version: 2, supported: true, readable: true, policies: ['CASH_ONLY'], actions: ['START', 'PAUSE', 'CANCEL'] } })); return;
    }
    if (request.url === '/capital-exits') {
      if (hang) { response.writeHead(200, { 'content-type': 'application/json' }); response.write('{"clientId":'); return; }
      response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ ...envelope, accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: '7', reserved: '80', exits: [view] })); return;
    }
    if (request.url === '/account') {
      if (hang) return;
      response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify(snapshot)); return;
    }
    response.writeHead(404).end();
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const connection: ConnectionConfig = { id: 'timeout-binding', name: 'Bounded source', url: 'http://127.0.0.1:' + address.port, clientId: envelope.clientId, accountId: envelope.accountId, mode: 'paper' };
  const registry = createRegistry(); registry.install(defineExtension({ name: 'test.capital-bounded-reader', tasks: [CapitalExitWatch] }));
  const harness = await Harness.open(await openNodeSqliteStorage(join(directory, 'account.sqlite')), { models: createModels(), registry, settings: { extensions: registry.snapshot().installed() } }, context); harness.resume();
  try {
    await harness.commit(async tx => {
      const app = await tx.doc(Application); app.accountControls = {};
      app.accountControls[accountKey(connection)] = json({ commands: {}, status: { capability: 'SUPPORTED', reason: null, accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: '7', reserved: '80', exits: [view] } });
    }, context);
    const owner = await harness.commit(tx => ensureCapitalWatch(tx, connection, 'USDC'), context);
    await until(async () => {
      const saved = control((await harness.snapshot(Application, context))?.accountControls?.[owner.key]);
      return saved.refreshFailuresByBinding?.[transportBindingKey(connection)]?.code === 'capital_watch_read_timeout';
    }, 'A stalled read must finish its round with an explicit timeout instead of hanging indefinitely.');
    const afterTimeout = control((await harness.snapshot(Application, context))!.accountControls![owner.key]);
    assert.equal(afterTimeout.status!.reserved, '80'); assert.equal(afterTimeout.status!.exits[0]!.state, 'READY');
    assert.notEqual((await harness.getTask(owner.taskId, context))!.state.status, 'terminal');
    await until(async () => interrupted >= 2, 'Both unfinished-body and missing-header connections must be aborted by the bounded signal.');
    hang = false;
    await until(async () => {
      const saved = control((await harness.snapshot(Application, context))?.accountControls?.[owner.key]);
      return !!saved.watchSnapshot && !saved.refreshFailuresByBinding?.[transportBindingKey(connection)];
    }, 'The original task must retry its reads after five seconds without losing the account reservation.');
    const recovered = control((await harness.snapshot(Application, context))!.accountControls![owner.key]);
    assert.equal(recovered.monitorTaskId, owner.taskId); assert.equal(recovered.status!.reserved, '80');
    assert.ok(requestedTimeouts.filter(value => value === 10_000).length >= 2, 'Production read rounds request ten seconds; only this test shortens that timer.');
    assert.equal(posts, 0);
  } finally {
    await harness.close(context); server.closeAllConnections();
    await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});
