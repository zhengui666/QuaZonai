import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxAssistantMessage, fauxProvider } from '@earendil-works/pi-ai';
import { defineDoc } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { Requests, Study } from '../src/documents.ts';

const context = BACKGROUND_CONTEXT;
const Admissions = defineDoc<{ admitted: number }>({ kind: 'qz.test.admission-policy', version: 1, scope: 'session', initial: () => ({ admitted: 0 }), checkpointWhen: () => true });

test('business authorization and intake commit together; replay preserves the frozen input without reauthorizing', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-intake-authority-'));
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage('Completed original input.')]);
  let calls = 0;
  const options = { database: join(directory, 'research.sqlite'), models,
    authorizeAdmission: async (tx: import('@earendil-works/pi-durable').Tx, input: import('../src/documents.ts').ResearchInput, metadata?: import('@earendil-works/pi-durable').JsonObject) => {
      calls++; (await tx.doc(Admissions)).admitted++;
      if (metadata?.blocked) throw new Error('business_admission_blocked');
      input.policy.sourceAuthority = 'frozen-in-admission';
    },
  };
  let host = await openResearch(options);
  const input = { goal: 'Commit the original research admission once', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} };
  try {
    await assert.rejects(host.start('blocked', input, undefined, undefined, { blocked: true }), /business_admission_blocked/);
    assert.equal((await host.harness.snapshot(Admissions, context))?.admitted ?? 0, 0);
    assert.equal((await host.harness.snapshot(Requests, context))?.requests['request:blocked'], undefined);
    assert.equal((await host.tasks()).records.items.length, 0);
    const receipt = await host.start('allowed', input, undefined, undefined, { original: { title: 'Owner request' } });
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    assert.equal((await host.harness.snapshot(Admissions, context))!.admitted, 1);
    assert.equal((await host.harness.snapshot(Study, receipt.conversationId, context))!.input!.policy.sourceAuthority, 'frozen-in-admission');
    assert.deepEqual(input.policy, {}, 'authorization does not mutate the owner supplied object');
    assert.deepEqual(await host.start('allowed', input), receipt);
    assert.equal(calls, 2, 'only the refused command and first real admission run authorization');
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.start('allowed', input), receipt);
    assert.equal(calls, 2); assert.equal(faux.state.callCount, 1);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});
