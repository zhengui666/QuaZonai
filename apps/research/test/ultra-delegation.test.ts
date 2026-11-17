import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxAssistantMessage, fauxProvider, fauxThinking, fauxText, fauxToolCall } from '@earendil-works/pi-ai';
import { createRegistry, Harness, ProviderDoc } from '@earendil-works/pi-durable';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import ultraDelegation from '../plugins/ultra-delegation.ts';
import { readUltraDelegations } from '../src/ultra-delegation-view.ts';
import { ModelTiers } from '../src/models.ts';

const deferred = () => { let resolve!: () => void; const promise = new Promise<void>(done => { resolve = done; }); return { resolve, promise }; };
async function setup() {
  const directory = await mkdtemp(join(tmpdir(), 'qz-ultra-delegation-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000, models: [{ id: 'delegate-model', reasoning: true }] });
  faux.getModel().thinkingLevelMap = { high: 'high' };
  const models = createModels(); models.setProvider(faux.provider);
  const registry = createRegistry(); registry.install(ultraDelegation);
  const open = async () => Harness.open(await openNodeSqliteStorage(join(directory, 'research.sqlite')), { models, registry }, context);
  let harness = await open();
  const parent = await harness.createConversation({ ownership: { kind: 'ownerless' }, agent: { model: { provider: faux.provider.id, modelId: faux.getModel().id }, thinkingLevel: 'high', extensions: [ultraDelegation], instructions: 'PARENT_PRIVATE_CONTEXT' } }, context);
  await parent.commit(async tx => { (await tx.doc(ModelTiers)).tiers[(await tx.doc(ProviderDoc, parent.id)).sessionId] = 'priority'; }, context);
  return { directory, faux, parent, get harness() { return harness; }, async reopen() { await harness.close(context); harness = await open(); return (await harness.conversation(parent.id, context))!; }, async close() { await harness.close(context); await rm(directory, { recursive: true, force: true }); } };
}
async function children(harness: Harness, parentId: number) {
  return harness.commit(async tx => (await tx.scanConversations({}, 100)).items.filter(item => item.id !== parentId && item.owner !== undefined), context);
}

test('Ultra native tools delegate in parallel with isolated context, original identities and inherited effort/tier', { timeout: 10000 }, async () => {
  const state = await setup(); const both = deferred(); let entered = 0;
  const child = async (input: any, options: any) => {
    assert.equal(options.reasoning, 'high');
    assert.equal(input.tools?.length ?? 0, 0);
    assert.ok(!JSON.stringify(input).includes('PARENT_PRIVATE_CONTEXT'));
    assert.ok(!JSON.stringify(input).includes('Ultra mode: proactively'));
    if (++entered === 2) both.resolve();
    await both.promise;
    return fauxAssistantMessage([fauxThinking('PRIVATE_CHILD_REASONING'), fauxText('Independent analysis from supplied evidence only.')]);
  };
  state.faux.setResponses([
    fauxAssistantMessage([fauxToolCall('ultra_delegate', { task: 'Review supplied hypothesis A.' }), fauxToolCall('ultra_delegate', { task: 'Review supplied hypothesis B.' })], { stopReason: 'toolUse' }),
    child, child,
    input => { assert.ok(JSON.stringify(input).includes('Independent analysis')); return fauxAssistantMessage('Integrated both analyses, not empirical validation.'); },
  ]);
  try {
    const receipt = await state.parent.submit({ type: 'input', content: 'Analyze evidence', requestId: 'parallel-original' }, context);
    assert.equal((await receipt.wait(context)).status, 'done'); assert.equal(entered, 2);
    const found = await children(state.harness, state.parent.id); assert.equal(found.length, 2);
    assert.notEqual(found[0]!.owner?.taskId, found[1]!.owner?.taskId);
    for (const item of found) {
      const handle = (await state.harness.conversation(item.id, context))!;
      const agent = await handle.agent(context); assert.equal(agent.thinkingLevel, 'high'); assert.equal(agent.tools.length, 0);
      const tier = await state.harness.commit(async tx => (await tx.doc(ModelTiers)).tiers[(await tx.doc(ProviderDoc, item.id)).sessionId], context);
      assert.equal(tier, 'priority');
    }
    const reopened = await state.reopen();
    const duplicate = await reopened.submit({ type: 'input', content: 'Analyze evidence', requestId: 'parallel-original' }, context);
    assert.equal(duplicate.id, receipt.id); assert.equal((await duplicate.wait(context)).status, 'done');
    assert.equal((await children(state.harness, state.parent.id)).length, 2); assert.equal(state.faux.state.callCount, 4);
    const view = await readUltraDelegations(state.harness, state.parent.id, context);
    assert.equal(view.length, 2); assert.ok(view.every(row => row.status === 'done' && row.effectiveLevel === 'high' && row.answer === 'Independent analysis from supplied evidence only.'));
    assert.ok(!JSON.stringify(view).includes('PRIVATE_CHILD_REASONING')); assert.ok(!JSON.stringify(view).includes('PARENT_PRIVATE_CONTEXT'));
  } finally { both.resolve(); await state.close(); }
});

test('Ultra child failure returns its original non-success status without an invented answer', { timeout: 10000 }, async () => {
  const state = await setup();
  state.faux.setResponses([
    fauxAssistantMessage(fauxToolCall('ultra_delegate', { task: 'Analyze supplied material.' }), { stopReason: 'toolUse' }),
    fauxAssistantMessage('', { stopReason: 'error', errorMessage: '400 delegated request rejected' }),
    input => {
      const tool = input.messages.findLast(message => message.role === 'toolResult');
      assert.ok(tool && tool.role === 'toolResult'); assert.equal(tool.isError, true);
      const result = JSON.parse(tool.content.filter(part => part.type === 'text').map(part => part.text).join(''));
      assert.equal(result.status, 'unanswered'); assert.equal(result.answer, null);
      return fauxAssistantMessage('Delegation failed; no child answer was obtained.');
    },
  ]);
  try { assert.equal((await (await state.parent.submit({ type: 'input', content: 'Review', requestId: 'failure' }, context)).wait(context)).status, 'done'); }
  finally { await state.close(); }
});

test('stopping the parent aborts its foreground child and leaves no background work', { timeout: 10000 }, async () => {
  const state = await setup(); const entered = deferred(); const release = deferred();
  state.faux.setResponses([
    fauxAssistantMessage(fauxToolCall('ultra_delegate', { task: 'Analyze until parent stops.' }), { stopReason: 'toolUse' }),
    async (_input, options) => { entered.resolve(); await Promise.race([release.promise, new Promise<void>(resolve => { if (options?.signal?.aborted) resolve(); else options?.signal?.addEventListener('abort', () => resolve(), { once: true }); })]); return fauxAssistantMessage('Late child result'); },
  ]);
  try {
    const receipt = await state.parent.submit({ type: 'input', content: 'Review', requestId: 'stop' }, context);
    await entered.promise;
    await state.parent.abort(context);
    assert.notEqual((await receipt.wait(context)).status, 'done');
    const found = await children(state.harness, state.parent.id); assert.equal(found.length, 1);
    await (await state.harness.conversation(found[0]!.id, context))!.waitForIdle(context);
    const tasks = await state.harness.inspect(context); assert.equal(tasks.tasks.length, 0);
    const view = await readUltraDelegations(state.harness, state.parent.id, context); assert.equal(view[0]!.status, 'aborted'); assert.equal(view[0]!.answer, null);
  } finally { release.resolve(); await state.close(); }
});

test('interrupted Ultra tool resumes its original child and submission after SQLite restart', { timeout: 10000 }, async () => {
  const state = await setup(); const entered = deferred(); const release = deferred();
  state.faux.setResponses([
    fauxAssistantMessage(fauxToolCall('ultra_delegate', { task: 'Original delegated evidence review.' }), { stopReason: 'toolUse' }),
    async (_input, options) => { entered.resolve(); await Promise.race([release.promise, new Promise<void>(resolve => { if (options?.signal?.aborted) resolve(); else options?.signal?.addEventListener('abort', () => resolve(), { once: true }); })]); return fauxAssistantMessage('Interrupted attempt, not a retained answer.'); },
  ]);
  try {
    const original = await state.parent.submit({ type: 'input', content: 'Original parent request', requestId: 'restart-original' }, context);
    await entered.promise;
    const before = await children(state.harness, state.parent.id); assert.equal(before.length, 1);
    const childSubmission = await state.harness.commit(async tx => (await tx.submissionByRequest(before[0]!.id, `ultra-delegate:${before[0]!.owner!.taskId}`))!, context);
    const reopened = await state.reopen();
    state.faux.setResponses([
      (input, options) => { assert.equal(options?.reasoning, 'high'); assert.ok(JSON.stringify(input).includes('Original delegated evidence review.')); return fauxAssistantMessage('Recovered original child answer.'); },
      input => { assert.ok(JSON.stringify(input).includes('Recovered original child answer.')); return fauxAssistantMessage('Integrated recovered child.'); },
    ]);
    const resumed = await reopened.submit({ type: 'input', content: 'Original parent request', requestId: 'restart-original' }, context);
    assert.equal(resumed.id, original.id); assert.equal((await resumed.wait(context)).status, 'done');
    const after = await children(state.harness, state.parent.id);
    assert.deepEqual(after.map(item => item.id), before.map(item => item.id));
    const retainedSubmission = await state.harness.commit(async tx => tx.submissionByRequest(after[0]!.id, `ultra-delegate:${after[0]!.owner!.taskId}`), context);
    assert.equal(retainedSubmission!.id, childSubmission.id);
    assert.equal(state.faux.state.callCount, 4);
  } finally { release.resolve(); await state.close(); }
});

test('invalid delegation remains a failed tool result without a fabricated child', { timeout: 10000 }, async () => {
  const state = await setup();
  state.faux.setResponses([
    fauxAssistantMessage(fauxToolCall('ultra_delegate', { task: '' }), { stopReason: 'toolUse' }),
    fauxAssistantMessage('No child was started because the assignment was invalid.'),
  ]);
  try {
    assert.equal((await (await state.parent.submit({ type: 'input', content: 'Invalid assignment contract', requestId: 'invalid' }, context)).wait(context)).status, 'done');
    const view = await readUltraDelegations(state.harness, state.parent.id, context);
    assert.equal(view.length, 1); assert.equal(view[0]!.status, 'failed'); assert.equal(view[0]!.conversationId, null); assert.equal(view[0]!.answer, null); assert.ok(view[0]!.error);
  } finally { await state.close(); }
});
