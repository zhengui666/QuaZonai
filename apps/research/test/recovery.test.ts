import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, readFile, appendFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import type { SubmissionId } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { evaluateTrial } from '../src/trials.ts';
import type { ResearchInput } from '../src/documents.ts';

for (const replay of ['safe', 'unsafe', 'failed'] as const) {
  test(`SIGKILL recovery retains original identity and upstream ${replay} tool semantics`, { timeout: 15_000 }, async () => {
    const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-crash-test-'));
    const child = spawn(process.execPath, ['--experimental-strip-types', fileURLToPath(new URL('./process-worker.ts', import.meta.url)), directory, replay], { stdio: ['ignore', 'pipe', 'pipe'] });
    let receipt!: { conversationId: number; submissionId: number }; let tail = ''; let errors = '';
    child.stderr.on('data', (chunk) => { errors += chunk; });
    try {
      await new Promise<void>((resolve, reject) => {
        child.on('exit', (code, signal) => reject(new Error(`probe exited before kill: ${code}/${signal}: ${errors}`)));
        child.stdout.on('data', (chunk) => {
          tail += chunk;
          let index: number;
          while ((index = tail.indexOf('\n')) >= 0) {
            const message = JSON.parse(tail.slice(0, index)); tail = tail.slice(index + 1);
            if (message.conversationId !== undefined) receipt = message;
            if (message.entered !== undefined) resolve();
          }
        });
      });
      const exit = once(child, 'exit'); child.kill('SIGKILL');
      assert.deepEqual(await exit, [null, 'SIGKILL']);
      const original = (await readFile(join(directory, 'calls'), 'utf8')).trim();
      let resumedCalls = 0;
      const faux = fauxProvider({ models: [{ id: 'crash-probe' }], tokensPerSecond: 100_000 });
      const models = createModels(); models.setProvider(faux.provider);
      faux.setResponses([fauxAssistantMessage('Recovered the original research.')]);
      const plugin = defineExtension({ name: 'crash-probe', tools: [defineTool({
        name: 'crash_probe', description: 'Protocol test only', parameters: Type.Object({}), replay: replay === 'unsafe' ? 'unsafe' : 'safe',
        async execute(_args, api, context) {
          const compute = async () => {
            resumedCalls++;
            await appendFile(join(directory, 'calls'), `${api.taskId}\n`);
            return { original: 'Original computation recovered' };
          };
          if (replay === 'failed') await evaluateTrial(api, context, { hypothesis: 'Original scientific failure' }, compute);
          else await compute();
          return { content: [{ type: 'text', text: 'Original computation recovered' }] };
        },
      })] });
      const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [plugin] });
      try {
        const submission = (await host.harness.submission(receipt.submissionId as SubmissionId, BACKGROUND_CONTEXT))!;
        assert.equal((await submission.wait(BACKGROUND_CONTEXT)).status, 'done');
        assert.equal(resumedCalls, replay === 'safe' ? 1 : 0);
        const calls = (await readFile(join(directory, 'calls'), 'utf8')).trim().split('\n');
        assert.ok(calls.every((id) => id === original));
        assert.equal((await host.list()).items.length, 1);
        const view = await host.view(receipt.conversationId);
        const serialized = JSON.stringify(view.entries);
        assert.ok(serialized.includes(replay === 'safe' ? 'Original computation recovered' : replay === 'failed' ? 'original_native_failure' : 'interrupted'));
        if (replay === 'failed') {
          assert.equal(view.research!.trials.length, 1);
          assert.equal(view.research!.trials[0]!.status, 'FAILED');
          assert.equal(view.research!.trials[0]!.error, 'original_native_failure');
          assert.equal(view.research!.trials[0]!.result, null);
          assert.deepEqual(calls, [original]);
        }
        assert.equal((await host.tasks()).tasks.length, 0);
      } finally { await host.close(); }
    } finally { child.kill('SIGKILL'); await rm(directory, { recursive: true, force: true }); }
  });
}

test('SIGKILL after admission is repaired automatically on reopen, without replaying final/stopped studies', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-admission-crash-'));
  const child = spawn(process.execPath, ['--experimental-strip-types', fileURLToPath(new URL('./process-worker.ts', import.meta.url)), directory, 'admission'], { stdio: ['ignore', 'pipe', 'pipe'] });
  let errors = ''; child.stderr.on('data', (part) => { errors += part; });
  try {
    const original = await new Promise<{ ids: number[]; input: ResearchInput }>((resolve, reject) => {
      let bytes = '';
      child.once('exit', (code) => reject(new Error(`admission child exited ${code}: ${errors}`)));
      child.stdout.on('data', (part) => { bytes += part; if (bytes.includes('\n')) resolve(JSON.parse(bytes.slice(0, bytes.indexOf('\n')))); });
    });
    const exit = once(child, 'exit'); child.kill('SIGKILL'); assert.deepEqual(await exit, [null, 'SIGKILL']);
    const faux = fauxProvider({ models: [{ id: 'crash-probe' }], tokensPerSecond: 100000 });
    const models = createModels(); models.setProvider(faux.provider); faux.setResponses([fauxAssistantMessage('Original admission recovered.')]);
    let host = await openResearch({ database: join(directory, 'research.sqlite'), models });
    try {
      const repaired = (await host.submissionFor(original.ids[0]!))!;
      assert.equal((await (await host.harness.submission(repaired.id, BACKGROUND_CONTEXT))!.wait(BACKGROUND_CONTEXT)).status, 'done');
      assert.equal(repaired.requestId, 'admission-gap');
      assert.deepEqual((await host.view(original.ids[0]!)).research!.input, original.input);
      assert.equal(await host.submissionFor(original.ids[1]!), null); assert.equal(await host.submissionFor(original.ids[2]!), null);
      assert.equal(faux.state.callCount, 1);
      await host.close(); host = await openResearch({ database: join(directory, 'research.sqlite'), models });
      assert.deepEqual(await host.start('admission-gap', original.input), { conversationId: original.ids[0], submissionId: repaired.id });
      assert.equal(faux.state.callCount, 1);
      await assert.rejects(host.start('stopped-gap', original.input), /research_already_final/);
    } finally { await host.close(); }
  } finally { child.kill('SIGKILL'); await rm(directory, { recursive: true, force: true }); }
});
