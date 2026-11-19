import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, writeFile, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { type ToolExecutionApi } from '@earendil-works/pi-durable';
import { compile } from '@earendil-works/pi-schema';
import { nativeConfiguredConfigureTool, nativeStrategyBriefTool } from '../plugins/native-strategy-backtest.ts';
import { openResearch } from '../src/host.ts';
import { ResearchConfiguration } from '../src/research-configuration.ts';
import { codingWorkspace } from '../src/coding-workspace.ts';
import { Study } from '../src/documents.ts';
const context = BACKGROUND_CONTEXT;
const prepare = nativeConfiguredConfigureTool.prepareArguments!;
const request = { schema_version: 1, name: 'existing-owner', config: { opaque: 'x'.repeat(522 * 1024), ns: '1776124800000000000', tail: 'COMPLETE_END' } };

test('configured input schema and preparation require exactly one complete object or file', () => {
  const validate = compile(nativeConfiguredConfigureTool.parameters);
  for (const input of [{}, {request, file:'run.json'}]) {
    assert.equal(validate.check(input), false);
    assert.throws(() => prepare(input), /exactly_one_input/);
  }
  assert.equal(validate.check({ request }), true);
  assert.equal(validate.check({ file:'run.json' }), true);
  assert.deepEqual(prepare({ request }), { request });
  for (const input of [{request:{}}, {file:''}, {request:{...request,config:{ns:9007199254740992}}}]) assert.throws(() => prepare(input));
});

test('file input retains complete request before execute; changed/deleted file cannot change original configuration on replay', async () => {
  const directory = await mkdtemp(join(tmpdir(),'qz-config-file-'));
  const workspace = join(directory,'source'); await mkdir(workspace);
  const file = join(workspace,'request.json'); await writeFile(file,JSON.stringify(request));
  const faux = fauxProvider({tokensPerSecond:100000}); const models = createModels(); models.setProvider(faux.provider);
  const coding=await codingWorkspace(workspace);
  const resolved=await (coding.extension.hooks![0]!.handlers as any).beforeTool({name:'native_configured_configure',arguments:{file:'request.json'}});
  const captured=prepare(resolved.arguments);
  assert.deepEqual(captured,{request,compact:true});
  const host=await openResearch({database:join(directory,'research.sqlite'),codingWorkspace:workspace,models});
  try {
    faux.setResponses([fauxAssistantMessage(fauxToolCall('native_configured_configure',{file:'request.json'}),{stopReason:'toolUse'}),fauxAssistantMessage('Configured only.')]);
    const receipt=await host.start('file',{goal:'Configure only',model:{provider:faux.provider.id,modelId:faux.getModel().id},data:{catalog:directory,nautilusTarget:{id:'not-executed',url:'http://127.0.0.1:1'}},policy:{}});
    await (await host.harness.submission(receipt.submissionId,context))!.wait(context);
    const saved=await host.harness.snapshot(ResearchConfiguration,receipt.conversationId,context);
    assert.deepEqual(saved?.request,request); assert.ok(saved?.configuredByTaskId);
    const conversation=(await host.harness.conversation(receipt.conversationId,context))!;
    await writeFile(file,JSON.stringify({...request,name:'changed-after-resolution'}));
    await rm(file);
    const api={conversationId:receipt.conversationId,taskId:saved.configuredByTaskId,commit:conversation.commit.bind(conversation),snapshot:host.harness.snapshot.bind(host.harness)} as ToolExecutionApi;
    const result=await nativeConfiguredConfigureTool.execute(captured,api,context);
    assert.ok(JSON.stringify(result).length<2048); assert.ok(!JSON.stringify(result).includes('COMPLETE_END'));
    assert.deepEqual((await host.harness.snapshot(ResearchConfiguration,receipt.conversationId,context))?.request,request);
    assert.deepEqual((await host.harness.snapshot(Study,receipt.conversationId,context))?.input?.data.native_configured_backtest_request,request);
    const inline=await nativeConfiguredConfigureTool.execute(prepare({request}),api,context);
    assert.deepEqual(JSON.parse((inline.output[0] as any).text).request,request);
    const brief=await nativeStrategyBriefTool.execute({summaryOnly:true},api,context);
    const summary=JSON.parse((brief.output[0] as any).text);
    assert.equal(summary.requestsIncluded,false);
    assert.deepEqual(summary.configuredRequestSummary,{name:request.name});
    assert.equal(summary.configuredRequest,undefined);
    assert.ok(JSON.stringify(brief).length<4096);
    const full=await nativeStrategyBriefTool.execute({},api,context);
    assert.deepEqual(JSON.parse((full.output[0] as any).text).configuredRequest,request);
  } finally {await host.close();await rm(directory,{recursive:true,force:true});}
});

test('invalid JSON/schema/numbers and literal or symlink escapes never create configuration', async () => {
  const directory=await mkdtemp(join(tmpdir(),'qz-config-invalid-')); const workspace=join(directory,'source');await mkdir(workspace);
  await writeFile(join(directory,'outside.json'),JSON.stringify(request));await symlink(join(directory,'outside.json'),join(workspace,'escape.json'));
  await writeFile(join(workspace,'invalid.json'),'{');await writeFile(join(workspace,'schema.json'),'{}');
  await writeFile(join(workspace,'unsafe.json'),'{"schema_version":1,"name":"owner","config":{"ns":1776124800000000000}}');
  const faux=fauxProvider({tokensPerSecond:100000});const models=createModels();models.setProvider(faux.provider);
  const host=await openResearch({database:join(directory,'research.sqlite'),codingWorkspace:workspace,models});
  try {
    for (const file of ['../outside.json','escape.json','invalid.json','schema.json','unsafe.json','~outside.json']) {
      faux.setResponses([fauxAssistantMessage(fauxToolCall('native_configured_configure',{file}),{stopReason:'toolUse'}),fauxAssistantMessage('No configuration.')]);
      const receipt=await host.start(file,{goal:'Configure only',model:{provider:faux.provider.id,modelId:faux.getModel().id},data:{catalog:directory,nautilusTarget:{id:'not-executed',url:'http://127.0.0.1:1'}},policy:{}});
      await (await host.harness.submission(receipt.submissionId,context))!.wait(context);
      assert.equal((await host.harness.snapshot(ResearchConfiguration,receipt.conversationId,context))?.request ?? null,null,file);
    }
  } finally {await host.close();await rm(directory,{recursive:true,force:true});}
});

 test('file input without a coding workspace cannot read or configure', async () => {
  let commits=0;
  await assert.rejects(nativeConfiguredConfigureTool.execute(prepare({file:'/tmp/request.json'}),{commit:async()=>{commits++;}} as unknown as ToolExecutionApi,context),/research_coding_workspace_required/);
  assert.equal(commits,0);
});
