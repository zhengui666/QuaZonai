import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTask, defineTool } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { NativeRemoteUncertainty, executeNativeValue, nativeCommands, withNativeRemoteTracking } from '../src/native-execution.ts';
import { assertNativeRemoteTaskSettled, reconcileNativeRemoteTasks } from '../src/native-remote-receipts.ts';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';

test('only original-id explicit admission refusal settles a Durable remote reference without a terminal receipt', { timeout: 15_000 }, async () => {
  for (const command of ['evaluate-tick-factor', 'replay-tick-economic']) for (const mode of ['explicit', 'generic', 'wrong-id'] as const) {
    const directory = await mkdtemp(join(tmpdir(), 'qz-remote-admission-'));
    let posts = 0; let reads = 0; let original: NativeExecutionRequest | undefined;
    const server = createServer(async (request, response) => {
      response.setHeader('content-type', 'application/json');
      if (request.url === '/v1/capabilities') {
        response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return;
      }
      if (request.method === 'POST') {
        posts++; let data = ''; for await (const chunk of request) data += chunk;
        original = JSON.parse(data) as NativeExecutionRequest; assert.equal(original.command, command);
        response.statusCode = 429;
        response.end(JSON.stringify(mode === 'generic' ? { error: 'too_many_requests' } : {
          schemaVersion: 1, requestId: mode === 'wrong-id' ? '0'.repeat(64) : original.requestId,
          admission: 'REJECTED', error: 'nautilus_tick_capacity_exhausted',
        })); return;
      }
      reads++; response.statusCode = 404; response.end(JSON.stringify({ error: 'nautilus_execution_not_found' }));
    });
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
    const target = { id: 'controlled-pre-admission', url: `http://127.0.0.1:${(server.address() as AddressInfo).port}` };
    const Compute = defineTask<{}, { phase: 'compute' }, null>({
      name: 'test.remote-admission.compute', version: 1, initial: () => ({ phase: 'compute' }),
      phases: { async compute(task, runtime, context) {
        try {
          await withNativeRemoteTracking(task.id, runtime, context, () => executeNativeValue(target, command, {}, [], directory, context));
          assert.fail('controlled endpoint rejected admission');
        } catch (error) {
          await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: String(error) } } }), context);
        }
      } },
      async abort(_task, runtime, context) {
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context);
      },
    });
    const extension = defineExtension({ name: 'test.remote-admission', tasks: [Compute], tools: [defineTool({
      name: 'compute', description: 'Controlled remote admission protocol', parameters: Type.Object({}), replay: 'safe',
      async execute(_args, api, context) {
        const id = await api.commit(tx => tx.createTask(Compute, {}, { ownership: { kind: 'task', taskId: api.taskId } }), context);
        await api.waitForTask(id, context);
        return { content: [{ type: 'text', text: 'Controlled admission finished.' }], control: { terminate: true } };
      },
    })] });
    const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
    faux.setResponses([fauxAssistantMessage(fauxToolCall('compute', {}), { stopReason: 'toolUse' })]);
    const options = { database: join(directory, 'research.sqlite'), models, extensions: [extension] };
    let host = await openResearch(options);
    try {
      const receipt = await host.start(`original-${mode}`, { goal: 'Verify admission evidence only', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} });
      await (await host.harness.submission(receipt.submissionId, BACKGROUND_CONTEXT))!.wait(BACKGROUND_CONTEXT);
      const task = (await host.harness.commit(tx => tx.scanTasks({ kind: 'test.remote-admission.compute' }, 10), BACKGROUND_CONTEXT)).items[0]!;
      assert.equal(task.state.outcome?.status, 'failed');
      const facts = await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT);
      assert.equal(facts?.requests.length, 1);
      const reference = facts!.requests[0]!;
      assert.equal(reference.requestId, original!.requestId);
      if (mode === 'explicit') {
        assert.equal(reference.status, 'REJECTED'); assert.equal(reference.admission, 'REJECTED');
        assert.equal(reference.error, 'nautilus_tick_capacity_exhausted');
        await host.harness.commit(tx => assertNativeRemoteTaskSettled(tx, task.conversationId, task.id), BACKGROUND_CONTEXT);
        await reconcileNativeRemoteTasks(host.harness, [task], BACKGROUND_CONTEXT);
        assert.equal(reads, 0, 'explicit non-execution does not invent or query a terminal execution receipt');
      } else {
        assert.equal(reference.status, 'PENDING');
        await assert.rejects(host.harness.commit(tx => assertNativeRemoteTaskSettled(tx, task.conversationId, task.id), BACKGROUND_CONTEXT), /native_remote_execution_unsettled/);
      }
      await host.close(); host = await openResearch(options);
      assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT), facts);
      assert.equal(posts, 1, 'reopening never resubmits rejected or uncertain work');
    } finally {
      await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
      await rm(directory, { recursive: true, force: true });
    }
  }
});
