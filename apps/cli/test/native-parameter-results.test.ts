import assert from 'node:assert/strict';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { run } from '../src/commands.ts';
import { saveProfile } from '../src/profile.ts';

test('CLI research grid intent and original task readers use existing authenticated API transport', async () => {
  const calls: { method: string; url: string; body: string }[] = [];
  const server = createServer(async (request, response) => {
    let body = ''; for await (const chunk of request) body += chunk;
    calls.push({ method: request.method!, url: request.url!, body });
    response.writeHead(200, { 'content-type': 'application/json' }); response.end(JSON.stringify({ original: '0.000000000000000019' }));
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const directory = await mkdtemp(join(tmpdir(), 'qz-grid-cli-'));
  const previous = process.env.XDG_CONFIG_HOME; process.env.XDG_CONFIG_HOME = directory;
  try {
    await saveProfile({ schema_version: 1, origin: `http://127.0.0.1:${(server.address() as { port: number }).port}`, token: 'qzc.test-grid', development_http: true, ca_certificate: null });
    const goal = 'QuaZonai generate fast periods [8,16] and slow periods [32,64]; Nautilus backtests, no automatic winner';
    await run(['research', 'start', '--goal', goal, '--dataset', 'training', '--model', 'provider:model', '--request-id', 'original-grid'], () => {});
    const outputs: unknown[] = [];
    await run(['research', 'native-backtests', '11', '--cursor', 'opaque/a+b='], value => outputs.push(value));
    await run(['research', 'evidence', '11', '24'], value => outputs.push(value));
    assert.equal(JSON.parse(calls[0]!.body).goal, goal);
    assert.deepEqual(calls.slice(1).map(({method,url}) => ({method,url})), [
      { method: 'GET', url: '/api/v3/research/11/native-backtests?cursor=opaque%2Fa%2Bb%3D' },
      { method: 'GET', url: '/api/v3/research/11/evidence/24' },
    ]);
    assert.deepEqual(outputs, [{ original: '0.000000000000000019' }, { original: '0.000000000000000019' }]);
  } finally {
    if (previous === undefined) delete process.env.XDG_CONFIG_HOME; else process.env.XDG_CONFIG_HOME = previous;
    await rm(directory, { recursive: true, force: true }); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
});


test('research process, run pages and source registration use the existing service contracts', async () => {
  const calls: { method: string; url: string; body: string }[] = [];
  const responses = [
    { messages: [], nextCursor: 'next/a+b=' },
    { messages: [{ entryId: 8, role: 'assistant', text: 'Original conclusion' }], nextCursor: null },
    { items: [{ iterations: '250000', pnl: null }], total: 21, nextOffset: null, resultTaskId: 24 },
    { requestId: 'source-1', files: [{ id: 'strategy', length: 12 }] },
  ];
  const server = createServer(async (request, response) => {
    let body = ''; for await (const chunk of request) body += chunk;
    calls.push({ method: request.method!, url: request.url!, body });
    response.writeHead(200, { 'content-type': 'application/json' }); response.end(JSON.stringify(responses[calls.length - 1]));
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const directory = await mkdtemp(join(tmpdir(), 'qz-visibility-cli-'));
  const previous = process.env.XDG_CONFIG_HOME; process.env.XDG_CONFIG_HOME = directory;
  try {
    await saveProfile({ schema_version: 1, origin: `http://127.0.0.1:${(server.address() as { port: number }).port}`, token: 'qzc.test-read', development_http: true, ca_certificate: null });
    const outputs: unknown[] = [];
    await run(['research', 'messages', '11'], value => outputs.push(value));
    await run(['research', 'messages', '11', '--cursor', 'next/a+b='], value => outputs.push(value));
    await run(['research', 'native-runs', '11', '--result-task-id', '24', '--offset', '20'], value => outputs.push(value));
    const source = { requestId: 'source-1', shareWithResearchModel: true, model: { provider: 'provider', modelId: 'model' }, files: [{ id: 'strategy', name: 'strategy.rs', revision: 'v1', content: 'fn run() {}' }] };
    const path = join(directory, 'source.json'); await writeFile(path, JSON.stringify(source));
    await run(['research', 'implementation-sources', '11', '--file', path, '--request-id', 'source-1'], value => outputs.push(value));
    assert.deepEqual(calls.map(({method,url}) => ({method,url})), [
      { method: 'GET', url: '/api/v3/research/11/messages' },
      { method: 'GET', url: '/api/v3/research/11/messages?cursor=next%2Fa%2Bb%3D' },
      { method: 'GET', url: '/api/v3/research/11/native-runs?resultTaskId=24&offset=20' },
      { method: 'POST', url: '/api/v3/research/11/implementation-sources' },
    ]);
    assert.deepEqual(JSON.parse(calls[3]!.body), source);
    assert.deepEqual(outputs, responses);
    for (const offset of ['-1', '1.5', 'NaN', '9007199254740992']) {
      await assert.rejects(run(['research', 'native-runs', '11', '--result-task-id', '24', `--offset=${offset}`]), /native_result_page_invalid/);
    }
    await assert.rejects(run(['research', 'native-runs', '11']), /native_numeric_research_id_required/);
    await writeFile(path, JSON.stringify({ ...source, shareWithResearchModel: false }));
    await assert.rejects(run(['research', 'implementation-sources', '11', '--file', path]), /implementation_source_explicit_sharing_required/);
    assert.equal(calls.length, 4);
  } finally {
    if (previous === undefined) delete process.env.XDG_CONFIG_HOME; else process.env.XDG_CONFIG_HOME = previous;
    await rm(directory, { recursive: true, force: true }); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
});
