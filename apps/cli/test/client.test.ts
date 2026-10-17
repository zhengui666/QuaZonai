import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { once } from 'node:events';
import { mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { deviceLogin } from '../src/login.ts';
import { loadProfile, saveProfile, profilePath } from '../src/profile.ts';
import type { Profile } from '../src/profile.ts';
import { run } from '../src/commands.ts';
import { schemas } from '../src/schema.ts';

async function server(handler: (request: IncomingMessage, response: ServerResponse, bytes: string) => void) {
  const server = createServer(async (request, response) => {
    let bytes = ''; for await (const chunk of request) bytes += chunk;
    response.setHeader('content-type', 'application/json'); handler(request, response, bytes);
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const address = server.address() as { port: number };
  return { origin: `http://127.0.0.1:${address.port}`, async close() { server.closeAllConnections(); await new Promise<void>((resolve) => server.close(() => resolve())); } };
}
const json = (response: ServerResponse, value: unknown, status = 200) => { response.statusCode = status; response.end(JSON.stringify(value)); };
const fakeToken = 'qzc.00000000-0000-4000-8000-000000000000.synthetic-capability';
const profile = (origin: string): Profile => ({ schema_version: 1, origin, token: fakeToken, development_http: true, ca_certificate: null });
async function configuration(origin: string) {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-cli-'));
  const previous = process.env.XDG_CONFIG_HOME; process.env.XDG_CONFIG_HOME = directory;
  await saveProfile(profile(origin));
  return { directory, async close() { if (previous === undefined) delete process.env.XDG_CONFIG_HOME; else process.env.XDG_CONFIG_HOME = previous; await rm(directory, { recursive: true, force: true }); } };
}

test('private device login retains legacy profile shape and prints only nonsecret identity; logout/forget are distinct', async () => {
  let revoked = 0; let loggedIn = 0;
  const api = await server((request, response, bytes) => {
    if (request.url === '/api/v3/devices/login') {
      assert.equal(request.method, 'POST'); assert.deepEqual(JSON.parse(bytes), { password: 'synthetic-test-password', name: 'protocol-test' }); loggedIn++;
      json(response, { token: fakeToken, id: 'nonsecret-device-id' });
    } else if (request.url === '/api/v3/session') {
      assert.equal(request.headers.authorization, `Bearer ${fakeToken}`); assert.equal(request.headers['x-quazonai-cli'], '1');
      json(response, { setupRequired: false, authenticated: true });
    } else if (request.url === '/api/v3/devices/current') { assert.equal(request.method, 'DELETE'); revoked++; response.writeHead(204).end(); }
    else json(response, { error: 'not_found' }, 404);
  });
  const config = await configuration(api.origin);
  try {
    const receipt = await deviceLogin({ ...profile(api.origin), token: '' }, 'synthetic-test-password', 'protocol-test');
    assert.deepEqual(receipt, { id: 'nonsecret-device-id', authenticated: true });
    assert.deepEqual(await loadProfile(), profile(api.origin));
    assert.deepEqual(Object.keys(JSON.parse(await readFile(profilePath(), 'utf8'))).sort(), ['ca_certificate', 'development_http', 'origin', 'schema_version', 'token']);
    assert.equal((await stat(profilePath())).mode & 0o777, 0o600);
    assert.equal((await stat(join(config.directory, 'quazonai'))).mode & 0o777, 0o700);
    const output: unknown[] = [];
    assert.equal(await run(['client', 'identity'], (value) => { output.push(value); }), 0);
    assert.equal(JSON.stringify(output).includes(fakeToken), false);
    await run(['logout'], (value) => { output.push(value); }); assert.equal(revoked, 1); assert.equal(loggedIn, 1);
    await assert.rejects(loadProfile(), /login_required/);
    await saveProfile(profile(api.origin)); await run(['forget'], (value) => { output.push(value); }); assert.equal(revoked, 1);
    const child = spawn(process.execPath, [fileURLToPath(new URL('../bin/quazonai.mjs', import.meta.url)), 'login'], { stdio: ['ignore', 'pipe', 'pipe'] });
    let error = ''; child.stderr.on('data', (part) => { error += part; }); const [code] = await once(child, 'exit');
    assert.equal(code, 1); assert.match(error, /login_requires_user_terminal/); assert.equal(error.includes(fakeToken), false);
  } finally { await config.close(); await api.close(); }
});

test('HTTP sends exact original bodies once, replays only explicit original keys, conflicts/errors stay safe; command routes match v3', async () => {
  const sent: { method: string; url: string; bytes: string }[] = []; let original: string | undefined; let drop = true;
  const api = await server((request, response, bytes) => {
    sent.push({ method: request.method!, url: request.url!, bytes });
    if (request.url === '/api/v3/research' && request.method === 'POST') {
      if (drop) { drop = false; original = bytes; request.socket.destroy(); return; }
      if (original !== bytes) { json(response, { error: 'request_id_conflict', message: fakeToken }, 409); return; }
      json(response, { conversationId: 42, submissionId: 7 }, 202);
    } else if (request.url === '/api/v3/observations' && request.method === 'GET') json(response, [{ id: 'o-1', cash: null }]);
    else if (request.url === '/api/v3/research/42/stop') json(response, { id: 42, status: 'STOPPED' }, 202);
    else json(response, { received: true });
  });
  const config = await configuration(api.origin);
  const file = join(config.directory, 'original.json');
  const bytes = '{\n "requestId":"original", "input":{"goal":"Finite original goal", "model":{"provider":"faux","modelId":"x"},"data":{"clock":"9223372036854775807"},"policy":{"max_trials":2}}\n}\n';
  await writeFile(file, bytes);
  try {
    const output: unknown[] = [];
    await assert.rejects(run(['research', 'start', '--file', file], (value) => { output.push(value); }), /write_outcome_unknown_retain_original_request/);
    assert.equal(sent.length, 1); assert.equal(sent[0]!.bytes, bytes); // No implicit retry.
    await run(['research', 'start', '--file', file, '--request-id', 'original'], (value) => { output.push(value); });
    assert.deepEqual(output[0], { conversationId: 42, submissionId: 7 }); assert.equal(sent[1]!.bytes, bytes);
    await writeFile(file, bytes.replace('Finite original goal', 'Changed goal'));
    await assert.rejects(run(['research', 'start', '--file', file]), (error: unknown) => { assert.equal((error as { code: string }).code, 'request_id_conflict'); assert.equal(String(error).includes(fakeToken), false); return true; });
    await assert.rejects(run(['research', 'start', '--file', file, '--request-id', 'new-key']), /request_id_conflict/);
    assert.equal(sent.length, 3);
    const portfolio = join(config.directory, 'portfolio.json'); await writeFile(portfolio, '{"requestId":"p-original","capital":"100.123456789123456789"}');
    const observation = join(config.directory, 'observation.json'); await writeFile(observation, '{"requestId":"o-original","portfolioId":"p-1","connectionId":"binding","mode":"paper"}');
    const settings = join(config.directory, 'settings.json'); await writeFile(settings, '{"defaultModel":{"provider":"faux","modelId":"x"},"thinkingLevel":null}');
    for (const args of [['research', 'models'], ['research', 'list', '--cursor', '{"id":4}'], ['research', 'stop', '42'], ['portfolio', 'build', '--file', portfolio], ['observation', 'start', '--file', observation], ['observation', 'get', 'o-1'], ['settings', 'set', '--file', settings], ['extensions', 'disable', 'qz.factor-wasm']]) await run(args, (value) => { output.push(value); });
    assert.deepEqual(sent.slice(3).map(({ method, url }) => [method, url]), [
      ['GET', '/api/v3/research/models'], ['GET', '/api/v3/research?cursor=%7B%22id%22%3A4%7D'], ['POST', '/api/v3/research/42/stop'], ['POST', '/api/v3/portfolios'], ['POST', '/api/v3/observations'], ['GET', '/api/v3/observations'], ['PATCH', '/api/v3/settings'], ['PATCH', '/api/v3/extensions/qz.factor-wasm'],
    ]);
    assert.equal(sent[6]!.bytes.includes('100.123456789123456789'), true);
    assert.deepEqual(output.at(-3), { id: 'o-1', cash: null });
    assert.ok((await schemas() as { names: string[] }).names.includes('ResearchStart'));
    assert.ok((await schemas('NativeFactorEvaluationRequestV1', true) as { schema: unknown }).schema);
  } finally { await config.close(); await api.close(); }
});

test('bounded wait uses native settled submissions, not final/IDLE/empty tasks; detach never requests stop', async () => {
  let status: string | null = 'placed'; const calls: string[] = [];
  const api = await server((request, response) => {
    calls.push(`${request.method} ${request.url}`);
    json(response, { id: 42, status: 'IDLE', tasks: [], final: { status: 'COMPUTED' }, ...(status ? { submission: { id: 7, status } } : {}) });
  });
  const config = await configuration(api.origin);
  try {
    let result: any;
    assert.equal(await run(['research', 'wait', '42', '--timeout', '0.04'], (value) => { result = value; }), 124);
    assert.equal(result.settled, false); assert.equal(result.cancellationRequested, false);
    status = 'done'; assert.equal(await run(['research', 'wait', '42', '--timeout', '1'], (value) => { result = value; }), 0);
    assert.equal(result.submissionStatus, 'done'); assert.equal(result.submissionId, 7);
    status = 'unanswered'; assert.equal(await run(['research', 'wait', '42', '--timeout', '1'], (value) => { result = value; }), 2);
    status = null; assert.equal(await run(['research', 'wait', '42', '--timeout', '0.04'], (value) => { result = value; }), 124); assert.equal(result.nativeSubmissionAvailable, false);
    status = 'placed'; const controller = new AbortController();
    assert.equal(await run(['research', 'watch', '42', '--timeout', '1'], (value) => { result = value; controller.abort(); }, controller.signal), 130);
    assert.equal(result.watchEnded, 'detached');
    const unlimited = new AbortController();
    assert.equal(await run(['research', 'watch', '42'], () => { unlimited.abort(); }, unlimited.signal), 130);
    assert.ok(calls.every((call) => call === 'GET /api/v3/research/42'));
  } finally { await config.close(); await api.close(); }
});

test('diagnostics and resume use the original numeric identities without implicit retries', async () => {
  const sent: { method: string; url: string; bytes: string }[] = []; let drop = true;
  const api = await server((request, response, bytes) => {
    sent.push({ method: request.method!, url: request.url!, bytes });
    if (request.method === 'POST') {
      if (drop) { drop = false; request.socket.destroy(); return; }
      json(response, { conversationId: 508, submissionId: 625, previousSubmissionId: 516 }, 202);
    } else json(response, { conversationId: 508, generation: { taskId: 620, error: 'model_error' } });
  });
  const config = await configuration(api.origin);
  try {
    const output: unknown[] = []; const emit = (value: unknown) => { output.push(value); };
    await run(['research', 'diagnostics', '508'], emit);
    await assert.rejects(run(['research', 'resume', '508', '--submission-id', '516'], emit), /write_outcome_unknown_retain_original_request/);
    assert.equal(sent.length, 2);
    await run(['research', 'resume', '508', '--submission-id', '516'], emit);
    assert.deepEqual(sent.map(({ method, url }) => [method, url]), [['GET', '/api/v3/research/508/diagnostics'], ['POST', '/api/v3/research/508/resume'], ['POST', '/api/v3/research/508/resume']]);
    assert.equal(sent[1]!.bytes, '{"submissionId":516}'); assert.equal(sent[2]!.bytes, sent[1]!.bytes);
    assert.deepEqual(output.at(-1), { conversationId: 508, submissionId: 625, previousSubmissionId: 516 });
    await assert.rejects(run(['research', 'resume', '508']), /native_numeric_research_id_required/);
    assert.equal(sent.length, 3);
  } finally { await config.close(); await api.close(); }
});
