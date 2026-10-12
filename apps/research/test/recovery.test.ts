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
