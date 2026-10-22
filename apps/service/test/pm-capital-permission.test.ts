import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import Fastify from 'fastify';
import type { ConversationId, JsonObject, TaskId } from '@earendil-works/pi-durable';
import { Application, json, object } from '../src/facts.ts';
import { accountKey, capitalStatusForBinding, CapitalExitCommand, control, readCapitalControl, recordCapitalControl, transportCapability } from '../src/capital-exit.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { built, context, setup } from './support.ts';

async function until(read: () => Promise<boolean>, message: string) {
  const deadline = Date.now() + 10_000;
  do { if (await read()) return; await sleep(10); } while (Date.now() < deadline);
  assert.fail(message);
}

// Real owner HTTP, official Durable/SQLite tasks and actual receiver requests.
// The subprocess below is an explicitly TEST_ONLY validation-protocol barrier;
// these tests establish authorization ordering, never native financial evidence.
async function fixture() {
  const state = await setup(), remote = Fastify(), source = accountSourceFixture();
  let permission: 'SUPPORTED' | 'READABLE' | 'FAILED' = 'SUPPORTED'; let loseResponse = false; let receiptReady = false;
  const posts: JsonObject[] = [], gets: string[] = [];
  const envelope = { clientId: source.connection.clientId, accountId: source.connection.accountId, mode: 'paper', currency: 'USDC' };
  for (const prefix of ['', '/other-source']) {
    remote.get(`${prefix}/capabilities`, (_request, reply) => permission === 'FAILED' ? reply.code(503).send({ error: 'source_unavailable' }) : {
      ...envelope, capitalExit: { version: 1, supported: permission === 'SUPPORTED', readable: true } });
    remote.get(`${prefix}/capital-exits`, () => ({ ...envelope, accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: '7', reserved: '0', exits: [] }));
  }
  remote.post('/capital-exits', (request, reply) => {
    const body = json(request.body); posts.push(body);
    if (loseResponse) { reply.hijack(); reply.raw.destroy(); return; }
    return { ...body, status: 'REJECTED', result: null };
  });
  remote.get<{ Params: { id: string } }>('/capital-exits/commands/:id', (request, reply) => {
    gets.push(request.params.id); const original = posts.find(item => item.requestId === request.params.id);
    return receiptReady && original ? { ...original, status: 'REJECTED', result: null } : reply.code(503).send({ error: 'receipt_unknown' });
  });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  try {
    const portfolio = await built(state), connection = { ...source.connection, url };
    const marker = join(state.directory, 'validation-entered'), release = join(state.directory, 'validation-release');
    const validator = join(state.directory, 'controlled-capital-validator.mjs');
    await writeFile(validator, `#!${process.execPath}\n// TEST_ONLY protocol scheduling barrier, not native validation or scientific evidence.\nimport assert from 'node:assert/strict';\nimport { readFileSync, writeFileSync, existsSync } from 'node:fs';\nimport { setTimeout } from 'node:timers/promises';\nconst input=JSON.parse(readFileSync(0,'utf8'));\nassert.equal(process.argv[2],'validate-capital-exit');\nassert.ok(['START','ACTION','REQUEST','PREVIEW','VIEW'].includes(input.kind));\nif(input.kind==='START'){writeFileSync(${JSON.stringify(marker)},'entered'); const deadline=Date.now()+15000; while(!existsSync(${JSON.stringify(release)})){if(Date.now()>deadline)throw Error('test barrier timed out');await setTimeout(5);}}\nconsole.log(JSON.stringify({kind:'TEST_ONLY_VALIDATION_PROTOCOL'}));\n`, { mode: 0o700 });
    state.options.config.jobBinary = validator; state.options.config.connections = [connection]; await state.restart();
    const fresh = await readCapitalControl(connection, 'USDC');
    await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); app.accountControls ??= {};
      recordCapitalControl(app.accountControls, connection, fresh);
      app.observations[connection.id] = json({ ...source.observation, connection,
        input: { ...source.observation.input, portfolioId: portfolio.view.id },
        view: { ...source.observation.view, portfolioId: portfolio.view.id },
        capitalPreviews: { original: { preview: { id: 'original-preview', capability: 'SUPPORTED', valid_until: new Date(Date.now() + 60_000).toISOString(),
          expected_account_control_revision: '7', original_observation_id: 'native-observation', plan_artifact_id: 'original-plan', funds: { requested: { amount: '10', currency: 'USDC' } } } } } });
    }, context);
    const path = `/api/v3/observations/${encodeURIComponent(connection.id)}/capital-exits`;
    const request = { requestId: 'original-capital-command', action: 'START', previewId: 'original-preview' };
    const read = async () => (await state.service.research.harness.snapshot(Application, context))!;
    return { state, connection, path, request, posts, gets, read, portfolio,
      permission(value: typeof permission) { permission = value; },
      loseResponse() { loseResponse = true; }, allowReceipt() { receiptReady = true; },
      entered: () => until(async () => await readFile(marker, 'utf8').catch(() => '') === 'entered', 'Original command must reach the validation barrier.'),
      release: () => writeFile(release, 'released'),
      send: () => state.service.app.inject({ method: 'POST', url: `${path}/commands`, headers: state.headers, payload: request }),
      refresh: () => state.service.app.inject({ url: path, headers: state.headers }),
      async cleanup() { await writeFile(release, 'released'); await state.cleanup(); await remote.close(); } };
  } catch (error) { await state.cleanup(); await remote.close(); throw error; }
}

test('a different base URL cannot inherit a recorded capability with identical client/account names', async () => {
  const f = await fixture();
  try {
    const app = await f.read(), original = control(app.accountControls![accountKey(f.connection, app.accountControls)]);
    const alias = { ...f.connection, id: 'unobserved-alias', url: `${f.connection.url}/other-source` };
    const before = structuredClone(original);
    assert.equal(transportCapability(original, alias), null);
    assert.deepEqual(original, before, 'A miss must not persist another source envelope under the alias.');
    assert.notEqual(capitalStatusForBinding(original, alias).capability, 'SUPPORTED');
    assert.equal(capitalStatusForBinding(original, alias).reserved, null);
    assert.equal(transportCapability(original, f.connection)!.document.clientId, f.connection.clientId);
    const legacy = { commands: {}, capabilities: before.capabilities, capabilitiesReceivedAtNs: '100' };
    assert.equal(transportCapability(legacy, f.connection)!.receivedAtNs, '100');
    assert.equal(transportCapability(legacy, alias), null, 'Legacy fallback can establish one lineage, not copy it to a second URL.');
  } finally { await f.cleanup(); }
});

test('an actually read second URL retains its own first receipt even when its envelope is identical', async () => {
  const f = await fixture();
  try {
    const app = await f.read(), key = accountKey(f.connection, app.accountControls), original = control(app.accountControls![key]);
    const first = transportCapability(original, f.connection)!;
    const alias = { ...f.connection, id: 'new-source', url: `${f.connection.url}/other-source` };
    await sleep(5);
    recordCapitalControl(app.accountControls!, alias, await readCapitalControl(alias, 'USDC'));
    const second = transportCapability(control(app.accountControls![accountKey(alias, app.accountControls)]), alias)!;
    assert.deepEqual(second.document, first.document);
    assert.ok(BigInt(second.receivedAtNs) > BigInt(first.receivedAtNs), 'A fresh source read cannot inherit another URL first receipt.');
    recordCapitalControl(app.accountControls!, alias, await readCapitalControl(alias, 'USDC'));
    assert.deepEqual(transportCapability(control(app.accountControls![key]), alias), second, 'Repeated reads of this exact binding retain its own first clock.');
    assert.deepEqual(transportCapability(control(app.accountControls![key]), f.connection), first);
  } finally { await f.cleanup(); }
});

for (const changed of ['READABLE', 'FAILED'] as const) test(`capital command rechecks a concurrent ${changed} permission at actual Durable admission`, { timeout: 20_000 }, async () => {
  const f = await fixture(); let pending: PromiseLike<unknown> | undefined;
  try {
    const response = f.send(); pending = response; void response.then(() => {});
    await f.entered(); f.permission(changed);
    const refreshed = await f.refresh(); assert.equal(refreshed.statusCode, 200, refreshed.body); assert.notEqual(refreshed.json().capability, 'SUPPORTED');
    assert.equal(refreshed.json().controlRevision, '7', 'Permission changed without a physical control revision change.');
    await f.release(); const result = await response;
    assert.equal(result.statusCode, 409, result.body);
    const app = await f.read(); assert.deepEqual(control(app.accountControls![accountKey(f.connection, app.accountControls)]).commands, {});
    assert.equal(f.posts.length, 0, 'Revocation must prevent task admission, reservation and the external POST.');
  } finally { await f.release(); if (pending) await pending; await f.cleanup(); }
});

test('a persisted pending capital task rejects revoked permission before POST and releases only its unsent reservation', { timeout: 20_000 }, async () => {
  const f = await fixture();
  try {
    const app = await f.read(), initial = control(app.accountControls![accountKey(f.connection, app.accountControls)]);
    const permissionBasis = { bindingKey: JSON.stringify([`${f.connection.url}/`, f.connection.mode, f.connection.clientId, f.connection.accountId]),
      capability: transportCapability(initial, f.connection)!, status: capitalStatusForBinding(initial, f.connection) };
    f.permission('READABLE'); const revoked = await readCapitalControl(f.connection, 'USDC');
    const taskId = await f.state.service.research.harness.commit(async tx => {
      const facts = await tx.doc(Application), key = accountKey(f.connection, facts.accountControls);
      recordCapitalControl(facts.accountControls!, f.connection, revoked);
      const id = await tx.createTask(CapitalExitCommand, { key, requestId: f.request.requestId, connection: f.connection, reconcile: false,
        binary: f.state.options.config.jobBinary, dataDir: f.state.directory }, { conversationId: f.portfolio.view.researchIds[0] as ConversationId, ownership: { kind: 'conversation' }, background: true });
      object(facts.accountControls![key]!.commands)[f.request.requestId] = json({ original: f.request, body: { schema_version: 1, preview_id: 'original-preview',
        expected_account_control_revision: '7', expected_source_observation_id: 'native-observation', acknowledged_plan_artifact_id: 'original-plan' },
        path: 'capital-exits', reserved: '10', state: 'PENDING', result: null, error: null, taskId: id, permissionBasis });
      return id;
    }, context);
    await f.state.service.research.harness.waitForTask(taskId as TaskId, context);
    const after = await f.read(), command = control(after.accountControls![accountKey(f.connection, after.accountControls)]).commands[f.request.requestId]!;
    assert.equal(f.posts.length, 0); assert.equal(command.state, 'REJECTED'); assert.equal(command.reserved, '0');
    assert.equal(command.result, null, 'A local refusal is not a remote receipt.'); assert.ok(command.error); assert.deepEqual(command.original, f.request);
  } finally { await f.cleanup(); }
});

test('a response-lost capital command still reconciles its original receipt after permission is revoked', { timeout: 20_000 }, async () => {
  const f = await fixture();
  try {
    await f.release(); f.loseResponse(); const response = await f.send(); assert.equal(response.statusCode, 202, response.body);
    await until(async () => f.posts.length === 1, 'The original POST must have happened.');
    const original = structuredClone(f.posts[0]!); f.permission('READABLE'); await f.refresh(); f.allowReceipt();
    const app = await f.read(), command = control(app.accountControls![accountKey(f.connection, app.accountControls)]).commands[f.request.requestId]!;
    await f.state.service.research.harness.waitForTask(command.taskId as TaskId, context);
    const after = await f.read(), settled = control(after.accountControls![accountKey(f.connection, after.accountControls)]).commands[f.request.requestId]!;
    assert.equal(settled.state, 'REJECTED'); assert.deepEqual(settled.result, { ...original, status: 'REJECTED', result: null });
    assert.deepEqual(f.posts, [original]); assert.ok(f.gets.length > 0 && f.gets.every(id => id === f.request.requestId));
  } finally { await f.cleanup(); }
});
