import assert from 'node:assert/strict';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { run } from '../src/commands.ts';
import { saveProfile } from '../src/profile.ts';
import { schemas } from '../src/schema.ts';

test('PM commands retain original revisions and bytes across uncertain writes; business identities stay opaque', async () => {
  const requests: { method: string; path: string; body: string }[] = []; let drop = true;
  const server = createServer(async (request, response) => {
    let body = ''; for await (const chunk of request) body += chunk;
    requests.push({ method: request.method!, path: request.url!, body });
    if (request.url?.endsWith('/recompute') && drop) { drop = false; request.socket.destroy(); return; }
    response.setHeader('content-type', 'application/json'); response.end(JSON.stringify({ received: true }));
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const directory = await mkdtemp(join(tmpdir(), 'qz-pm-cli-')); const previous = process.env.XDG_CONFIG_HOME;
  process.env.XDG_CONFIG_HOME = directory;
  const body = '{\n  "requestId":"original-pm-command", "expectedRevision":"9007199254740993"\n}\n';
  const file = join(directory, 'command.json'); await writeFile(file, body);
  try {
    const address = server.address() as { port: number };
    await saveProfile({ schema_version: 1, origin: `http://127.0.0.1:${address.port}`, token: 'qzc.synthetic-pm-cli', development_http: true, ca_certificate: null });
    await assert.rejects(run(['pm', 'recompute', 'mandate:one', '--file', file], () => {}), /write_outcome_unknown_retain_original_request/);
    assert.equal(requests.length, 1);
    await run(['pm', 'recompute', 'mandate:one', '--file', file], () => {});
    assert.deepEqual(requests.slice(0, 2), [
      { method: 'POST', path: '/api/v3/mandates/mandate%3Aone/recompute', body },
      { method: 'POST', path: '/api/v3/mandates/mandate%3Aone/recompute', body },
    ]);
    for (const args of [
      ['pm', 'capabilities'], ['pm', 'list'], ['pm', 'get', 'mandate:one'], ['pm', 'decision', 'mandate:one'], ['pm', 'history', 'mandate:one'],
      ['pm', 'cycle', 'cycle:one'], ['pm', 'capacity', 'capacity:one'], ['campaign', 'compare', 'campaign:one'],
      ['pm', 'targets', 'mandate:one', '--file', file], ['pm', 'refresh-data', 'mandate:one', '--file', file],
      ['pm', 'clarify-intent', 'mandate:one', '--file', file],
      ['maintenance', 'status'], ['maintenance', 'enter', '--file', file],
      ['campaign', 'close', 'campaign:one', '--file', file], ['strategy', 'assessments', 'strategy:one'], ['strategy', 'pause', 'strategy:one', '--file', file],
    ]) await run(args, () => {});
    assert.deepEqual(requests.slice(2).map(row => [row.method, row.path]), [
      ['GET', '/api/v3/capabilities'], ['GET', '/api/v3/mandates'], ['GET', '/api/v3/mandates/mandate%3Aone'], ['GET', '/api/v3/mandates/mandate%3Aone/decision'], ['GET', '/api/v3/mandates/mandate%3Aone/decisions'],
      ['GET', '/api/v3/cycles/cycle%3Aone'], ['GET', '/api/v3/capacity-estimates/capacity%3Aone'], ['GET', '/api/v3/campaigns/campaign%3Aone/decision'],
      ['POST', '/api/v3/mandates/mandate%3Aone/targets'], ['POST', '/api/v3/mandates/mandate%3Aone/refresh-data'],
      ['POST', '/api/v3/mandates/mandate%3Aone/intent-clarifications'],
      ['GET', '/api/v3/maintenance'], ['POST', '/api/v3/maintenance/enter'],
      ['POST', '/api/v3/campaigns/campaign%3Aone/close'], ['GET', '/api/v3/strategies/strategy%3Aone/assessments'], ['POST', '/api/v3/strategies/strategy%3Aone/pause'],
    ]);
    const admitted = requests.length;
    await writeFile(file, '{"requestId":"same-request"}');
    await assert.rejects(run(['campaign', 'resume', 'campaign:one', '--file', file]), /expected_revision_required/);
    assert.equal(requests.length, admitted);
    const schema = await schemas('MandateRevision'); assert.equal(schema.entry, 'MandateRevision'); assert.match(schema.source!, /interface OperatingComparison/);
  } finally {
    if (previous === undefined) delete process.env.XDG_CONFIG_HOME; else process.env.XDG_CONFIG_HOME = previous;
    await rm(directory, { recursive: true, force: true }); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
});
