import assert from 'node:assert/strict';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT, withAbortSignal } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTask, defineTool } from '@earendil-works/pi-durable';
import type { NativeExecutionRequest, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { NativeRemoteUncertainty, retainNativeRemoteUncertainty, withNativeRemoteTracking } from '../../research/src/native-execution.ts';
import { openResearch } from '../../research/src/host.ts';
import { validateCapitalDocument } from '../src/capital-exit.ts';
import { implementationIdentity } from '../src/pm-science.ts';

const digest = `sha256:${'c'.repeat(64)}`;
function signal() { let resolve!: () => void; const promise = new Promise<void>(done => { resolve = done; }); return { promise, resolve }; }
function reply(response: ServerResponse, value: unknown) { response.setHeader('content-type', 'application/json'); response.end(JSON.stringify(value)); }
async function readRequest(request: IncomingMessage): Promise<NativeExecutionRequest> {
  const chunks: Buffer[] = []; for await (const chunk of request) chunks.push(Buffer.from(chunk));
  return JSON.parse(Buffer.concat(chunks).toString('utf8'));
}
async function peer(handler: (request: IncomingMessage, response: ServerResponse) => Promise<void> | void) {
  const failures: unknown[] = [];
  const server = createServer((request, response) => { void Promise.resolve(handler(request, response)).catch(error => {
    failures.push(error); response.statusCode = 500; response.end('controlled_peer_failure');
  }); });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target: RemoteNativeTarget = { id: 'controlled-capital-science', url: `http://127.0.0.1:${address.port}` };
  return { target, failures, async close() { server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); } };
}
async function bounded<T>(promise: Promise<T>, milliseconds = 5_000): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  try { return await Promise.race([promise, new Promise<never>((_resolve, reject) => {
    timer = setTimeout(() => reject(new Error('caller_abort_did_not_settle_remote_helper')), milliseconds);
  })]); } finally { if (timer) clearTimeout(timer); }
}

test('PM implementation identity and capital validation interrupt a connected capabilities request', { timeout: 10_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-helper-capabilities-abort-'));
  try {
    for (const helper of ['implementation', 'capital']) {
      const accepted = signal(); const requests: string[] = []; const controller = new AbortController();
      const source = await peer((request, _response) => {
        requests.push(`${request.method} ${request.url}`); accepted.resolve();
        // Connected, but no headers/body: a connect timeout cannot end this wait.
      });
      const context = withAbortSignal(controller.signal, BACKGROUND_CONTEXT);
      const operation = helper === 'implementation' ? implementationIdentity(source.target, context)
        : validateCapitalDocument({ dataDir: directory, jobBinary: source.target }, 'REQUEST', {}, context);
      const settled = operation.then(() => ({ error: null }), error => ({ error }));
      try {
        await bounded(accepted.promise);
        controller.abort(new Error('owner_stop_capability_probe'));
        const outcome = await bounded(settled);
        assert.match(outcome.error?.message ?? '', /owner_stop_capability_probe/);
        assert.deepEqual(requests, ['GET /v1/capabilities'], 'no native operation was admitted and no replacement request is sent');
        assert.deepEqual(source.failures, []);
      } finally { await source.close(); await settled; }
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('capital validation forwards task abort and cancels only the accepted original request', { timeout: 10_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-capital-request-abort-'));
  const accepted = signal(); const requests: string[] = []; const controller = new AbortController(); let original: NativeExecutionRequest | undefined;
  const source = await peer(async (request, response) => {
    requests.push(`${request.method} ${request.url}`);
    if (request.url === '/v1/capabilities') return reply(response, { schemaVersion: 1, implementationDigest: digest, commands: ['validate-capital-exit'] });
    if (request.method === 'POST') { original = await readRequest(request); accepted.resolve(); return; }
    assert.equal(request.method, 'DELETE'); assert.equal(request.url, `/v1/executions/${original!.requestId}`);
    reply(response, { schemaVersion: 1, requestId: original!.requestId, status: 'CANCELLED' });
  });
  const operation = validateCapitalDocument({ dataDir: directory, jobBinary: source.target }, 'VIEW', { amount: '9007199254740993.0001' }, withAbortSignal(controller.signal, BACKGROUND_CONTEXT));
  const settled = operation.then(() => ({ error: null }), error => ({ error }));
  try {
    await bounded(accepted.promise); controller.abort(new Error('owner_stop_capital_validation'));
    const outcome = await bounded(settled);
    assert.match(outcome.error?.message ?? '', /owner_stop_capital_validation/);
    assert.equal(original!.command, 'validate-capital-exit');
    assert.deepEqual(original!.input, { kind: 'VIEW', value: { amount: '9007199254740993.0001' } });
    assert.deepEqual(requests, ['GET /v1/capabilities', 'POST /v1/execute', `DELETE /v1/executions/${original!.requestId}`]);
    assert.deepEqual(source.failures, []);
  } finally { await source.close(); await settled; await rm(directory, { recursive: true, force: true }); }
});

test('unanswered capital cancellation retains the original reference through Durable stop and reopen', { timeout: 15_000 }, async t => {
  const actualTimeout = AbortSignal.timeout.bind(AbortSignal); const deadlines: number[] = [];
  t.mock.method(AbortSignal, 'timeout', (milliseconds: number) => { deadlines.push(milliseconds); return actualTimeout(milliseconds === 15_000 ? 10 : milliseconds); });
  const directory = await mkdtemp(join(tmpdir(), 'qz-capital-unknown-receipt-')); const accepted = signal();
  let original: NativeExecutionRequest | undefined; let posts = 0; let cancels = 0;
  const source = await peer(async (request, response) => {
    if (request.url === '/v1/capabilities') return reply(response, { schemaVersion: 1, implementationDigest: digest, commands: ['validate-capital-exit'] });
    if (request.method === 'POST') { posts++; original = await readRequest(request); accepted.resolve(); return; }
    assert.equal(request.method, 'DELETE'); assert.equal(request.url, `/v1/executions/${original!.requestId}`); cancels++;
    // Never confirm cancellation. The helper must preserve the unknown identity.
  });
  const validation = defineTask<{ target: RemoteNativeTarget }, { phase: 'validate' }, null>({
    name: 'test.capital-remote-validation', version: 1, initial: () => ({ phase: 'validate' }),
    phases: { async validate(task, runtime, context) {
      try {
        await withNativeRemoteTracking(task.id, runtime, context,
          () => validateCapitalDocument({ dataDir: directory, jobBinary: task.input.target }, 'VIEW', {}, context));
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), context);
      } catch (error) {
        await retainNativeRemoteUncertainty(error, task.id, runtime, context);
        throw error;
      }
    } },
    async abort(_task, runtime, context) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context); },
  });
  const extension = defineExtension({ name: 'test.capital-remote-stop', tasks: [validation], tools: [defineTool({
    name: 'validate_capital', description: 'Controlled capital validation transport', parameters: Type.Object({}), replay: 'safe',
    async execute(_args, api, context) {
      const id = await api.commit(tx => tx.createTask(validation, { target: source.target }, { ownership: { kind: 'task', taskId: api.taskId } }), context);
      await api.waitForTask(id, context); return {};
    },
  })] });
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('validate_capital', {}), { stopReason: 'toolUse' })]);
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [extension] };
  let host = await openResearch(options);
  try {
    const started = await host.start('original-capital-remote-stop', { goal: 'Keep original remote validation reference', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} });
    await bounded(accepted.promise); await bounded(host.stop(started.conversationId));
    const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'test.capital-remote-validation' }, 10), BACKGROUND_CONTEXT);
    assert.equal(tasks.items.length, 1); const task = tasks.items[0]!;
    assert.equal(task.state.outcome?.status, 'aborted'); assert.equal(posts, 1); assert.equal(cancels, 1); assert.ok(deadlines.includes(15_000));
    const receipt = await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT);
    assert.deepEqual(receipt, { requests: [{ taskId: task.id, requestId: original!.requestId, target: source.target, status: 'PENDING', error: 'nautilus_execution_unknown' }] });
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT), receipt);
    assert.equal(posts, 1); assert.deepEqual(source.failures, []);
  } finally { await source.close(); await host.close(); await rm(directory, { recursive: true, force: true }); }
});
