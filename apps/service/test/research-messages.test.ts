import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AssistantEntry, UserEntry, ToolResultEntry, type EntryRecord } from '@earendil-works/pi-durable';
import { Study } from '../../research/src/documents.ts';
import { researchMessages, researchDetail } from '../src/projections.ts';
import { context, setup, fauxAssistantMessage, fauxToolCall } from './support.ts';
import type { ResearchDetail } from '../../../packages/contracts/src/index.ts';

test('research text projection excludes thinking, tools, internal prompts and provider metadata', () => {
  const message = fauxAssistantMessage('Which period should I use?');
  const assistant = { id: 17, conversationId: 1, kind: AssistantEntry.kind, model: [{ ...message,
    content: [{ type: 'thinking', thinking: 'PRIVATE_REASONING' }, { type: 'text', text: 'Which period should I use?' },
      { type: 'toolCall', id: 'private-call', name: 'private_tool', arguments: { secret: 'PRIVATE_ARGUMENT' } }] }] } as unknown as EntryRecord;
  const entries = [assistant, { ...assistant, id: 18, kind: UserEntry.kind }, { ...assistant, id: 19, kind: ToolResultEntry.kind }] as EntryRecord[];
  assert.deepEqual(researchMessages(entries), [{ entryId: 17, role: 'assistant', text: 'Which period should I use?' }]);
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
    assert.deepEqual(original.messages?.map(message => message.text), ['原生 EMA 研究', question]);
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
        if (data) { const value = JSON.parse(data.slice(6)) as ResearchDetail; if (value.messages?.length === 4) streamed = value; }
      }
    }
    assert.deepEqual(streamed.messages?.map(message => message.text), ['原生 EMA 研究', question, answer, '已收到，使用原会话继续。']);
    assert.equal(new Set(streamed.messages?.map(message => message.entryId)).size, 4);
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
    assert.equal(messages.length, 100); assert.equal(messages[0]!.text, 'Stored text 5');
    assert.ok(response.json<ResearchDetail>().messagesNextCursor);
    assert.equal(messages.at(-1)!.text, 'Stored text 104');
    assert.equal(new Set(messages.map(message => message.entryId)).size, 100);
    assert.deepEqual(messages.map(message => message.entryId), messages.map(message => message.entryId).sort((a, b) => a - b));
  } finally { await state.cleanup(); }
});


test('verified owner intents and ordinary answers survive Service reopen without leaking internal prompts; history is paged',async()=>{
 const state=await setup();
 try{
  state.faux.setResponses([fauxAssistantMessage('First ordinary answer')]);
  const receipt=await state.service.research.start('owner-message-original',{goal:'My original question',model:state.model,data:{catalog:state.directory},policy:{}});
  await (await state.service.research.harness.submission(receipt.submissionId,context))!.wait(context);
  state.faux.setResponses([fauxAssistantMessage('Second ordinary answer')]);
  const resumed=await state.service.app.inject({method:'POST',url:`/api/v3/research/${receipt.conversationId}/resume`,headers:state.headers,payload:{submissionId:receipt.submissionId,instruction:'My exact follow-up'}});
  assert.equal(resumed.statusCode,202,resumed.body);
  await (await state.service.research.harness.submission(resumed.json().submissionId,context))!.wait(context);
  await state.service.research.harness.commit(async tx=>{
   await tx.appendEntry(UserEntry,receipt.conversationId,{model:[{role:'user',content:'INTERNAL_UNVERIFIED_PROMPT_DO_NOT_SHOW',timestamp:Date.now()}]});
  },context);
  const read=async()=>{const response=await state.service.app.inject({url:`/api/v3/research/${receipt.conversationId}/messages`,headers:state.headers});assert.equal(response.statusCode,200,response.body);return response.json();};
  const before=await read();
  assert.deepEqual(before.messages.map((m:any)=>[m.role,m.text]),[['user','My original question'],['assistant','First ordinary answer'],['user','My exact follow-up'],['assistant','Second ordinary answer']]);
  assert.equal(before.messages[0].submissionId,receipt.submissionId);assert.equal(before.messages[0].requestId,'owner-message-original');
  assert.equal(before.messages[2].submissionId,resumed.json().submissionId);
  assert.equal(JSON.stringify(before).includes('INTERNAL_UNVERIFIED'),false);
  assert.equal(JSON.stringify(before).includes('The owner requests'),false);
  assert.equal(before.nextCursor,null);
  await state.restart();assert.deepEqual(await read(),before);
  await state.service.research.harness.commit(async tx=>{for(let i=0;i<105;i++)await tx.appendEntry(AssistantEntry,receipt.conversationId,{model:[fauxAssistantMessage(`Archived answer ${i}`)]});},context);
  const latest=await read();assert.equal(latest.messages.length,100);assert.ok(latest.nextCursor);assert.equal(latest.messages[0].text,'Archived answer 5');
  const earlier=await state.service.app.inject({url:`/api/v3/research/${receipt.conversationId}/messages?cursor=${encodeURIComponent(latest.nextCursor)}`,headers:state.headers});assert.equal(earlier.statusCode,200,earlier.body);
  assert.equal(earlier.json().nextCursor,null);assert.equal(earlier.json().messages[0].text,'My original question');
  const ids=[...earlier.json().messages,...latest.messages].map((m:any)=>m.entryId);assert.equal(new Set(ids).size,ids.length);
  const detail=await state.service.app.inject({url:`/api/v3/research/${receipt.conversationId}`,headers:state.headers});assert.equal(detail.statusCode,200,detail.body);
  assert.deepEqual(detail.json().messages,latest.messages);assert.equal(detail.json().messagesNextCursor,latest.nextCursor);
 }finally{await state.cleanup();}
});

test('HTTP task metadata retains native UNKNOWN while private diagnostics stay private across restart',async()=>{
 const state=await setup();
 const {writeFile,readFile}=await import('node:fs/promises');const {join}=await import('node:path');
 try{
  const binary=join(state.directory,'unknown-runner'),marker=join(state.directory,'process-starts');
  await writeFile(binary,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';appendFileSync(${JSON.stringify(marker)},'one\\n');for await(const part of process.stdin){};process.stderr.write('PRIVATE_PROCESS_DIAGNOSTIC');process.exit(6);`,{mode:0o755});
  const target={kind:'RUST_PROCESS',id:'controlled',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'controlled',precision:'standard'},research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};
  state.faux.setResponses([fauxAssistantMessage(fauxToolCall('native_configured_backtest',{}),{stopReason:'toolUse'})]);
  const receipt=await state.service.research.start('unknown-original',{goal:'Observe original unknown execution',model:state.model,data:{catalog:state.directory,nautilusTarget:target,native_configured_backtest_request:{schema_version:1,name:'controlled',config:{}}},policy:{}});
  const read=async()=>{const r=await state.service.app.inject({url:`/api/v3/research/${receipt.conversationId}`,headers:state.headers});assert.equal(r.statusCode,200,r.body);return r.json();};
  let detail=await read();for(let i=0;i<100&&!detail.tasks.some((t:any)=>t.error==='nautilus_execution_unknown');i++){await new Promise(r=>setTimeout(r,20));detail=await read();}
  const original=detail.tasks.find((t:any)=>t.error==='nautilus_execution_unknown');assert.ok(original);assert.equal(detail.final,null);assert.equal(JSON.stringify(detail).includes('PRIVATE_PROCESS_DIAGNOSTIC'),false);
  assert.equal(detail.executionUncertain,true);assert.equal(detail.executionUncertainty.reason,'nautilus_execution_unknown');
  const originalUncertainty=detail.executionUncertainty;
  const withoutTaskRows=await researchDetail(state.service.research,receipt.conversationId,async()=>[]);assert.deepEqual(withoutTaskRows.tasks,[]);assert.equal(withoutTaskRows.executionUncertain,true);assert.deepEqual(withoutTaskRows.executionUncertainty,originalUncertainty);
  // Preserve a previous computed result while the new original execution remains unresolved.
  await state.service.research.harness.commit(async tx=>{const study=await tx.doc(Study,receipt.conversationId);study.final={status:'COMPUTED',result:{},taskId:1};},context);
  const assertUnknownLists=async()=>{for(const endpoint of ['/api/v3/research','/api/v3/research/list']){const response=await state.service.app.inject({url:endpoint,headers:state.headers});assert.equal(response.statusCode,200,response.body);assert.equal(response.json().items.find((row:any)=>row.id===receipt.conversationId)?.status,'UNKNOWN',endpoint);}};
  await assertUnknownLists();
  const withOldFinal=await read();assert.equal(withOldFinal.final.status,'COMPUTED');assert.equal(withOldFinal.executionUncertain,true);assert.deepEqual(withOldFinal.executionUncertainty,originalUncertainty);
  await state.restart();const reopened=await read();assert.deepEqual(reopened.tasks.find((t:any)=>t.id===original.id),original);assert.equal(reopened.final.status,'COMPUTED');assert.equal(reopened.executionUncertain,true);assert.equal(reopened.executionUncertainty.taskId,originalUncertainty.taskId);assert.ok(['nautilus_execution_unknown','nautilus_cancellation_unknown'].includes(reopened.executionUncertainty.reason));assert.equal(reopened.originalSubmission.requestId,'unknown-original');assert.equal((await readFile(marker,'utf8')).trim(),'one');await assertUnknownLists();
 }finally{await state.cleanup();}
});

test('message cursor rejects malformed and cross-conversation envelopes without exposing history', async () => {
 const state=await setup();
 try {
  state.faux.setResponses([fauxAssistantMessage('Ordinary answer')]);
  const receipt=await state.service.research.start('cursor-validation',{goal:'Original question',model:state.model,data:{},policy:{}});
  await (await state.service.research.harness.submission(receipt.submissionId,context))!.wait(context);
  const encode=(value:unknown)=>Buffer.from(JSON.stringify(value)).toString('base64url');
  for(const cursor of ['not-json',encode(null),encode({conversationId:receipt.conversationId,cursor:[]}),encode({conversationId:receipt.conversationId,cursor:'wrong'}),encode({conversationId:receipt.conversationId+1,cursor:{}})]){
   const response=await state.service.app.inject({url:`/api/v3/research/${receipt.conversationId}/messages?cursor=${encodeURIComponent(cursor)}`,headers:state.headers});
   assert.equal(response.statusCode,422,response.body);
   assert.ok(response.body.includes('research_message_cursor_invalid'));
   assert.equal(response.body.includes('Original question'),false);
  }
 } finally {await state.cleanup();}
});

test('an entirely internal official entry page retains its cursor and reaches verified earlier messages after reopen', async () => {
 const state=await setup();
 try {
  state.faux.setResponses([fauxAssistantMessage('Visible answer')]);
  const receipt=await state.service.research.start('empty-visible-page',{goal:'Visible original question',model:state.model,data:{},policy:{}});
  await (await state.service.research.harness.submission(receipt.submissionId,context))!.wait(context);
  await state.service.research.harness.commit(async tx=>{
   for(let i=0;i<100;i++)await tx.appendEntry(UserEntry,receipt.conversationId,{model:[{role:'user',content:`INTERNAL_PAGE_${i}`,timestamp:Date.now()}]});
  },context);
  const path=`/api/v3/research/${receipt.conversationId}`;
  const read=async(cursor?:string)=>{
   const response=await state.service.app.inject({url:`${path}/messages${cursor?`?cursor=${encodeURIComponent(cursor)}`:''}`,headers:state.headers});
   assert.equal(response.statusCode,200,response.body);return response.json();
  };
  const latest=await read();assert.deepEqual(latest.messages,[]);assert.ok(latest.nextCursor);
  const detail=(await state.service.app.inject({url:path,headers:state.headers})).json();
  assert.deepEqual(detail.messages,[]);assert.equal(detail.messagesNextCursor,latest.nextCursor);
  await state.restart();assert.deepEqual(await read(),latest);
  const earlier=await read(latest.nextCursor);
  assert.equal(earlier.nextCursor,null);
  assert.deepEqual(earlier.messages.map((message:any)=>[message.role,message.text]),[['user','Visible original question'],['assistant','Visible answer']]);
  const ids=earlier.messages.map((message:any)=>message.entryId);assert.equal(new Set(ids).size,ids.length);
  assert.equal(JSON.stringify([latest,earlier]).includes('INTERNAL_PAGE_'),false);
 } finally {await state.cleanup();}
});

test('uncertainty projection respects retained receipt status and exact current owner', async () => {
 const state=await setup();
 const {ResearchConfiguration}=await import('../../research/src/research-configuration.ts');
 const {NativeRemoteUncertainty}=await import('../../research/src/native-execution.ts');
 try {
  state.faux.setResponses([fauxAssistantMessage('Ordinary answer')]);
  const receipt=await state.service.research.start('uncertainty-status-matrix',{goal:'Inspect retained receipt facts',model:state.model,data:{},policy:{}});
  await (await state.service.research.harness.submission(receipt.submissionId,context))!.wait(context);
  await state.service.research.harness.commit(async tx=>{const study=await tx.doc(Study,receipt.conversationId);study.final={status:'COMPUTED',result:{},taskId:1};},context);
  // Projection-only receipt fixtures: no native invocation or execution is created.
  const owner=receipt.submissionId as any;
  for(const [status,error,matches,expected] of [
   ['COMPLETED','nautilus_execution_unknown',true,false],
   ['FAILED','nautilus_execution_unknown',true,false],
   ['CANCELLED','nautilus_cancellation_unknown',true,false],
   ['REJECTED','nautilus_execution_unknown',true,false],
   ['PENDING','nautilus_execution_unknown',true,true],
   ['PENDING',null,true,false],
   ['RUNNING',null,true,false],
   ['UNKNOWN',null,true,true],
   ['UNKNOWN','nautilus_execution_unknown',false,false],
  ] as const){
   await state.service.research.harness.commit(async tx=>{
    const config=await tx.doc(ResearchConfiguration,receipt.conversationId);config.evaluationTaskId=owner;
    const facts=await tx.doc(NativeRemoteUncertainty,receipt.conversationId,String(owner),null);
    facts.requests=[{taskId:matches?Number(owner):Number(owner)+1,status,error}];
   },context);
   const response=await state.service.app.inject({url:`/api/v3/research/${receipt.conversationId}`,headers:state.headers});
   assert.equal(response.statusCode,200,response.body);
   const detail=response.json();assert.equal(detail.executionUncertain,expected,`${status}/${error}/${matches}`);
   if(expected)assert.deepEqual(detail.executionUncertainty,{taskId:Number(owner),reason:'nautilus_execution_unknown'});
   else assert.equal(detail.executionUncertainty,null);
   for(const endpoint of ['/api/v3/research','/api/v3/research/list']){const list=await state.service.app.inject({url:endpoint,headers:state.headers});assert.equal(list.statusCode,200,list.body);assert.equal(list.json().items.find((row:any)=>row.id===receipt.conversationId)?.status,expected?'UNKNOWN':'COMPUTED',`${endpoint}/${status}/${error}/${matches}`);}
   assert.equal(detail.final.status,'COMPUTED');
  }
 } finally {await state.cleanup();}
});
