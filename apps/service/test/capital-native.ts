import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setTimeout as sleep } from 'node:timers/promises';
import Fastify from 'fastify';
import { Application, json, nowNs, observation } from '../src/facts.ts';
import { built, context, setup } from './support.ts';
import { AdjustAccount, unsettledAdjustment } from '../src/automatic.ts';
import type { TaskId } from '@earendil-works/pi-durable';

const id = (value: string) => `01900000-0000-7000-8000-${value.padStart(12, '0')}`;
async function until<T>(read: () => Promise<T>, ready: (value: T) => boolean): Promise<T> {
  const end = Date.now() + 14_000; let value: T;
  do { value = await read(); if (ready(value)) return value; await sleep(40); } while (Date.now() < end);
  assert.fail(`condition not reached: ${JSON.stringify(value)}`);
}

// Planning/START was retired. Current retained-exit PAUSE/CANCEL, lost-response
// recovery and native preview contracts are exercised by capital-workbench.test.ts
// and capital-retirement.test.ts in the native suite, not by a synthetic new plan.
const capitalCapability = (supported = true, readable = true) => ({ version: 2, supported, readable,
  policies: ['CASH_ONLY', 'NATURAL_RELEASE', 'BOUNDED_LIMIT'],
  actions: ['START', 'PAUSE', 'CANCEL', 'RESUME', 'RELEASE_RESERVATION', 'RECONCILE_EXTERNAL_TRANSFER', 'RECONCILE_WITHDRAWAL'] });

// Original target authority has no equivalent in native START/STOP. These
// retained facts exercise only recovery/retirement, never new signal generation.
async function retainedProtocol(mode: 'paper' | 'live', allocated: boolean) {
  const state = await setup(), remote = Fastify(); const queries: string[] = []; let posts = 0, available = false, mismatch = false, reserved = '100.000000000000000001', revision = '7', sequence = 9007199254740993n;
  const identity = { clientId: 'retained-client', accountId: 'retained-account', mode, currency: 'USD' };
  const target = { schema_version: 1, asof_ns: nowNs(), valid_until_ns: String(BigInt(nowNs()) + 300_000_000_000n), cash_weight: '1', targets: [] };
  const prepared = { schemaVersion: 1, controlRevision: '7', snapshotSequence: '9007199254740993', reserved,
    equity: '600', budget: '449.999999999999999999', capital: '449.999999999999999999', cashBuffer: '50', factors: [] };
  const receipt = { ...identity, id: 'original-before-upgrade', expiresAt: new Date(Date.now() + 300_000).toISOString(), externalId: 'original-remote-id', acknowledgedAt: null, status: 'ACKNOWLEDGED', allocation: prepared };
  remote.get('/capabilities', () => ({ ...identity, capitalExit: capitalCapability() }));
  remote.get('/capital-exits', () => ({ ...identity, accountSourceId: id('1'), observationId: id('2'), controlRevision: revision, reserved, exits: [] }));
  remote.get('/account', () => ({ ...identity, sequence: String(sequence), nativeClockNs: String(sequence), accountSourceId: id('1'), observationId: id('2'), controlRevision: revision, equity: '600', cash: '600', positions: [] }));
  remote.post('/targets', (_req, reply) => { posts++; return reply.code(410).send({ error: 'retired_target_must_not_be_called' }); });
  remote.get<{ Params: { id: string } }>('/targets/:id', (request, reply) => {
    queries.push(request.params.id);
    return available ? { ...receipt, ...(mismatch ? { allocation: { ...prepared, reserved: '0' }, accountId: allocated ? identity.accountId : 'other-account' } : {}) }
      : reply.code(404).send({ error: 'original_receipt_unknown' });
  });
  const connection = { ...identity, id: 'retained', name: 'Retained transport protocol, not a fill claim', url: await remote.listen({ host: '127.0.0.1', port: 0 }) };
  const source = await built(state); state.options.config.connections = [connection]; await state.restart();
  const original = await state.service.work.startObservation({ requestId: 'account-only', purpose: 'ACCOUNT_ONLY', mode, connectionId: connection.id });
  await state.service.research.harness.waitForTask((await state.service.work.getObservation(original.id)).taskId as TaskId, context);
  await state.service.research.harness.commit(async tx => {
    const app = await tx.doc(Application), fact = observation(app.observations[original.id]);
    fact.input.portfolioId = source.view.id; fact.target = target;
    fact.view.receipt = { id: allocated ? 'never-offered-base' : receipt.id, status: allocated ? 'NOT_OFFERED' : 'UNCERTAIN', expiresAt: receipt.expiresAt, externalId: null, acknowledgedAt: null };
    if (allocated) {
      fact.view.automaticPolicy = { enabled: false, cashBuffer: '50', maximumSnapshotAgeSeconds: 60 };
      fact.allocationContext = prepared; fact.allocationTarget = target;
      fact.view.allocation = { status: 'UNCERTAIN', reason: 'original_response_lost', budget: prepared.budget, equity: '600', capital: prepared.capital, reserved,
        receipt: { id: receipt.id, status: 'UNCERTAIN', expiresAt: receipt.expiresAt, externalId: null, acknowledgedAt: null }, members: [] } as any;
    }
    app.observations[original.id] = json(fact);
  }, context);
  return { state, connection, original, source, prepared, target, receipt, queries, get posts() { return posts; },
    allow(value = true) { available = value; }, mismatch(value: boolean) { mismatch = value; }, reserve(value: string) { reserved = value; revision = String(BigInt(revision) + 1n); sequence++; },
    async read() { return state.service.work.getObservation(original.id); }, async cleanup() { await state.cleanup(); await remote.close(); } };
}

for (const mode of ['paper', 'live'] as const) for (const allocated of [false, true]) test(`retained ${allocated ? 'allocation' : 'manual'} receipt in ${mode} survives uncertainty, identity mismatch and restart without another POST`, { timeout: 30_000 }, async () => {
  const p = await retainedProtocol(mode, allocated);
  try {
    const readReceipt = async () => { const fact = await p.read(); return allocated ? fact.view.allocation!.receipt! : fact.view.receipt!; };
    await p.state.service.work.refreshObservation(p.original.id);
    assert.equal((await readReceipt()).status, 'UNCERTAIN'); assert.equal(p.posts, 0); assert.ok(p.queries.every(id => id === p.receipt.id));
    await p.state.restart(); p.reserve('200.000000000000000001'); p.allow(); p.mismatch(true);
    await p.state.service.work.refreshObservation(p.original.id);
    assert.equal((await readReceipt()).status, 'UNCERTAIN'); assert.equal((await readReceipt()).id, p.receipt.id);
    if (allocated) assert.deepEqual((await p.read()).allocationContext, p.prepared, 'New reservation facts cannot rewrite the original sent context.');
    p.mismatch(false); await p.state.service.work.refreshObservation(p.original.id);
    assert.equal((await readReceipt()).status, 'ACKNOWLEDGED'); assert.equal((await readReceipt()).externalId, p.receipt.externalId);
    assert.equal(p.posts, 0); assert.ok(p.queries.every(id => id === p.receipt.id));
    if (allocated) { assert.equal((await p.read()).view.allocation!.budget, '449.999999999999999999'); assert.deepEqual((await p.read()).allocationContext, p.prepared); }
    const disabled = await p.state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${p.original.id}/automatic`, headers: p.state.headers, payload: { enabled: true, cashBuffer: '0', maximumSnapshotAgeSeconds: 60 } });
    assert.equal(disabled.statusCode, 410); assert.equal(p.posts, 0);
    await p.state.restart(); assert.equal((await readReceipt()).id, p.receipt.id); assert.equal((await readReceipt()).status, 'ACKNOWLEDGED');
  } finally { await p.cleanup(); }
});

for (const phase of ['snapshot', 'deliver', 'abort'] as const) test(`retained unsent allocation ${phase} never sends or re-sizes and preserves its exact money/context`, async () => {
  const p = await retainedProtocol('paper', true);
  try {
    await p.state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application), fact = observation(app.observations[p.original.id]);
      fact.view.allocation!.status = 'SIZED'; fact.view.allocation!.receipt!.status = 'PENDING'; fact.view.automaticPolicy!.enabled = true;
      app.observations[p.original.id] = json(fact);
    }, context);
    await p.state.restart();
    const task: any = { id: 0, conversationId: p.source.view.researchIds[0], input: { id: p.original.id, binary: '/must-not-execute', dataDir: p.state.directory } };
    const runtime: any = { signal: new AbortController().signal, commit: (operation: any) => p.state.service.research.harness.commit(operation, context) };
    if (phase === 'abort') await AdjustAccount.definition.abort!(task, runtime, context);
    else await AdjustAccount.definition.phases[phase](task, runtime, context);
    const fact = await p.read(); assert.equal(fact.view.allocation!.receipt!.status, 'NOT_OFFERED'); assert.equal(fact.view.allocation!.status, 'STOPPED');
    assert.equal(unsettledAdjustment(fact), false); assert.deepEqual(fact.allocationContext, p.prepared); assert.deepEqual(fact.allocationTarget, p.target);
    assert.equal(fact.view.allocation!.budget, '449.999999999999999999'); assert.equal(p.posts, 0); assert.deepEqual(p.queries, []);
    await p.state.restart(); assert.equal((await p.read()).view.allocation!.receipt!.status, 'NOT_OFFERED'); assert.equal(p.posts, 0);
  } finally { await p.cleanup(); }
});

test('aborting an uncertain retained allocation schedules original receipt recovery, not remote cancellation or a new target', { timeout: 30_000 }, async () => {
  const p = await retainedProtocol('paper', true);
  try {
    const task: any = { id: 0, conversationId: p.source.view.researchIds[0], input: { id: p.original.id, binary: '/must-not-execute', dataDir: p.state.directory } };
    const runtime: any = { signal: new AbortController().signal, commit: (operation: any) => p.state.service.research.harness.commit(operation, context) };
    await AdjustAccount.definition.abort!(task, runtime, context);
    const recovering = await until(() => p.read(), fact => fact.view.allocation?.reason === 'automatic_receipt_requires_reconciliation' || p.queries.length > 0);
    assert.equal(recovering.view.allocation!.receipt!.status, 'UNCERTAIN'); assert.equal(unsettledAdjustment(recovering), true);
    assert.equal(recovering.view.allocation!.receipt!.id, p.receipt.id); assert.equal(p.posts, 0);
    await p.state.restart(); p.allow();
    const settled = await until(() => p.read(), fact => fact.view.allocation?.receipt?.status === 'ACKNOWLEDGED');
    assert.equal(settled.view.allocation!.receipt!.id, p.receipt.id); assert.deepEqual(settled.allocationContext, p.prepared);
    assert.equal(p.posts, 0); assert.ok(p.queries.every(id => id === p.receipt.id));
  } finally { await p.cleanup(); }
});
