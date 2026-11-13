import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import { writeFile, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { ConnectionConfig } from '../src/config.ts';
import { retainTarget } from './retained-target.ts';
import { built, setup } from './support.ts';

// These independent HTTP peers exercise routing and retained receipts only.
// Neither peer owns a broker account or submits financial orders.
async function peer(mode: 'paper' | 'live', suffix: string) {
  const app = Fastify();
  const identity = { clientId: `${mode}-client`, accountId: `${mode}-account`, mode, currency: 'USD' };
  const requests: string[] = []; let offered: Record<string, unknown> = {};
  app.addHook('onRequest', async request => { requests.push(`${request.method} ${request.url}`); });
  const receipt = () => ({ ...offered, status: 'ACKNOWLEDGED', externalId: `${mode}-original-receipt`, acknowledgedAt: '2026-10-25T00:00:00.000000001Z' });
  app.post('/targets', async request => { offered = request.body as Record<string, unknown>; return receipt(); });
  app.get('/targets/:id', receipt);
  app.get('/capabilities', () => ({ ...identity, capitalExit: { version: 2, supported: false, readable: true } }));
  app.get('/capital-exits', () => ({ ...identity,
    accountSourceId: `01900000-0000-7000-8000-${suffix}1`, observationId: `01900000-0000-7000-8000-${suffix}2`,
    controlRevision: '1', reserved: '0', exits: [] }));
  app.get('/account', () => ({ ...identity, sequence: '9007199254740995', nativeClockNs: '1800000000123456789',
    equity: mode === 'paper' ? '1000' : '2000', cash: null, positions: [] }));
  const url = await app.listen({ host: '127.0.0.1', port: 0 });
  const connection: ConnectionConfig = { id: `instance-${mode}`, name: `${mode} controlled peer`, mode, url,
    clientId: identity.clientId, accountId: identity.accountId };
  return { app, requests, connection, retain(value: Record<string, unknown>) { offered = value; } };
}

test('paper and live retain distinct remote instances across configuration changes and restart', { timeout: 30_000 }, async () => {
  const state = await setup(); const paper = await peer('paper', '00000000001'); const live = await peer('live', '00000000002');
  try {
    // Persist genuinely historical HTTP settings, never use the retired new-write API.
    const settingsPath=join(state.directory,'instances.json');
    await writeFile(settingsPath,JSON.stringify({revision:0,backtest:null,connections:[paper.connection,live.connection]}),{mode:0o600});
    await state.restart();
    const { view } = await built(state); assert.equal(view.status, 'COMPUTED', view.error ?? '');
    const wrongMode = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers,
      payload: { requestId: 'wrong-instance-purpose', portfolioId: view.id, mode: 'live', connectionId: paper.connection.id } });
    assert.equal(wrongMode.statusCode, 410); assert.equal(wrongMode.json().error, 'native_strategy_only');
    assert.deepEqual(paper.requests, []); assert.deepEqual(live.requests, []);
    const ids: string[] = [];
    for (const remote of [paper, live]) {
      const retained = await retainTarget(state, remote.connection, view.id);
      remote.retain(retained);
      const response = await state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${retained.observationId}/refresh`, headers: state.headers });
      assert.equal(response.statusCode, 200, response.body);
      const observed = response.json(); ids.push(observed.id);
      assert.equal(observed.status, 'OBSERVING', observed.error ?? '');
      assert.equal(observed.receipt?.externalId, `${remote.connection.mode}-original-receipt`);
      assert.equal(observed.equity, remote === paper ? '1000' : '2000');
    }
    const changed = await state.service.app.inject({method:'PUT',url:'/api/v3/settings/instances',headers:state.headers,
      payload:{expectedRevision:0,environment:{kind:'RUST_PROCESS',id:'new-rust',cargoBinary:process.execPath,manifestPath:join(state.directory,'Cargo.toml'),packageName:'protocol_only'}}});
    assert.equal(changed.statusCode,200,changed.body);
    const forbidden=await state.service.app.inject({method:'PUT',url:'/api/v3/settings/instances',headers:state.headers,
      payload:{expectedRevision:changed.json().revision,backtest:null,connections:[{...paper.connection,url:live.connection.url},{...live.connection,url:paper.connection.url}]}});
    assert.equal(forbidden.statusCode,400,'a new HTTP transport may not be configured');
    // Reopen an already-saved historical configuration with swapped current URLs.
    // Original observation facts must still route to their captured peers.
    const stored=JSON.parse(await readFile(settingsPath,'utf8'));
    await writeFile(settingsPath,JSON.stringify({...stored,connections:[{...paper.connection,url:live.connection.url},{...live.connection,url:paper.connection.url}]}),{mode:0o600});
    await state.restart();
    const workspace = await state.service.app.inject({ method: 'GET', url: '/api/v3/workspace', headers: state.headers });
    assert.ok(workspace.json().connections.every((item: { connected: boolean }) => !item.connected), 'an old observed binding does not prove the replacement connection works');
    assert.ok(([...paper.requests,...live.requests] as string[]).every(path=>path.startsWith('GET ')),'historical refresh, source save and restart never send a new target/control');
    paper.requests.length = 0; live.requests.length = 0;
    for (const [index, remote] of [paper, live].entries()) {
      const other = remote === paper ? live : paper; const otherCount = other.requests.length;
      const refreshed = await state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${ids[index]}/refresh`, headers: state.headers });
      assert.equal(refreshed.statusCode, 200, refreshed.body);
      assert.equal(refreshed.json().receipt.externalId, `${remote.connection.mode}-original-receipt`);
      assert.equal(other.requests.length, otherCount, 'refresh must not cross into the newly configured different instance');
      assert.ok(remote.requests.includes('GET /account'));
      assert.ok(remote.requests.some(path => path.startsWith('GET /targets/')));
      assert.ok(remote.requests.every(path => path.startsWith('GET ')), 'reconciliation must not resend a target');
      assert.deepEqual((await state.service.work.getObservation(ids[index]!)).connection, remote.connection);
    }
  } finally { await state.cleanup(); await paper.app.close(); await live.app.close(); }
});
