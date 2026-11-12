import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AssistantEntry, UserEntry, ToolResultEntry, type EntryRecord } from '@earendil-works/pi-durable';
import { researchMessages } from '../src/projections.ts';
import { context, setup, fauxAssistantMessage, fauxToolCall } from './support.ts';
import type { ResearchDetail } from '../../../packages/contracts/src/index.ts';

test('research text projection excludes thinking, tools, internal prompts and provider metadata', () => {
  const message = fauxAssistantMessage('Which period should I use?');
  const assistant = { id: 17, conversationId: 1, kind: AssistantEntry.kind, model: [{ ...message,
    content: [{ type: 'thinking', thinking: 'PRIVATE_REASONING' }, { type: 'text', text: 'Which period should I use?' },
      { type: 'toolCall', id: 'private-call', name: 'private_tool', arguments: { secret: 'PRIVATE_ARGUMENT' } }] }] } as unknown as EntryRecord;
  const entries = [assistant, { ...assistant, id: 18, kind: UserEntry.kind }, { ...assistant, id: 19, kind: ToolResultEntry.kind }] as EntryRecord[];
  assert.deepEqual(researchMessages(entries), [{ entryId: 17, text: 'Which period should I use?' }]);
});

test('original Pi messages reach detail and SSE, accept one exact reply, and survive restart', { timeout: 20_000 }, async () => {
  const state = await setup();
  const question = '请选择 EMA 的快慢周期。'; const answer = '使用 12 和 26。';
  let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  try {
    state.faux.setResponses([fauxAssistantMessage(question)]);
    const started = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
      payload: { requestId: 'pi-text-question', goal: '原生 EMA 研究', model: state.model, datasetId: 'controlled' } });
    assert.equal(started.statusCode, 202, started.body); const receipt = started.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const path = `/api/v3/research/${receipt.conversationId}`;
    const original = (await state.service.app.inject({ url: path, headers: state.headers })).json<ResearchDetail>();
    assert.deepEqual(original.messages?.map(message => message.text), [question]);
    const base = await state.service.app.listen({ host: '127.0.0.1', port: 0 });
    const response = await fetch(`${base}${path}/events`, { headers: state.headers, signal: AbortSignal.timeout(15_000) });
    assert.equal(response.status, 200); reader = response.body!.getReader();
    state.faux.setResponses([transcript => {
      assert.ok(JSON.stringify(transcript).includes(question)); assert.ok(JSON.stringify(transcript).includes(answer));
      return fauxAssistantMessage('已收到，使用原会话继续。');
    }]);
    const payload = { submissionId: receipt.submissionId, instruction: answer };
    const [first, retry] = await Promise.all([0, 1].map(() => state.service.app.inject({ method: 'POST', url: `${path}/resume`, headers: state.headers, payload })));
    assert.equal(first.statusCode, 202, first.body); assert.deepEqual(retry.json(), first.json());
    await (await state.service.research.harness.submission(first.json().submissionId, context))!.wait(context);
    let text = ''; const decoder = new TextDecoder(); let streamed: ResearchDetail | undefined;
    while (!streamed) {
      const next = await reader.read(); assert.equal(next.done, false); text += decoder.decode(next.value, { stream: true });
      for (let boundary; (boundary = text.indexOf('\n\n')) >= 0;) {
        const packet = text.slice(0, boundary); text = text.slice(boundary + 2);
        const data = packet.split('\n').find(line => line.startsWith('data: '));
        if (data) { const value = JSON.parse(data.slice(6)) as ResearchDetail; if (value.messages?.length === 2) streamed = value; }
      }
    }
    assert.deepEqual(streamed.messages?.map(message => message.text), [question, '已收到，使用原会话继续。']);
    assert.equal(new Set(streamed.messages?.map(message => message.entryId)).size, 2);
    await reader.cancel(); reader = undefined;
    await state.restart();
    const restored = (await state.service.app.inject({ url: path, headers: state.headers })).json<ResearchDetail>();
    assert.deepEqual(restored.messages, streamed.messages); assert.equal(state.faux.state.callCount, 2);
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('research_finish', { reason: '暂告一段落' }), { stopReason: 'toolUse' })]);
    const stopped = await state.service.app.inject({ method: 'POST', url: `${path}/resume`, headers: state.headers,
      payload: { submissionId: restored.submission!.id, instruction: '先结束讨论' } });
    assert.equal(stopped.statusCode, 202, stopped.body);
    await (await state.service.research.harness.submission(stopped.json().submissionId, context))!.wait(context);
    state.faux.setResponses([fauxAssistantMessage('继续原来的讨论。')]);
    const continued = await state.service.app.inject({ method: 'POST', url: `${path}/resume`, headers: state.headers,
      payload: { submissionId: stopped.json().submissionId, instruction: '继续讨论' } });
    assert.equal(continued.statusCode, 202, continued.body);
    await (await state.service.research.harness.submission(continued.json().submissionId, context))!.wait(context);
  } finally { await reader?.cancel(); await state.cleanup(); }
});

test('message projection reads official entry pages in original order beyond a single watch window', async () => {
  const state = await setup();
  try {
    state.faux.setResponses([fauxAssistantMessage('First question')]);
    const receipt = await state.service.research.start('message-pages', { goal: 'Read the original history', model: state.model, data: {}, policy: {} });
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    await state.service.research.harness.commit(async tx => {
      for (let index = 0; index < 105; index++) await tx.appendEntry(AssistantEntry, receipt.conversationId,
        { model: [fauxAssistantMessage(`Stored text ${index}`)] });
    }, context);
    const response = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
    assert.equal(response.statusCode, 200, response.body);
    const messages = response.json<ResearchDetail>().messages!;
    assert.equal(messages.length, 106); assert.equal(messages[0]!.text, 'First question');
    assert.equal(messages.at(-1)!.text, 'Stored text 104');
    assert.equal(new Set(messages.map(message => message.entryId)).size, 106);
    assert.deepEqual(messages.map(message => message.entryId), messages.map(message => message.entryId).sort((a, b) => a - b));
  } finally { await state.cleanup(); }
});
