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
  const bytes = '{\n "requestId":"original", "input":{"goal":"Finite original goal", "model":{"provider":"openai","modelId":"gpt-6.1-sol"},"thinkingLevel":"max","serviceTier":"priority","data":{"clock":"9223372036854775807"},"policy":{"max_trials":2}}\n}\n';
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

test('portfolio capacity posts exact assumptions to the original ID without a new request key', async () => {
  const sent: { method: string; url: string; bytes: string }[] = [];
  const api = await server((request, response, bytes) => { sent.push({ method: request.method!, url: request.url!, bytes }); json(response, { status: 'UNAVAILABLE', reason: 'TEST_ONLY_PROTOCOL' }); });
  const config = await configuration(api.origin); const file = join(config.directory, 'portfolio-capital.json');
  try {
    await assert.rejects(run(['portfolio', 'capacity', 'p-original']), /file_required/);
    await writeFile(file, JSON.stringify({ scenarios: ['0.0000000000000000001'], participation: '0.01', maximumAgeSeconds: 60 }));
    await assert.rejects(run(['portfolio', 'capacity', 'p-original', '--file', file]), /research_capital_scenarios_invalid/);
    assert.equal(sent.length, 0);
    const bytes = '{\n "scenarios":["1000.000000000000000001"], "participation":"0.01", "maximumAgeSeconds":60\n}\n';
    await writeFile(file, bytes); const output: unknown[] = [];
    await run(['portfolio', 'capacity', 'p/original', '--file', file], value => { output.push(value); });
    assert.deepEqual(sent, [{ method: 'POST', url: '/api/v3/portfolios/p%2Foriginal/capacity', bytes }]);
    assert.deepEqual(output, [{ status: 'UNAVAILABLE', reason: 'TEST_ONLY_PROTOCOL' }]);
    assert.equal((await schemas('CapitalAssumptions') as { entry: string }).entry, 'CapitalAssumptions');
    const child = spawn(process.execPath, [fileURLToPath(new URL('../bin/quazonai.mjs', import.meta.url)), '--help'], { stdio: ['ignore', 'pipe', 'pipe'] });
    let help = ''; child.stdout.on('data', part => { help += part; }); const [code] = await once(child, 'exit');
    assert.equal(code, 0); assert.match(help, /portfolio capacity ID --file ASSUMPTIONS.json/);
  } finally { await config.close(); await api.close(); }
});

test('factor capacity validates native decimal limits locally and preserves accepted exact file bytes', async () => {
  const sent: string[] = [];
  const api = await server((_request, response, bytes) => { sent.push(bytes); json(response, { received: true }); });
  const config = await configuration(api.origin); const file = join(config.directory, 'capital.json');
  try {
    for (const scenario of ['100000000000000000000', '0'.repeat(64) + '1', '0.0000000000000000001', '1\n']) {
      await writeFile(file, JSON.stringify({ scenarios: [scenario], participation: '0.01', maximumAgeSeconds: 60 }));
      await assert.rejects(run(['factor', 'capacity', 'r2.t3.m0', '--file', file]), /research_capital_scenarios_invalid/); assert.equal(sent.length, 0);
    }
    for (const scenario of ['99999999999999999999.999999999999999999', '0.000000000000000001', '0'.repeat(63) + '1', '1.' + '0'.repeat(62)]) {
      const bytes = JSON.stringify({ scenarios: [scenario], participation: '0.01', maximumAgeSeconds: 60 }, null, 2) + '\n'; await writeFile(file, bytes);
      await run(['factor', 'capacity', 'r2.t3.m0', '--file', file], () => {}); assert.equal(sent.at(-1), bytes);
    }
  } finally { await config.close(); await api.close(); }
});

test('structured validation and factor/account-control commands preserve original file bytes and concrete routes', async () => {
  const sent: { method: string; url: string; bytes: string }[] = [];
  const api = await server((request, response, bytes) => { sent.push({ method: request.method!, url: request.url!, bytes }); json(response, { received: true }); });
  const config = await configuration(api.origin); const file = join(config.directory, 'original.json');
  const specification = { version: 1, name: 'Original structured study', question: 'Original economic question', rationale: 'Explicit falsifiable mechanism', evaluation: { metric: 'pearson_ic', direction: 'maximize', scheme: 'registered-partitions' }, search: { scope: 'single-factor', maxMembers: 1, stop: 'budget-or-selection' } };
  try {
    await writeFile(file, JSON.stringify({ requestId: 'unsupported', specification: { ...specification, horizon: 9 } }));
    await assert.rejects(run(['research', 'start', '--file', file]), /research_override_unsupported/); assert.equal(sent.length, 0);
    const bytes = JSON.stringify({ requestId: 'original-structured', goal: specification.question, specification, model: { provider: 'faux', modelId: 'original' }, datasetId: 'registered', maxTrials: 2 }, null, 2) + '\n'; await writeFile(file, bytes);
    await run(['research', 'start', '--file', file], () => {}); assert.equal(sent[0]!.bytes, bytes);
    await run(['factor', 'list'], () => {}); await run(['factor', 'get', 'r2.t3.m0'], () => {}); await run(['factor', 'capacity', 'r2.t3.m0'], () => {});
    const assumptions = '{ "scenarios":["1000.000000000000000001"], "participation":"0.01", "maximumAgeSeconds":60 }\n'; await writeFile(file, assumptions); await run(['factor', 'capacity', 'r2.t3.m0', '--file', file], () => {}); assert.equal(sent.at(-1)!.bytes, assumptions);
    await writeFile(file, '{"scenarios":["1000"],"participation":"2","maximumAgeSeconds":60}'); await assert.rejects(run(['factor', 'capacity', 'r2.t3.m0', '--file', file]), /research_participation_invalid/);
    const preview = '{ "requestId":"preview-original", "amount":"100.000000000000000001", "currency":"USD" }\n'; await writeFile(file, preview);
    await run(['capital', 'status', 'o-original'], () => {}); await run(['capital', 'preview', 'o-original', '--file', file], () => {}); assert.equal(sent.at(-1)!.bytes, preview);
    const command = '{"requestId":"command-original","action":"START","previewId":"native-original"}\n'; await writeFile(file, command); await run(['capital', 'command', 'o-original', '--file', file], () => {}); assert.equal(sent.at(-1)!.bytes, command);
    const policy = '{"enabled":true,"cashBuffer":"0.000000000000000001","maximumSnapshotAgeSeconds":60}\n'; await writeFile(file, policy); await run(['observation', 'automatic', 'o-original', '--file', file], () => {}); assert.equal(sent.at(-1)!.bytes, policy);
    assert.deepEqual(sent.slice(1).map(row => [row.method, row.url]), [
      ['GET', '/api/v3/factors'], ['GET', '/api/v3/factors/r2.t3.m0'], ['GET', '/api/v3/factors/r2.t3.m0/capacity'], ['POST', '/api/v3/factors/r2.t3.m0/capacity'], ['GET', '/api/v3/observations/o-original/capital-exits'],
      ['POST', '/api/v3/observations/o-original/capital-exits/preview'], ['POST', '/api/v3/observations/o-original/capital-exits/commands'], ['POST', '/api/v3/observations/o-original/automatic'],
    ]);
  } finally { await config.close(); await api.close(); }
});

test('delete commands retain exact original identities, send once without bodies and expose receipts or blockers', async () => {
  const sent: { method: string; url: string; bytes: string }[] = []; let drop = true;
  const receipt = { kind: 'research', id: '508', deletedAt: '2026-10-25T12:00:00.000Z' };
  const api = await server((request, response, bytes) => {
    sent.push({ method: request.method!, url: request.url!, bytes });
    if (request.url === '/api/v3/research/508' && drop) { drop = false; request.socket.destroy(); return; }
    if (request.url === '/api/v3/portfolios/blocked') { json(response, { error: 'portfolio_in_use' }, 409); return; }
    json(response, request.url === '/api/v3/research/508' ? receipt : { kind: 'factor', id: 'r2.t3.m0', deletedAt: receipt.deletedAt });
  });
  const config = await configuration(api.origin); const output: unknown[] = [];
  try {
    for (const command of [['pm', 'delete'], ['research', 'delete'], ['factor', 'delete'], ['portfolio', 'delete']]) await assert.rejects(run(command), /resource_id_required|native_numeric_research_id_required/);
    await assert.rejects(run(['research', 'delete', '9007199254740993']), /native_numeric_research_id_required/);
    assert.equal(sent.length, 0);
    await assert.rejects(run(['research', 'delete', '508'], value => { output.push(value); }), /write_outcome_unknown_retain_original_request/);
    assert.equal(sent.length, 1); assert.equal(output.length, 0);
    await run(['research', 'delete', '508'], value => { output.push(value); });
    assert.deepEqual(output, [receipt]);
    for (const args of [['pm', 'delete', 'mandate:one/two'], ['factor', 'delete', 'r2.t3.m0'], ['portfolio', 'delete', 'p/original'], ['backtest', 'delete', 'p:alias']]) await run(args, () => {});
    await assert.rejects(run(['portfolio', 'delete', 'blocked']), error => {
      assert.equal((error as { code: string }).code, 'portfolio_in_use'); assert.equal((error as { status: number }).status, 409); return true;
    });
    assert.deepEqual(sent, [
      { method: 'DELETE', url: '/api/v3/research/508', bytes: '' },
      { method: 'DELETE', url: '/api/v3/research/508', bytes: '' },
      { method: 'DELETE', url: '/api/v3/mandates/mandate%3Aone%2Ftwo', bytes: '' },
      { method: 'DELETE', url: '/api/v3/factors/r2.t3.m0', bytes: '' },
      { method: 'DELETE', url: '/api/v3/portfolios/p%2Foriginal', bytes: '' },
      { method: 'DELETE', url: '/api/v3/portfolios/p%3Aalias', bytes: '' },
      { method: 'DELETE', url: '/api/v3/portfolios/blocked', bytes: '' },
    ]);
    const child = spawn(process.execPath, [fileURLToPath(new URL('../bin/quazonai.mjs', import.meta.url)), '--help'], { stdio: ['ignore', 'pipe', 'pipe'] });
    let help = ''; child.stdout.on('data', part => { help += part; }); const [code] = await once(child, 'exit');
    assert.equal(code, 0);
    for (const line of [/pm list \| get ID \| delete ID/, /research models.*delete ID/, /factor list \| get ID \| delete ID/, /portfolio build.*delete ID/]) assert.match(help, line);
  } finally { await config.close(); await api.close(); }
});


test('economic tick CLI submits original bytes and reads the opaque replay through existing device auth', async () => {
  const requests: { method: string; path: string; body: string }[] = [];
  const api = await server((request, response, body) => {
    assert.equal(request.headers.authorization, `Bearer ${fakeToken}`);
    assert.equal(request.headers['x-quazonai-cli'], '1');
    requests.push({ method: request.method!, path: request.url!, body });
    json(response, { id: 'replay:original', status: 'QUEUED' });
  });
  const config = await configuration(api.origin);
  try {
    const body = '{\n "requestId":"original-economic", "datasetId":"ticks", "factorId":"r1.t2.m0", "request":{"scenario":{"initial_usdt":"3000.000000000000000001"}}\n}\n';
    const file = join(config.directory, 'economic.json'); await writeFile(file, body);
    await run(['portfolio', 'replay-ticks', '--file', file], () => {});
    await run(['portfolio', 'tick-replay', 'replay:original'], () => {});
    assert.deepEqual(requests, [
      { method: 'POST', path: '/api/v3/tick-economic-replays', body },
      { method: 'GET', path: '/api/v3/tick-economic-replays/replay%3Aoriginal', body: '' },
    ]);
  } finally { await config.close(); await api.close(); }
});
