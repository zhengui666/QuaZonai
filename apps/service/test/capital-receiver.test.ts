import assert from 'node:assert/strict';
import { test } from 'node:test';
import { join } from 'node:path';
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import Fastify from 'fastify';
import type { TaskId } from '@earendil-works/pi-durable';
import { createCapitalAccount } from '../../nautilus/src/capital-account.ts';
import { parseHyperliquidAccount } from '../../nautilus/src/hyperliquid-account.ts';
import type { AccountOptions, NativeAccountRuntime, NativeAccountSummary } from '../../nautilus/src/account-runtime.ts';
import { capitalDecimalUnits, validateCapitalDocument } from '../../../packages/contracts/src/capital-exit.ts';
import type { CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';
import type { ConnectionConfig } from '../src/config.ts';
import { accountKey, capitalStatus, control } from '../src/capital-exit.ts';
import { Application, json, nowNs, object, observation } from '../src/facts.ts';
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

test('the real receiver rejects retired capital producers without sending controls or altering historical sources', { timeout: 30_000 }, async () => {
  // Only the external NDJSON transport is replaced. This test exercises real receiver/service business code,
  // HTTP and official Durable/SQLite; these protocol samples are not exchange or native financial evidence.
  const state = await setup(); const calls: { operation: string; payload: Record<string, unknown> }[] = [];
  let sequence = 0n; let frame: NativeAccountSummary | null = null; let nativeEpoch: unknown = null;
  const runtime: NativeAccountRuntime = {
    accountObservationCapability:()=>({supported:true,nativeSessionId:'protocol-session'}),
    async request(operation, payload = {}) {
      calls.push({ operation, payload: structuredClone(payload) });
      if (operation === 'ACCOUNT_SUMMARY' || operation === 'CONTROL' && payload.epoch !== nativeEpoch) {
        if (operation === 'ACCOUNT_SUMMARY' && frame) return structuredClone(frame) as unknown as Record<string, unknown>;
        if (operation === 'CONTROL') nativeEpoch = payload.epoch;
        const ns = nowNs(); sequence++;
        frame = {
          ready: true, recovery_required: false, sequence: String(sequence), native_version: 'TEST_ONLY_NDJSON_PROTOCOL', observed_at_ns: ns, currency_precision: 6,
          observation_id: 'protocol-observation-' + sequence, native_session_id: 'protocol-session', native_trader_id: 'QZ-PROTOCOL', native_account_id: 'HYPERLIQUID-PAPER-PROTOCOL',
          native_snapshot: { account_id: 'HYPERLIQUID-PAPER-PROTOCOL', account_type: 'CASH', base_currency: 'USDC',
            balances: [{ currency: 'USDC', total: '1000', locked: '0', free: '1000' }], margins: [], unrealized_pnls: [], realized_pnls: [],
            total_equity: [{ amount: '1000', currency: 'USDC' }], base_currency_equity: { amount: '1000', currency: 'USDC' },
            is_stale: false, stale_instruments: [], stale_currencies: [], unpriced_instruments: [], event_id: 'protocol-event-' + sequence, ts_event: ns, ts_init: ns },
          positions: [], instruments: [], official_node_order_activity:{open:false,inflight:false,active_local:false,emulated:false}, cash: '1000', equity: '1000', currency: 'USDC',
        };
        if (operation === 'ACCOUNT_SUMMARY') return structuredClone(frame) as unknown as Record<string, unknown>;
      }
      if (operation === 'TARGET') return { acknowledged: true, target_id: payload.id, epoch: payload.epoch };
      assert.ok(['CONTROL', 'CANCEL_EXIT'].includes(operation), 'Capital preparation cannot request liquidation or transfers in this cash-only protocol test.');
      return { applied: true };
    },
    async close() {},
  };
  const options: AccountOptions = { id: 'cash-protocol', clientId: 'receiver-client', accountId: 'HYPERLIQUID-PAPER-PROTOCOL', mode: 'paper',
    nativeStrategy: { instrument_id: 'HYPE-USDC-SPOT.HYPERLIQUID', strategy: { kind: 'EMA_CROSS', trade_size: '1', fast_period: 10, slow_period: 50 } }, instruments: ['HYPE-USDC-SPOT.HYPERLIQUID'], startingBalance: '1000', paperFeeRate: '0.001', cashBuffer: '100' };
  const originalCommand = { requestId: 'historical-unknown', clientId: options.clientId, accountId: options.accountId, command: { schema_version: 2, preview_id: 'original-plan' } };
  const receiverDir = join(state.directory, 'receiver'); await mkdir(receiverDir);
  await writeFile(join(receiverDir, 'capital-receipts.json'), JSON.stringify({ schema_version: 2, sourceId: 'retained-receiver-source', identity: {mode:'paper',network:'mainnet',accountId:options.accountId,accountAddress:null,currency:'USDC'}, revision:'3',epoch:'3',previews:{},exits:{},targets:{},ledgerClaims:{original:'original-ledger'},reverseTransferPending:false,commands:{'historical-unknown':{original:originalCommand,state:'UNCERTAIN',error:'original-response-lost',exitId:'original-exit'}} }));
  let receiver = await createCapitalAccount(options, receiverDir, '/missing-native-worker', runtime);
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
    assert.equal(initial.statusCode, 410, initial.body);
    assert.equal(initial.json().error, 'native_strategy_only');
    assert.equal(calls.filter(row => row.operation === 'TARGET').length, 0);
    const path = '/api/v3/observations/' + binding.json().id + '/capital-exits';
    const planning = await state.service.app.inject({ url: path + '/planning-context', headers: state.headers });
    assert.equal(planning.statusCode, 400); assert.equal(planning.json().error, 'downstream_control_404');
    const preview = await state.service.app.inject({ method: 'POST', url: path + '/preview', headers: state.headers, payload: {requestId:'retired-preview'} });
    assert.equal(preview.statusCode, 404);
    for (const action of ['START','RESUME','RELEASE_RESERVATION']) {
      const response = await state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers, payload:{requestId:'retired-'+action,action} });
      assert.equal(response.statusCode,410,response.body); assert.equal(response.json().error,'native_http_control_retired_migrate_to_rust_process');
    }
    const status = await state.service.app.inject({url:path,headers:state.headers});
    assert.equal(status.statusCode,200,status.body); assert.equal(status.json().reserved,null, 'The worker no longer fabricates an aggregate reservation from legacy records.'); assert.deepEqual(status.json().unsettledReservations,[]);
    const refreshed=await state.service.app.inject({method:'POST',url:'/api/v3/observations/'+binding.json().id+'/refresh',headers:state.headers});
    assert.equal(refreshed.statusCode,200,refreshed.body);
    assert.ok((await state.service.research.harness.snapshot(Application,context))!.portfolios[sourceId]);
    assert.ok(calls.length>0); assert.ok(calls.every(row=>row.operation==='ACCOUNT_SUMMARY'));
    assert.equal(responses.filter(row=>row.method==='POST').length,0,'Retired financial actions cannot reach the actual receiver.');
    const raw = async (method: string, path: string, body?: unknown, expected = 200) => {
      const response = await fetch(url + path, {method,headers:{'content-type':'application/json','x-qz-client-id':options.clientId},...(body ? {body:JSON.stringify(body)} : {})});
      assert.equal(response.status,expected); return response.json();
    };
    const originalReceipt = await raw('GET','/capital-exits/commands/historical-unknown');
    assert.equal(originalReceipt.status,'UNCERTAIN'); assert.deepEqual(originalReceipt.command,originalCommand.command);
    await raw('POST','/capital-exits',originalCommand,404);
    assert.deepEqual(await raw('GET','/capital-exits/commands/historical-unknown'),originalReceipt);
    await receiver.close(); receiver = await createCapitalAccount(options,receiverDir,'/missing-native-worker',runtime);
    assert.deepEqual(await raw('GET','/capital-exits/commands/historical-unknown'),originalReceipt);
    assert.deepEqual(JSON.parse(await readFile(join(receiverDir,'capital-receipts.json'),'utf8')).ledgerClaims,{original:'original-ledger'});
    assert.ok(calls.every(row=>row.operation==='ACCOUNT_SUMMARY'),'Receipt retry and reopen do not issue controls.');

  } finally { await remote.close(); await receiver.close(); await state.cleanup(); }
});

test('independent live proof never rewrites a native observation or automatically sends CONTROL', { timeout: 30_000 }, async () => {
  // The actual receiver and Service run unchanged. This NDJSON protocol peer
  // models the official owner's CONTROL -> new Portfolio observation contract;
  // it does not claim exchange connectivity, a fill or scientific qualification.
  const state = await setup(); let sequence = 0n; let economicKey = ''; let frame: NativeAccountSummary | null = null;
  let spotTotal = '1000'; let proofReads = 0;
  const controls: Record<string, unknown>[] = [];
  const options: AccountOptions = { id: 'live-proof-protocol', clientId: 'live-proof-client', accountId: 'HYPERLIQUID-LIVE-PROTOCOL', mode: 'live',
    accountAddress: '0x' + '1'.repeat(40), nativeStrategy: { instrument_id: 'HYPE-USDC-SPOT.HYPERLIQUID', strategy: { kind: 'EMA_CROSS', trade_size: '1', fast_period: 10, slow_period: 50 } }, instruments: ['HYPE-USDC-SPOT.HYPERLIQUID'] };
  function observe() {
    sequence++; const ns = nowNs();
    frame = { ready: true, recovery_required: false, sequence: String(sequence), currency_precision: 6, native_version: 'TEST_ONLY_LIVE_NDJSON_PROTOCOL', observed_at_ns: ns,
      observation_id: 'live-native-observation-' + sequence, native_session_id: 'live-native-session', native_trader_id: 'QZ-LIVE-PROTOCOL', native_account_id: options.accountId,
      native_snapshot: { account_id: options.accountId, account_type: 'MARGIN', base_currency: null, balances: [], margins: [], unrealized_pnls: [], realized_pnls: [],
        total_equity: [], base_currency_equity: null, is_stale: false, stale_instruments: [], stale_currencies: [], unpriced_instruments: [],
        event_id: 'live-native-event-' + sequence, ts_event: ns, ts_init: ns },
      positions: [], instruments: [], official_node_order_activity:{open:false,inflight:false,active_local:false,emulated:false}, cash: null, equity: null, currency: 'USDC' };
  }
  const runtime: NativeAccountRuntime = {
    accountObservationCapability:()=>({supported:true,nativeSessionId:'live-native-session'}),
    async request(operation, payload = {}) {
      if (operation === 'CONTROL') {
        controls.push(structuredClone(payload));
        const key = JSON.stringify({ free: payload.spot_free_cash, total: payload.spot_total_cash, balances: payload.spot_balances });
        if (key !== economicKey) { economicKey = key; observe(); }
        return { applied: true };
      }
      assert.equal(operation, 'ACCOUNT_SUMMARY', 'Read-only proof monitoring cannot request any financial operation.');
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
    const initial = (await state.service.work.getObservation(id)).snapshot!; assert.equal(initial.cash, null);
    const initialReads = proofReads;
    await until(async () => proofReads, count => count > initialReads);
    const heartbeat = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations/' + id + '/refresh', headers: state.headers });
    assert.equal(heartbeat.statusCode, 200, heartbeat.body); assert.equal(heartbeat.json().error, null);
    assert.equal((await state.service.work.getObservation(id)).snapshot!.sequence, initial.sequence, 'Pure Info proof freshness must not invent a native account event.');
    spotTotal = '900'; const beforeEconomicRead = proofReads;
    await until(async () => proofReads, count => count > beforeEconomicRead);
    const changed = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations/' + id + '/refresh', headers: state.headers });
    assert.equal(changed.statusCode, 200, changed.body); assert.equal(changed.json().error, null); assert.equal(changed.json().cash, null);
    const observed = (await state.service.work.getObservation(id)).snapshot!;
    assert.equal(observed.sequence, initial.sequence, 'Independent venue proof must not invent a native event or overwrite its values.');
    assert.equal(observed.controlRevision, initial.controlRevision, 'An external cash observation does not itself create a new trading control epoch.');
    const originalNative = object(object(observed.nativeObservation).snapshot);
    assert.equal(originalNative.account_type, 'MARGIN'); assert.equal(originalNative.base_currency, null, 'Read projections must not manufacture CASH scientific scope.');
    assert.deepEqual(controls, [], 'Refreshing external proof never sends CONTROL.');
    const repeated = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations/' + id + '/refresh', headers: state.headers });
    assert.equal(repeated.statusCode, 200, repeated.body); assert.equal(repeated.json().error, null);
    assert.equal((await state.service.work.getObservation(id)).snapshot!.sequence, observed.sequence);
  } finally { await remote.close(); await receiver.close(); await state.cleanup(); }
});
