import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout } from 'node:timers/promises';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { createRegistry, defineExtension, Harness } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { NativeExecutionRequestV2, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { openDurableStorage } from '../../research/src/durable-storage.ts';
import { NativeSteps } from '../../research/src/native-execution.ts';
import { AdjustAccount, AdjustmentRounds } from '../src/automatic.ts';
import { unavailableCapacity } from '../src/capacity.ts';
import { Application, json, nowNs, observation } from '../src/facts.ts';
import type { ObservationFact, PortfolioFact } from '../src/facts.ts';

const context = BACKGROUND_CONTEXT;
async function until(predicate: () => Promise<boolean>) {
  for (let i = 0; i < 200; i++) { if (await predicate()) return; await setTimeout(25); }
  throw new Error('automatic_protocol_condition_timeout');
}

for (const expires of [false, true]) test(`automatic V2 round ${expires ? 'rechecks original expiry' : 'collects original sizing'} after host restart`, { timeout: 15_000 }, async () => {
  // Controlled HTTP/SQLite lifecycle evidence only, not native sizing or CRIU.
  const dataDir = await mkdtemp(join(tmpdir(), 'qz-auto-round-'));
  const originalClock = (BigInt(nowNs()) - 1_000_000n).toString();
  const deadline = (BigInt(nowNs()) + (expires ? 1_000_000_000n : 60_000_000_000n)).toString();
  const requests: NativeExecutionRequestV2[] = []; let accountReads = 0; let targetPosts = 0; let nativeReads = 0; let complete = false;
  let accepted!: () => void; const posted = new Promise<void>(resolve => { accepted = resolve; });
  const instanceId = 'controlled-native'; const profileId = 'controlled-profile'; const implementationDigest = `sha256:${'a'.repeat(64)}`;
  let baseUrl = '';
  const server = createServer(async (req, res) => {
    res.setHeader('content-type', 'application/json'); const send = (value: unknown) => res.end(JSON.stringify(value));
    if (req.url === '/account') { accountReads++; return send({ clientId: 'client', accountId: 'account', mode: 'paper', currency: 'USD', accountSourceId: 'source', observationId: 'observation',
      sequence: '9007199254740997', nativeClockNs: originalClock, controlRevision: '3', equity: '1000', cash: '1000', positions: [], execution: { currency: 'USD', validUntilNs: deadline, liquidity: [] } }); }
    if (req.url === '/capabilities') return send({ clientId: 'client', accountId: 'account', mode: 'paper', currency: 'USD', capitalExit: { version: 1, supported: true, readable: true }, automaticTargets: { supported: true, revisionBound: true, snapshotBound: true } });
    if (req.url === '/capital-exits') return send({ clientId: 'client', accountId: 'account', mode: 'paper', currency: 'USD', accountSourceId: 'source', observationId: 'observation', controlRevision: '3', reserved: '0', exits: [] });
    if (req.url === '/targets' && req.method === 'POST') { targetPosts++; return send({ error: 'test_must_not_offer_target' }); }
    if (req.url === '/v2/capabilities') return send({ schemaVersion: 2, instanceId, implementationDigest, commands: ['size-account'], recovery: { profileId, verifiedCommands: ['size-account'], checkpointIntervalMs: 60_000 } });
    if (req.url === '/v2/execute' && req.method === 'POST') { let body = ''; for await (const chunk of req) body += chunk; requests.push(JSON.parse(body)); accepted(); }
    else if (req.url?.startsWith('/v2/executions/')) nativeReads++;
    else { res.statusCode = 404; return send({ error: 'not_found' }); }
    const status = complete ? 'COMPLETED' : 'RUNNING';
    return send({ schemaVersion: 2, instanceId, requestId: requests[0]!.requestId, taskId: 12, status,
      recovery: { profileId, phase: 'execute', generation: 1, checkpointAt: null, blockedReason: null },
      ...(complete ? { result: { status: expires ? 'SIZED' : 'BLOCKED', reason: 'controlled-sizing-result', capital: '0', budget: '1000', residualCash: '1000', members: [],
        ...(expires ? { target: { schema_version: 1, asof_ns: originalClock, valid_until_ns: deadline, targets: [], cash_weight: '1' } } : {}) } } : {}) });
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string'); baseUrl = `http://127.0.0.1:${address.port}`;
  const target: RemoteNativeTarget = { id: instanceId, url: baseUrl, protocolVersion: 2, instanceId, recoveryProfileId: profileId };
  const registry = createRegistry(); registry.install(defineExtension({ name: 'test.account-round', tasks: [AdjustAccount] }));
  const open = async () => { const harness = await Harness.open(await openDurableStorage(join(dataDir, 'durable.sqlite')), { registry, models: createModels() }, context); harness.resume(); return harness; };
  let harness: Harness;
  try { harness = await open(); } catch (error) { server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); throw error; }
  try {
    const ids = await harness.commit(async tx => {
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const taskId = await tx.createTask(AdjustAccount, { id: 'binding', binary: target, dataDir }, { conversationId: conversation.id, ownership: { kind: 'conversation' }, background: true });
      const receipt = { id: 'original-prior-target', status: 'REJECTED', externalId: null, expiresAt: new Date().toISOString(), acknowledgedAt: null };
      const fact = { input: { portfolioId: 'portfolio' }, view: { id: 'binding', currency: 'USD', status: 'OBSERVING', positions: [], curve: [], equity: '1000', cash: '1000', updatedAt: null, error: null, receipt, allocation: null,
        automaticPolicy: { enabled: true, cashBuffer: '0', maximumSnapshotAgeSeconds: 60 } },
        connection: { id: 'binding', name: 'Test binding', mode: 'paper', url: baseUrl, clientId: 'client', accountId: 'account' },
        target: { schema_version: 1, valid_until_ns: deadline, targets: [] }, snapshot: null, taskId } as unknown as ObservationFact;
      const capacity = { ...unavailableCapacity('USD', 'controlled'), status: 'ESTIMATED', maximumCapital: '1000', minimumCapital: '1', validUntilNs: deadline,
        assumptions: { ...unavailableCapacity('USD', 'controlled').assumptions, participation: '0.01', maximumAgeSeconds: 60 } };
      const source = { view: { id: 'portfolio', factors: [], researchIds: [], capacity }, evidence: [], policy: {} } as unknown as PortfolioFact;
      const application = await tx.doc(Application); application.observations.binding = json(fact); application.portfolios.portfolio = json(source);
      return { conversationId: conversation.id, taskId };
    }, context);
    await posted;
    const before = await harness.snapshot(AdjustmentRounds, ids.taskId, context);
    assert.ok(before?.active); assert.equal(accountReads, 1);
    assert.equal(requests[0]!.command, 'size-account');
    const original = structuredClone(requests[0]!);
    await harness.close(context);
    if (expires) await setTimeout(1100);
    complete = true; harness = await open();
    await until(async () => {
      const fact = observation((await harness.snapshot(Application, context))!.observations.binding);
      return fact.view.allocation?.reason === (expires ? 'automatic_sizing_basis_expired' : 'controlled-sizing-result');
    });
    assert.equal(requests.length, 1, 'restart collects the original remote request instead of admitting another size calculation');
    assert.equal(accountReads, 1, 'the unfinished round cannot read a newer account snapshot before collection');
    assert.ok(nativeReads > 0);
    const steps = await harness.snapshot(NativeSteps, ids.conversationId, String(ids.taskId), context);
    assert.equal(Object.keys(steps!.steps).length, 1);
    assert.deepEqual(Object.values(steps!.steps)[0]!.request, original);
    assert.equal((await harness.snapshot(AdjustmentRounds, ids.taskId, context))!.active, null);
    assert.equal(targetPosts, 0, 'expired or blocked sizing never offers a target');
  } finally { await harness.close(context); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(dataDir, { recursive: true, force: true }); }
});
