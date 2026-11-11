import assert from 'node:assert/strict';
import { test } from 'node:test';
import { join } from 'node:path';
import Fastify from 'fastify';
import type { TaskId } from '@earendil-works/pi-durable';
import { createCapitalAccount } from '../../nautilus/src/capital-account.ts';
import { parseHyperliquidAccount } from '../../nautilus/src/hyperliquid-account.ts';
import type { AccountOptions, NativeAccountRuntime, NativeAccountSnapshot } from '../../nautilus/src/account-runtime.ts';
import { capitalDecimalUnits, validateCapitalDocument } from '../../../packages/contracts/src/capital-exit.ts';
import type { CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';
import type { ConnectionConfig } from '../src/config.ts';
import { accountKey, capitalStatus, control } from '../src/capital-exit.ts';
import { Application, json, nowNs, object, observation, nsTime } from '../src/facts.ts';
import { captureAccount } from '../src/pm-account.ts';
import { pmTargetAllocation, pmTargetPayload } from '../src/pm-target.ts';
import { digest } from '../src/pm-facts.ts';
import type { PmRecord } from '../src/pm-facts.ts';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { downstreamRequest } from '../src/downstream.ts';
import type { PortfolioFact } from '../src/facts.ts';
import { context, setup } from './support.ts';

async function until<T>(read: () => Promise<T>, ready: (value: T) => boolean) {
  const deadline = Date.now() + 12_000; let value: T;
  do {
    value = await read(); if (ready(value)) return value;
    await new Promise(resolve => setTimeout(resolve, 20));
  } while (Date.now() < deadline);
  assert.fail('The real service/receiver protocol did not settle: ' + JSON.stringify(value));
}

test('the real receiver and service share command receipts, account reservations and safe release semantics', { timeout: 30_000 }, async () => {
  // Only the external NDJSON transport is replaced. This test exercises real receiver/service business code,
  // HTTP and official Durable/SQLite; these protocol samples are not exchange or native financial evidence.
  const state = await setup(); const calls: { operation: string; payload: Record<string, unknown> }[] = [];
  let sequence = 0n; let frame: NativeAccountSnapshot | null = null; let nativeEpoch: unknown = null;
  const runtime: NativeAccountRuntime = {
    async request(operation, payload = {}) {
      calls.push({ operation, payload: structuredClone(payload) });
      if (operation === 'SNAPSHOT' || operation === 'CONTROL' && payload.epoch !== nativeEpoch) {
        if (operation === 'SNAPSHOT' && frame) return structuredClone(frame) as unknown as Record<string, unknown>;
        if (operation === 'CONTROL') nativeEpoch = payload.epoch;
        const ns = nowNs(); sequence++;
        frame = {
          ready: true, recovery_required: false, sequence: String(sequence), native_version: 'TEST_ONLY_NDJSON_PROTOCOL', observed_at_ns: ns, currency_precision: 6,
          observation_id: 'protocol-observation-' + sequence, native_session_id: 'protocol-session', native_trader_id: 'QZ-PROTOCOL', native_account_id: 'HYPERLIQUID-PAPER-PROTOCOL',
          native_snapshot: { account_id: 'HYPERLIQUID-PAPER-PROTOCOL', account_type: 'CASH', base_currency: 'USDC',
            balances: [{ currency: 'USDC', total: '1000', locked: '0', free: '1000' }], margins: [], unrealized_pnls: [], realized_pnls: [],
            total_equity: [{ amount: '1000', currency: 'USDC' }], base_currency_equity: { amount: '1000', currency: 'USDC' },
            is_stale: false, stale_instruments: [], stale_currencies: [], unpriced_instruments: [], event_id: 'protocol-event-' + sequence, ts_event: ns, ts_init: ns },
          positions: [], instruments: [], orders: [], fills: [], cash: '1000', equity: '1000', currency: 'USDC',
        };
        if (operation === 'SNAPSHOT') return structuredClone(frame) as unknown as Record<string, unknown>;
      }
      if (operation === 'TARGET') return { acknowledged: true, target_id: payload.id, epoch: payload.epoch };
      assert.ok(['CONTROL', 'CANCEL_EXIT'].includes(operation), 'Capital preparation cannot request liquidation or transfers in this cash-only protocol test.');
      return { applied: true };
    },
    async close() {},
  };
  const options: AccountOptions = { id: 'cash-protocol', clientId: 'receiver-client', accountId: 'HYPERLIQUID-PAPER-PROTOCOL', mode: 'paper',
    instruments: ['HYPE-USDC-SPOT.HYPERLIQUID'], startingBalance: '1000', paperFeeRate: '0.001', cashBuffer: '100' };
  const receiver = await createCapitalAccount(options, join(state.directory, 'receiver'), '/missing-native-worker', runtime);
  const remote = Fastify(); const responses: { method: string; path: string; body: unknown; response: unknown }[] = [];
  remote.all('/*', async (request, reply) => {
    if (request.method === 'POST' && object(request.body).requestId === 'receiver-release-race') {
      // A second authorized operator changes the native revision after the Service's read,
      // but before its financial request arrives. Do not translate or repair the original request.
      const competing = await receiver.handle(request.url, request.raw, async () => ({
        requestId: 'native-cancel-before-release', clientId: options.clientId, accountId: options.accountId,
        command: { schema_version: 2, expected_revision: object(object(request.body).command).expected_revision, action: 'CANCEL' },
      }));
      assert.equal(object(competing.value).status, 'ACCEPTED');
    }
    const result = await receiver.handle(request.url, request.raw, async () => request.body);
    responses.push({ method: request.method, path: request.url, body: request.body, response: structuredClone(result.value) });
    return reply.code(result.status).send(result.value);
  });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  const connection: ConnectionConfig = { id: 'real-receiver', name: 'Real receiver protocol account', url, clientId: options.clientId, accountId: options.accountId, mode: 'paper' };
  try {
    state.options.config.jobBinary = join(state.directory, 'missing-science-worker'); state.options.config.connections = [connection]; await state.restart();
    const binding = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers,
      payload: { requestId: 'receiver-account-only', purpose: 'ACCOUNT_ONLY', mode: 'paper', connectionId: connection.id } });
    assert.equal(binding.statusCode, 202, binding.body); assert.equal(binding.json().portfolioId, null); assert.equal(binding.json().receipt, null);
    const fact = await state.service.work.getObservation(binding.json().id); assert.equal(fact.target, null);
    // An explicit retained pre-review source tests only the ordinary delivery protocol,
    // not research qualification or a native scientific result.
    const sourceId = 'retained-protocol-source';
    await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); const owner = control(app.accountControls![accountKey(connection, app.accountControls)]).conversationId!;
      const source: PortfolioFact = { input: { requestId: sourceId, name: 'Retained protocol source', researchIds: [owner], datasetId: 'controlled', capital: '1000' },
        view: { id: sourceId, name: 'Retained protocol source', researchIds: [owner], datasetId: 'controlled', status: 'COMPUTED', capital: '1000', currency: 'USDC',
          createdAt: new Date().toISOString(), taskId: null, metrics: [], curve: [], targets: [], error: null }, evidence: [], policy: {}, native: null,
        target: { schema_version: 1, asof_ns: nowNs(), valid_until_ns: String(BigInt(nowNs()) + 60_000_000_000n), cash_weight: '0.75',
          targets: [{ instrument_id: 'HYPE-USDC-SPOT.HYPERLIQUID', currency: 'USDC', weight: '0.25' }] } };
      app.portfolios[sourceId] = json(source);
    }, context);
    const initial = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers,
      payload: { requestId: 'ordinary-initial-signal', portfolioId: sourceId, mode: 'paper', connectionId: connection.id } });
    assert.equal(initial.statusCode, 202, initial.body);
    await state.service.research.harness.waitForTask((await state.service.work.getObservation(initial.json().id)).taskId as TaskId, context);
    assert.equal((await state.service.work.getObservation(initial.json().id)).view.receipt!.status, 'ACKNOWLEDGED');
    const firstTarget = responses.find(row => row.method === 'POST' && row.path === '/targets')!;
    assert.equal(object(firstTarget.body).allocation, undefined, 'An ordinary initial signal has no allocation envelope.');
    assert.equal(calls.find(row => row.operation === 'TARGET')!.payload.capital, '900');
    const path = '/api/v3/observations/' + binding.json().id + '/capital-exits';
    const planning = await state.service.app.inject({ url: path + '/planning-context', headers: state.headers });
    assert.equal(planning.statusCode, 200, planning.body); assert.equal(planning.json().funds.free_cash.amount, '1000');
    const preview = await state.service.app.inject({ method: 'POST', url: path + '/preview', headers: state.headers,
      payload: { requestId: 'receiver-cash-preview', amount: '200', currency: 'USDC', contextId: planning.json().id, policy: { kind: 'CASH_ONLY' } } });
    assert.equal(preview.statusCode, 200, preview.body); assert.equal(preview.json().capability, 'SUPPORTED');
    const startRequest = { requestId: 'receiver-cash-start', action: 'START', previewId: preview.json().id };
    const started = await state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers, payload: startRequest });
    assert.equal(started.statusCode, 202, started.body);
    const readControl = async () => {
      const app = (await state.service.research.harness.snapshot(Application, context))!;
      return control(app.accountControls![accountKey(connection, app.accountControls)]);
    };
    const acknowledged = await until(readControl, value => value.commands[startRequest.requestId]?.state === 'ACCEPTED');
    const accepted = acknowledged.commands[startRequest.requestId]!;
    const startExchange = responses.find(row => row.method === 'POST' && row.path === '/capital-exits')!;
    const actualReceipt = object(startExchange.response); const actualRequest = object(startExchange.body);
    assert.equal(actualReceipt.requestId, startRequest.requestId); assert.equal(actualReceipt.status, 'ACCEPTED');
    assert.deepEqual(actualReceipt.command, actualRequest.command, 'The real receiver echoes the exact financial command envelope consumed by the Service.');
    assert.equal(object(accepted.result).status, 'ACCEPTED');
    validateCapitalDocument('VIEW', actualReceipt.result);

    const status = await state.service.app.inject({ url: path, headers: state.headers });
    assert.equal(status.statusCode, 200, status.body); assert.equal(capitalDecimalUnits(status.json().reserved), capitalDecimalUnits('200'));
    const reconciled = await readControl(); assert.equal(reconciled.commands[startRequest.requestId]!.reserved, '0');
    assert.equal(capitalDecimalUnits(capitalStatus(reconciled).reserved), capitalDecimalUnits('200'), 'Native aggregate and a local START reservation are counted once after native status includes the command.');
    assert.equal(reconciled.status!.exits.length, 1);

    // The PM target is an explicitly retained protocol sample, not a new scientific
    // qualification. Its NAV weight must keep denominator E despite buffer and R.
    const refreshed = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations/' + binding.json().id + '/refresh', headers: state.headers });
    assert.equal(refreshed.statusCode, 200, refreshed.body);
    const app = (await state.service.research.harness.snapshot(Application, context))!;
    const source = observation(app.observations[binding.json().id]); const basis = captureAccount(app, binding.json().id).basis;
    const expiry = String(BigInt(nowNs()) + 60_000_000_000n);
    const signal = { schema_version: 1, asof_ns: nowNs(), valid_until_ns: expiry, cash_weight: '0.5',
      targets: [{ instrument_id: 'HYPE-USDC-SPOT.HYPERLIQUID', currency: 'USDC', weight: '0.5' }] };
    const decision = { id: 'retained-pm-decision', basisRevision: 'retained-pm-basis', policyRevisionId: 'retained-pm-policy', validUntilNs: expiry,
      accountBasis: basis, explanationFacts: { nativeTargetEvidenceRef: 'retained-pm-target' },
      assetTargets: [{ instrumentId: 'HYPE-USDC-SPOT.HYPERLIQUID', targetNotional: { amount: '500', currency: 'USDC' }, navWeight: '0.5' }],
      capital: { equity: { amount: '1000', currency: 'USDC' }, deployableBudget: { amount: '900', currency: 'USDC' },
        committedCapital: { amount: '500', currency: 'USDC' }, ownerLiquidityReserve: { amount: '100', currency: 'USDC' } } } as unknown as Pm.PortfolioDecision;
    const record = { evidence: { 'retained-pm-target': { sourceKind: 'NATIVE_RESULT', contentDigest: digest(signal), sourceRevision: decision.basisRevision, objectKey: 'retained-pm-target', availableAtNs: '1' } },
      artifacts: { 'retained-pm-target': signal } } as unknown as PmRecord;
    const allocation = pmTargetAllocation(record, decision, source, app.accountControls);
    assert.equal(capitalDecimalUnits(allocation.budget), capitalDecimalUnits('700')); assert.equal(capitalDecimalUnits(allocation.capital), capitalDecimalUnits('500'));
    assert.equal(object(allocation.capitalExit).signalCapital, '1000'); assert.deepEqual(object(allocation.capitalExit).signalTarget, signal);
    const pmReceipt = await downstreamRequest(connection, 'targets', 'POST', { id: 'retained-pm-target-receipt', clientId: options.clientId, accountId: options.accountId,
      expiresAt: nsTime(expiry), target: pmTargetPayload(record, decision), allocation });
    assert.equal(pmReceipt.status, 'ACKNOWLEDGED');
    const nativeTarget = calls.filter(row => row.operation === 'TARGET').at(-1)!;
    assert.equal(nativeTarget.payload.signal_capital, '1000'); assert.equal(nativeTarget.payload.capital, '500');
    assert.equal(capitalDecimalUnits(object((nativeTarget.payload.weights as unknown[])[0]).weight) * capitalDecimalUnits(nativeTarget.payload.signal_capital) / (10n ** 18n), capitalDecimalUnits('500'));


    const ready = await until(async () => {
      const response = await state.service.app.inject({ url: path, headers: state.headers }); assert.equal(response.statusCode, 200, response.body);
      return response.json().exits[0] as CapitalExitView;
    }, exit => exit.state === 'READY');
    validateCapitalDocument('VIEW', ready); assert.equal(ready.funds.withdrawability, 'SIMULATED');
    const paused = await state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers,
      payload: { requestId: 'receiver-pause', action: 'PAUSE', exitId: ready.id, expectedRevision: ready.revision } });
    assert.equal(paused.statusCode, 202, paused.body);
    await until(readControl, value => value.commands['receiver-pause']?.state === 'ACCEPTED');
    const stopped = await until(async () => {
      const response = await state.service.app.inject({ url: path, headers: state.headers }); assert.equal(response.statusCode, 200, response.body);
      return response.json().exits[0] as CapitalExitView;
    }, exit => exit.state === 'PAUSED');
    assert.equal(stopped.funds.reserved_amount.amount, '200'); assert.equal(stopped.action_availability.RELEASE_RESERVATION!.allowed, true);
    const staleRelease = { requestId: 'receiver-release-race', action: 'RELEASE_RESERVATION', exitId: stopped.id, expectedRevision: stopped.revision };
    const rejected = await state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers, payload: staleRelease });
    assert.equal(rejected.statusCode, 202, rejected.body);
    const rejectedControl = await until(readControl, value => value.commands[staleRelease.requestId]?.state === 'REJECTED');
    assert.equal(rejectedControl.commands[staleRelease.requestId]!.error, 'capital_exit_revision_conflict');
    const retryRejected = await state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers, payload: staleRelease });
    assert.equal(retryRejected.statusCode, 202, retryRejected.body); assert.equal(retryRejected.json().state, 'REJECTED');
    assert.equal(responses.filter(row => row.method === 'POST' && object(row.body).requestId === staleRelease.requestId).length, 1);
    const cancelled = await until(async () => {
      const afterConflict = await state.service.app.inject({ url: path, headers: state.headers });
      assert.equal(afterConflict.statusCode, 200, afterConflict.body); return afterConflict.json().exits[0] as CapitalExitView;
    }, exit => exit.state === 'CANCELLED_RESERVED');
    assert.equal(cancelled.state, 'CANCELLED_RESERVED'); assert.equal(cancelled.funds.reserved_amount.amount, '200');
    const released = await state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers,
      payload: { requestId: 'receiver-release', action: 'RELEASE_RESERVATION', exitId: cancelled.id, expectedRevision: cancelled.revision } });
    assert.equal(released.statusCode, 202, released.body);
    await until(readControl, value => value.commands['receiver-release']?.state === 'ACCEPTED');
    const closed = await state.service.app.inject({ url: path + '/' + stopped.id, headers: state.headers });
    assert.equal(closed.statusCode, 200, closed.body); const closedExit = closed.json() as CapitalExitView;
    validateCapitalDocument('VIEW', closedExit); assert.equal(closedExit.state, 'CLOSED'); assert.equal(closedExit.preparation_state, 'CLOSED');
    assert.equal(closedExit.funds.reserved_amount.amount, '0'); assert.equal(closedExit.funds.waived_amount.amount, '200');
    const finalStatus = await state.service.app.inject({ url: path, headers: state.headers }); assert.equal(capitalDecimalUnits(finalStatus.json().reserved), 0n);
    const releaseTask = (await readControl()).commands['receiver-release']!.taskId;
    assert.equal((await state.service.research.harness.getTask(releaseTask as TaskId, context))!.state.status, 'terminal');
    // A historical closed exit may retain its last fence fact. It does not remain an active obligation.
    const nextSignal = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers,
      payload: { requestId: 'ordinary-signal-after-close', portfolioId: sourceId, mode: 'paper', connectionId: connection.id } });
    assert.equal(nextSignal.statusCode, 202, nextSignal.body);
    await state.service.research.harness.waitForTask((await state.service.work.getObservation(nextSignal.json().id)).taskId as TaskId, context);
    assert.equal((await state.service.work.getObservation(nextSignal.json().id)).view.receipt!.status, 'ACKNOWLEDGED');
    assert.equal(calls.filter(row => row.operation === 'TARGET').at(-1)!.payload.capital, '900');
    assert.ok(calls.some(row => row.operation === 'CONTROL')); assert.ok(calls.some(row => row.operation === 'CANCEL_EXIT'));
    assert.ok(calls.every(row => ['SNAPSHOT', 'CONTROL', 'CANCEL_EXIT', 'TARGET'].includes(row.operation)));
    assert.equal(responses.filter(row => row.method === 'POST' && row.path === '/capital-exits').length, 1);
  } finally { await remote.close(); await receiver.close(); await state.cleanup(); }
});

test('live economic proof is published with its new native observation while proof heartbeats retain their sequence', { timeout: 30_000 }, async () => {
  // The actual receiver and Service run unchanged. This NDJSON protocol peer
  // models the official owner's CONTROL -> new Portfolio observation contract;
  // it does not claim exchange connectivity, a fill or scientific qualification.
  const state = await setup(); let sequence = 0n; let economicKey = ''; let frame: NativeAccountSnapshot | null = null;
  let spotTotal = '1000'; let proofReads = 0;
  const controls: Record<string, unknown>[] = [];
  const options: AccountOptions = { id: 'live-proof-protocol', clientId: 'live-proof-client', accountId: 'HYPERLIQUID-LIVE-PROTOCOL', mode: 'live',
    accountAddress: '0x' + '1'.repeat(40), instruments: ['HYPE-USDC-SPOT.HYPERLIQUID'] };
  function observe() {
    sequence++; const ns = nowNs();
    frame = { ready: true, recovery_required: false, sequence: String(sequence), currency_precision: 6, native_version: 'TEST_ONLY_LIVE_NDJSON_PROTOCOL', observed_at_ns: ns,
      observation_id: 'live-native-observation-' + sequence, native_session_id: 'live-native-session', native_trader_id: 'QZ-LIVE-PROTOCOL', native_account_id: options.accountId,
      native_snapshot: { account_id: options.accountId, account_type: 'MARGIN', base_currency: null, balances: [], margins: [], unrealized_pnls: [], realized_pnls: [],
        total_equity: [], base_currency_equity: null, is_stale: false, stale_instruments: [], stale_currencies: [], unpriced_instruments: [],
        event_id: 'live-native-event-' + sequence, ts_event: ns, ts_init: ns },
      positions: [], instruments: [], orders: [], fills: [], cash: null, equity: null, currency: 'USDC' };
  }
  const runtime: NativeAccountRuntime = {
    async request(operation, payload = {}) {
      if (operation === 'CONTROL') {
        controls.push(structuredClone(payload));
        const key = JSON.stringify({ free: payload.spot_free_cash, total: payload.spot_total_cash, balances: payload.spot_balances });
        if (key !== economicKey) { economicKey = key; observe(); }
        return { applied: true };
      }
      assert.equal(operation, 'SNAPSHOT', 'Read-only proof monitoring cannot request any financial operation.');
      if (!frame) observe(); return structuredClone(frame) as unknown as Record<string, unknown>;
    }, async close() {},
  };
  const reader = { async account() {
    proofReads++;
    return parseHyperliquidAccount(options.accountAddress!, { balances: [{ coin: 'USDC', total: spotTotal, hold: '0' }] },
      { assetPositions: [], crossMarginSummary: { totalMarginUsed: '0' }, withdrawable: '0' }, 'default', { userSpotCrossRate: '0.001' });
  }, async ledger() { return []; } };
  const receiver = await createCapitalAccount(options, join(state.directory, 'live-receiver'), '/missing-native-worker', runtime, reader);
  const remote = Fastify();
  remote.all('/*', async (request, reply) => { const result = await receiver.handle(request.url, request.raw, async () => request.body); return reply.code(result.status).send(result.value); });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  const connection: ConnectionConfig = { id: 'live-proof', name: 'Live read protocol', mode: 'live', clientId: options.clientId, accountId: options.accountId, url };
  try {
    state.options.config.jobBinary = join(state.directory, 'missing-science'); state.options.config.connections = [connection]; await state.restart();
    const created = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers,
      payload: { requestId: 'live-read-binding', purpose: 'ACCOUNT_ONLY', mode: 'live', connectionId: connection.id } });
    assert.equal(created.statusCode, 202, created.body); const id = created.json().id;
    await state.service.research.harness.waitForTask((await state.service.work.getObservation(id)).taskId as TaskId, context);
    const initial = (await state.service.work.getObservation(id)).snapshot!; assert.equal(initial.cash, '1000');
    const initialReads = proofReads;
    await until(async () => proofReads, count => count > initialReads);
    const heartbeat = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations/' + id + '/refresh', headers: state.headers });
    assert.equal(heartbeat.statusCode, 200, heartbeat.body); assert.equal(heartbeat.json().error, null);
    assert.equal((await state.service.work.getObservation(id)).snapshot!.sequence, initial.sequence, 'Pure Info proof freshness must not invent a native account event.');
    spotTotal = '900'; const beforeEconomicRead = proofReads;
    await until(async () => proofReads, count => count > beforeEconomicRead);
    const changed = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations/' + id + '/refresh', headers: state.headers });
    assert.equal(changed.statusCode, 200, changed.body); assert.equal(changed.json().error, null); assert.equal(changed.json().cash, '900');
    const observed = (await state.service.work.getObservation(id)).snapshot!;
    assert.ok(BigInt(String(observed.sequence)) > BigInt(String(initial.sequence)), 'The new economic proof must follow an actual owner observation, never a reused sequence.');
    assert.equal(observed.controlRevision, initial.controlRevision, 'An external cash observation does not itself create a new trading control epoch.');
    const originalNative = object(object(observed.nativeObservation).snapshot);
    assert.equal(originalNative.account_type, 'MARGIN'); assert.equal(originalNative.base_currency, null, 'Read projections must not manufacture CASH scientific scope.');
    assert.equal(controls.at(-1)!.spot_free_cash, '900'); assert.equal(controls.at(-1)!.spot_total_cash, '900');
    const repeated = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations/' + id + '/refresh', headers: state.headers });
    assert.equal(repeated.statusCode, 200, repeated.body); assert.equal(repeated.json().error, null);
    assert.equal((await state.service.work.getObservation(id)).snapshot!.sequence, observed.sequence);
  } finally { await remote.close(); await receiver.close(); await state.cleanup(); }
});
