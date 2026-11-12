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
    const observation = join(config.directory, 'observation.json'); await writeFile(observation, '{"requestId":"o-original","purpose":"ACCOUNT_ONLY","connectionId":"binding","mode":"paper"}');
    const settings = join(config.directory, 'settings.json'); await writeFile(settings, '{"defaultModel":{"provider":"faux","modelId":"x"},"thinkingLevel":null}');
    for (const args of [['research', 'models'], ['research', 'list', '--cursor', '{"id":4}'], ['research', 'stop', '42'], ['observation', 'start', '--file', observation], ['observation', 'get', 'o-1'], ['settings', 'set', '--file', settings], ['extensions', 'disable', 'qz.research-native']]) await run(args, (value) => { output.push(value); });
    assert.deepEqual(sent.slice(3).map(({ method, url }) => [method, url]), [
      ['GET', '/api/v3/research/models'], ['GET', '/api/v3/research?cursor=%7B%22id%22%3A4%7D'], ['POST', '/api/v3/research/42/stop'], ['POST', '/api/v3/observations'], ['GET', '/api/v3/observations'], ['PATCH', '/api/v3/settings'], ['PATCH', '/api/v3/extensions/qz.research-native'],
    ]);
    assert.deepEqual(output.at(-3), { id: 'o-1', cash: null });
    for (const cores of ['4', '12']) {
      await run(['settings', 'set', '--tick-book-cores', cores], () => {});
      assert.deepEqual(JSON.parse(sent.at(-1)!.bytes), { tickBookCores: Number(cores) });
    }
    for (const cores of ['0', '1.2', '-1']) await assert.rejects(run(['settings', 'set', `--tick-book-cores=${cores}`]), /tick_book_cores_invalid/);
    assert.ok((await schemas() as { names: string[] }).names.includes('ResearchStart'));
    assert.ok((await schemas('NativeStrategyBacktestRequestV1', true) as { schema: unknown }).schema);
  } finally { await config.close(); await api.close(); }
});

test('bounded wait uses native settled submissions, not final/IDLE/empty tasks; detach never requests stop', async () => {
  let status: string | null = 'placed'; const calls: string[] = [];
  const api = await server((request, response) => {
    calls.push(`${request.method} ${request.url}`);
    response.setHeader('content-type', 'text/event-stream');
    response.write(`event: research\ndata: ${JSON.stringify({ id: 42, status: 'IDLE', tasks: [], final: { status: 'COMPUTED' }, ...(status ? { submission: { id: 7, status } } : {}) })}\n\n`);
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
    assert.ok(calls.every((call) => call === 'GET /api/v3/research/42/events'));
  } finally { await config.close(); await api.close(); }
});

test('SSE abort cancels a pending body read after garbage collection', async () => {
  const requests: string[] = [];
  const api = await server((request, response) => {
    requests.push(request.method!);
    response.setHeader('content-type', 'text/event-stream');
    response.write('data: {"running":true}\n\n');
  });
  const script = `
    import assert from 'node:assert/strict';
    import { setTimeout as delay } from 'node:timers/promises';
    import { connect } from ${JSON.stringify(new URL('../src/client.ts', import.meta.url).href)};
    const client = await connect(${JSON.stringify(profile(api.origin))});
    try {
      const controller = new AbortController();
      const events = client.events('/research/42/events', controller.signal);
      assert.deepEqual((await events.next()).value, { running: true });
      const pending = events.next();
      await delay(50);
      global.gc(); global.gc();
      await delay(50);
      controller.abort();
      const result = await pending.catch(error => {
        assert.equal(error.name, 'AbortError');
        return { done: true, value: undefined };
      });
      assert.deepEqual(result, { done: true, value: undefined });
    } finally { await client.close(); }
  `;
  try {
    const child = spawn(process.execPath, ['--expose-gc', '--experimental-strip-types', '--input-type=module', '-e', script],
      { stdio: ['ignore', 'pipe', 'pipe'], timeout: 5000 });
    let error = ''; child.stderr.on('data', part => { error += part; });
    const [code, signal] = await once(child, 'exit');
    assert.equal(signal, null, error); assert.equal(code, 0, error);
    assert.deepEqual(requests, ['GET']);
  } finally { await api.close(); }
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
    await run(['research', 'resume', '508', '--submission-id', '516', '--instruction', 'Recompute the original candidates'], emit);
    assert.deepEqual(JSON.parse(sent[3]!.bytes), { submissionId: 516, instruction: 'Recompute the original candidates' });
  } finally { await config.close(); await api.close(); }
});

test('account capital exit schemas retain safety actions after research capacity retirement', async () => {
  const schema = await schemas('CapitalExitPreviewInput');
  assert.equal(schema.entry, 'CapitalExitPreviewInput');
  assert.match(schema.source!, /RECONCILE_EXTERNAL_TRANSFER/);
  assert.match(schema.source!, /NATURAL_RELEASE/);
});

test('retired factor and portfolio creation commands fail locally without issuing writes', async () => {
  const sent: string[] = [];
  const api = await server((_request, response, bytes) => { sent.push(bytes); json(response, { received: true }); });
  const config = await configuration(api.origin);
  try {
    for (const args of [['factor', 'capacity', 'r2.t3.m0'], ['factor','list'], ['factor','delete','id'], ['portfolio','build'], ['portfolio','capacity','p']]) await assert.rejects(run(args), /unknown_/);
    assert.deepEqual(sent, []);
  } finally { await config.close(); await api.close(); }
});

test('native research and account-control commands preserve original file bytes and concrete routes', async () => {
  const sent: { method: string; url: string; bytes: string }[] = [];
  const api = await server((request, response, bytes) => { sent.push({ method: request.method!, url: request.url!, bytes }); json(response, { received: true }); });
  const config = await configuration(api.origin); const file = join(config.directory, 'original.json');
  try {
    const bytes = JSON.stringify({ requestId: 'original-native', goal: 'Run native indicators', model: { provider: 'faux', modelId: 'original' }, datasetId: 'registered' }, null, 2) + '\n'; await writeFile(file, bytes);
    await run(['research', 'start', '--file', file], () => {}); assert.equal(sent[0]!.bytes, bytes);
    const preview = '{ "requestId":"preview-original", "amount":"100.000000000000000001", "currency":"USD" }\n'; await writeFile(file, preview);
    await run(['capital', 'status', 'o-original'], () => {});
    await run(['capital', 'context', 'o-original'], () => {});
    await run(['capital', 'get', 'o-original', 'exit/original'], () => {});
    await assert.rejects(run(['capital', 'get', 'o-original'], () => {}), /resource_id_required/);
    await assert.rejects(run(['capital', 'get', 'o-original', 'exit-original', 'extra'], () => {}), /unexpected_argument/);
    await run(['capital', 'preview', 'o-original', '--file', file], () => {}); assert.equal(sent.at(-1)!.bytes, preview);
    const command = '{"requestId":"command-original","action":"START","previewId":"native-original"}\n'; await writeFile(file, command); await run(['capital', 'command', 'o-original', '--file', file], () => {}); assert.equal(sent.at(-1)!.bytes, command);
    const policy = '{"enabled":true,"cashBuffer":"0.000000000000000001","maximumSnapshotAgeSeconds":60}\n'; await writeFile(file, policy); const sentBeforeRetired = sent.length; await assert.rejects(run(['observation', 'automatic', 'o-original', '--file', file], () => {}), /unknown_observation_command/); assert.equal(sent.length, sentBeforeRetired);
    assert.deepEqual(sent.slice(1).map(row => [row.method, row.url]), [
      ['GET', '/api/v3/observations/o-original/capital-exits'],
      ['GET', '/api/v3/observations/o-original/capital-exits/planning-context'], ['GET', '/api/v3/observations/o-original/capital-exits/exit%2Foriginal'],
      ['POST', '/api/v3/observations/o-original/capital-exits/preview'], ['POST', '/api/v3/observations/o-original/capital-exits/commands'],
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
    for (const command of [['pm', 'delete'], ['research', 'delete'], ['portfolio', 'delete']]) await assert.rejects(run(command), /resource_id_required|native_numeric_research_id_required/);
    await assert.rejects(run(['research', 'delete', '9007199254740993']), /native_numeric_research_id_required/);
    assert.equal(sent.length, 0);
    await assert.rejects(run(['research', 'delete', '508'], value => { output.push(value); }), /write_outcome_unknown_retain_original_request/);
    assert.equal(sent.length, 1); assert.equal(output.length, 0);
    await run(['research', 'delete', '508'], value => { output.push(value); });
    assert.deepEqual(output, [receipt]);
    for (const args of [['pm', 'delete', 'mandate:one/two'], ['portfolio', 'delete', 'p/original'], ['backtest', 'delete', 'p:alias']]) await run(args, () => {});
    await assert.rejects(run(['portfolio', 'delete', 'blocked']), error => {
      assert.equal((error as { code: string }).code, 'portfolio_in_use'); assert.equal((error as { status: number }).status, 409); return true;
    });
    assert.deepEqual(sent, [
      { method: 'DELETE', url: '/api/v3/research/508', bytes: '' },
      { method: 'DELETE', url: '/api/v3/research/508', bytes: '' },
      { method: 'DELETE', url: '/api/v3/mandates/mandate%3Aone%2Ftwo', bytes: '' },
      { method: 'DELETE', url: '/api/v3/portfolios/p%2Foriginal', bytes: '' },
      { method: 'DELETE', url: '/api/v3/portfolios/p%3Aalias', bytes: '' },
      { method: 'DELETE', url: '/api/v3/portfolios/blocked', bytes: '' },
    ]);
    const child = spawn(process.execPath, [fileURLToPath(new URL('../bin/quazonai.mjs', import.meta.url)), '--help'], { stdio: ['ignore', 'pipe', 'pipe'] });
    let help = ''; child.stdout.on('data', part => { help += part; }); const [code] = await once(child, 'exit');
    assert.equal(code, 0);
    for (const line of [/pm list \| get ID \| delete ID/, /research models.*delete ID/, /portfolio list.*delete ID/]) assert.match(help, line);
    assert.doesNotMatch(help, /replay-ticks|tick-replay/);
    assert.match(help, /Equity metric IDs retain source and 252\/365 frequency/);
  } finally { await config.close(); await api.close(); }
});


test('retired economic replay commands are refused without reading payloads or sending HTTP', async () => {
  const requests: string[] = [];
  const api = await server((request, response) => {
    requests.push(`${request.method} ${request.url}`);
    json(response, { error: 'unexpected_request' }, 500);
  });
  const config = await configuration(api.origin);
  try {
    for (const group of ['portfolio', 'backtest']) {
      await assert.rejects(run([group, 'replay-ticks', '--file', join(config.directory, 'must-not-be-read.json')]), { code: 'unknown_portfolio_command' });
      await assert.rejects(run([group, 'tick-replay', 'original-id']), { code: 'unknown_portfolio_command' });
    }
    assert.deepEqual(requests, []);
  } finally { await config.close(); await api.close(); }
});

test('portfolio watch reads running progress and partial equity until the same portfolio settles', async () => {
  const requests: string[] = [];
  const native = await server((request, response) => {
    requests.push(`${request.method} ${request.url}`);
    const finished = requests.length > 1;
    response.setHeader('content-type', 'text/event-stream');
    response.end(`event: portfolio\ndata: ${JSON.stringify({ id: 'running-id', status: finished ? 'COMPUTED' : 'RUNNING', progress: { phase: finished ? 'COMPLETED' : 'SIMULATING', updatedAt: '2026-10-29T00:00:00Z' }, curve: [{ at: '2026-10-29T00:00:00Z', value: '100.000000000000000001' }] })}\n\n`);
  });
  const config = await configuration(native.origin);
  try {
    const receipts: any[] = [];
    assert.equal(await run(['portfolio', 'watch', 'running/id', '--timeout', '5'], value => receipts.push(value)), 0);
    assert.equal(receipts[0].status, 'RUNNING');
    assert.equal(receipts[0].progress.phase, 'SIMULATING');
    assert.equal(receipts[0].curve[0].value, '100.000000000000000001');
    assert.equal(receipts.at(-1).settled, true);
    assert.deepEqual(requests, ['GET /api/v3/portfolios/running%2Fid/events', 'GET /api/v3/portfolios/running%2Fid/events']);
  } finally { await config.close(); await native.close(); }
});

test('backtest wait timeout detaches without canceling or restarting the running portfolio', async () => {
  const requests: string[] = [];
  const native = await server((request, response) => {
    requests.push(request.method!);
    response.setHeader('content-type', 'text/event-stream'); response.write(`event: portfolio\ndata: ${JSON.stringify({ id: 'running-id', status: 'RUNNING', curve: [] })}\n\n`);
  });
  const config = await configuration(native.origin);
  try {
    const receipts: any[] = [];
    assert.equal(await run(['backtest', 'wait', 'running-id', '--timeout', '0.05'], value => receipts.push(value)), 124);
    assert.equal(receipts.at(-1).settled, false);
    assert.equal(receipts.at(-1).cancellationRequested, false);
    assert.deepEqual(requests, ['GET']);
  } finally { await config.close(); await native.close(); }
});


test('research goal flags reject retired execution quota options before admission', async () => {
  const sent: Record<string, unknown>[] = [];
  const api = await server((_request, response, bytes) => { sent.push(JSON.parse(bytes)); json(response, { conversationId: 41 }, 202); });
  const config = await configuration(api.origin);
  try {
    const args = ['research', 'start', '--goal', 'Uncapped original question', '--dataset', 'registered', '--model', 'faux:original', '--request-id', 'uncapped-original'];
    await run(args, () => {});
    await assert.rejects(run([...args, '--max-trials', '1'], () => {}), /Unknown option/);
    assert.deepEqual(sent[0], { requestId: 'uncapped-original', goal: 'Uncapped original question', datasetId: 'registered', model: { provider: 'faux', modelId: 'original' } });
    assert.equal(sent.length, 1);
  } finally { await config.close(); await api.close(); }
});

test('observation CLI rejects retired portfolio admission but preserves account binding and protection cancellation', async () => {
  const sent: { method: string; url: string; bytes: string }[] = [];
  const api = await server((request, response, bytes) => { sent.push({ method: request.method!, url: request.url!, bytes }); json(response, { settled: false }); });
  const config = await configuration(api.origin); const file = join(config.directory, 'bracket.json');
  try {
    const bytes = JSON.stringify({ requestId: 'explicit-bracket', portfolioId: 'original', connectionId: 'paper', mode: 'paper', nativeBrackets: {
      'BTC-USDC-SPOT.HYPERLIQUID': { enabled: true, stop_loss: '90.000000000000000001', take_profit: '110.000000000000000001' },
    } });
    await writeFile(file, bytes);
    assert.match(JSON.stringify(await schemas('NativeTargetBrackets')), /NativeTargetBrackets/);
    await assert.rejects(run(['observation', 'start', '--file', file], () => {}), /native_strategy_only_account_binding_required/);
    assert.equal(sent.length, 0);
    const account = '{ "requestId":"account-original", "purpose":"ACCOUNT_ONLY", "connectionId":"paper", "mode":"paper" }\n';
    await writeFile(file, account);
    await run(['observation', 'start', '--file', file], () => {});
    assert.equal(sent[0]!.bytes, account);
    await run(['observation', 'cancel-protection', 'original/observation'], () => {});
    assert.equal(sent[1]!.method, 'POST');
    assert.equal(sent[1]!.url, '/api/v3/observations/original%2Fobservation/cancel-protection');
    await assert.rejects(run(['observation', 'cancel-protection'], () => {}), /resource_id_required/);
  } finally { await config.close(); await api.close(); }
});

test('research get retains original advisory reports and evidence links without deriving stars', async () => {
  const detail = { id: 42, assessments: [{ taskId: 61, mode: 'SYNTHESIS', configuration: 'HISTORICAL', status: 'completed', error: null,
    result: { judgment: { stars: null, rationale: 'Actual validation is missing' }, dataJudgment: { stars: null }, evidence: { requestWindowNs: '1790000000000000001' } },
    evidenceLinks: [{ taskId: 61, href: '/api/v3/research/42/evidence/61' }] }] };
  const api = await server((request, response) => { assert.equal(request.url, '/api/v3/research/42'); json(response, detail); });
  const config = await configuration(api.origin);
  try { let output: unknown; assert.equal(await run(['research', 'get', '42'], value => { output = value; }), 0); assert.deepEqual(output, detail); }
  finally { await config.close(); await api.close(); }
});
