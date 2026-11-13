import assert from 'node:assert/strict';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT, withAbortSignal } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { defineExtension, defineTask, defineDocFamily, MemoryStorage } from '@earendil-works/pi-durable';
import type { RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { remoteNativeCapabilities, remoteNativeExecutionStatus, NativeRemoteUncertainty, NativeExecutionTask, reconcileNativeExecution, cancelUnsettledNativeExecutions } from '../../research/src/native-execution.ts';
import { openResearch } from '../../research/src/host.ts';
import { executeNative } from '../../research/src/native-execution.ts';

const digest = `sha256:${'c'.repeat(64)}`;
function signal() { let resolve!: () => void; const promise = new Promise<void>(done => { resolve = done; }); return { promise, resolve }; }
function reply(response: ServerResponse, value: unknown) { response.setHeader('content-type', 'application/json'); response.end(JSON.stringify(value)); }
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
async function explicitHost(directory: string) {
  const models = createModels(); models.setProvider(fauxProvider().provider);
  return openResearch({ database: join(directory, 'memory.sqlite'), models, storage: new MemoryStorage() });
}
async function bounded<T>(promise: Promise<T>, milliseconds = 5_000): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  try { return await Promise.race([promise, new Promise<never>((_resolve, reject) => {
    timer = setTimeout(() => reject(new Error('caller_abort_did_not_settle_remote_helper')), milliseconds);
  })]); } finally { if (timer) clearTimeout(timer); }
}

test('historical capabilities GET honors caller abort; new HTTP execution is rejected without admission', { timeout: 10_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-helper-capabilities-abort-'));
  const host = await explicitHost(directory);
  try {
    {
      const accepted = signal(); const requests: string[] = []; const controller = new AbortController();
      const source = await peer((request, _response) => {
        requests.push(`${request.method} ${request.url}`); accepted.resolve();
        // Connected, but no headers/body: a connect timeout cannot end this wait.
      });
      const context = withAbortSignal(controller.signal, BACKGROUND_CONTEXT);
      const operation = host.withNativeHost(() => remoteNativeCapabilities(source.target, context.abortSignal));
      const settled = operation.then(() => ({ error: null }), error => ({ error }));
      try {
        await bounded(accepted.promise);
        controller.abort(new Error('owner_stop_capability_probe'));
        const outcome = await bounded(settled);
        assert.match(outcome.error?.message ?? '', /owner_stop_capability_probe/);
        assert.deepEqual(requests, ['GET /v1/capabilities'], 'no native operation was admitted and no replacement request is sent');
        assert.deepEqual(source.failures, []);
        const before = (await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),BACKGROUND_CONTEXT)).items;
        await assert.rejects(host.withNativeHost(()=>executeNative(source.target,'review-source',{},[],directory,BACKGROUND_CONTEXT)),/native_http_execution_retired/);
        assert.deepEqual((await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),BACKGROUND_CONTEXT)).items,before);
        assert.deepEqual(requests,['GET /v1/capabilities']);
      } finally { await source.close(); await settled; }
    }
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

// Reconstruct retained identities in the official SQLite store, never issue an old POST.
const Calls = defineDocFamily<any, null>({kind:'qz.native-call',version:1,scope:'task',family:true,initial:()=>({taskId:null}),checkpointWhen:()=>true});
const RetainedOwner = defineTask<null,{phase:'hold'},null>({
  name:'test.retained-native-owner',version:1,initial:()=>({phase:'hold'}),
  phases:{hold:(_task,runtime,context)=>runtime.commit(()=>({status:'sleeping',until:runtime.now()+60000,checkpoint:{phase:'hold'}}),context)},
  async abort(task,runtime,context){await cancelUnsettledNativeExecutions(task.id,runtime,context);await runtime.commit(()=>({status:'terminal',outcome:{status:'aborted'}}),context);},
});

for(const abortOwner of [false,true])test(`retained HTTP task cancellation preserves original UNKNOWN and GET-only identity (owner abort=${abortOwner})`,{timeout:15000},async()=>{
  const directory=await mkdtemp(join(tmpdir(),'qz-retained-http-cancel-'));
  const accepted=signal(),controller=new AbortController(),requestId='a'.repeat(64);
  const requests:string[]=[];let hang=true;
  const source=await peer((request,response)=>{
    requests.push(`${request.method} ${request.url}`);
    assert.equal(request.method,'GET');assert.equal(request.url,`/v1/executions/${requestId}`);accepted.resolve();
    if(!hang)reply(response,{schemaVersion:1,requestId,status:'UNKNOWN',error:'original_outcome_unknown'});
  });
  const models=createModels();models.setProvider(fauxProvider().provider);
  const options={database:join(directory,'research.sqlite'),models,deferResume:true,extensions:[defineExtension({name:'test.retained-http-owner',tasks:[RetainedOwner]})]};
  let host=await openResearch(options);
  try{
    const root=await host.harness.root(BACKGROUND_CONTEXT);
    const ids=await root.commit(async tx=>{
      const owner=await tx.createTask(RetainedOwner,null,{ownership:{kind:'conversation'}});
      const child=await tx.createTask(NativeExecutionTask,{target:source.target,requestId,basis:'retained-original',command:'review-source',request:{request:{schemaVersion:1,requestId,command:'review-source',implementationDigest:digest,args:[],input:{amount:'9007199254740993.0001'}}},ownerTaskId:owner,receiptRequired:true},{ownership:{kind:'task',taskId:owner}});
      (await tx.doc(Calls,owner,'retained-original',null)).taskId=child;
      (await tx.doc(NativeRemoteUncertainty,root.id,String(owner),null)).requests.push({taskId:owner,requestId,target:source.target,status:'UNKNOWN',error:'original_outcome_unknown'});
      return {owner,child};
    },BACKGROUND_CONTEXT);
    const runtime=()=>({conversationId:root.id,signal:new AbortController().signal,getTask:host.harness.getTask.bind(host.harness),snapshot:host.harness.snapshot.bind(host.harness),commit:host.harness.commit.bind(host.harness)} as unknown as Parameters<typeof reconcileNativeExecution>[1]);
    await assert.rejects(reconcileNativeExecution(ids.child,runtime(),ids.child,BACKGROUND_CONTEXT),/native_reconciliation_identity_conflict/);
    assert.deepEqual(requests,[],'wrong owner cannot read or adopt another original task');
    const before=await host.harness.snapshot(NativeRemoteUncertainty,root.id,String(ids.owner),BACKGROUND_CONTEXT);
    const operation=reconcileNativeExecution(ids.owner,runtime(),ids.child,withAbortSignal(controller.signal,BACKGROUND_CONTEXT));
    const settled=operation.then(()=>({error:null}),error=>({error}));
    await bounded(accepted.promise);controller.abort(new Error('owner_stop_original_receipt_read'));
    assert.match((await bounded(settled)).error?.message??'',/owner_stop_original_receipt_read/);
    assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty,root.id,String(ids.owner),BACKGROUND_CONTEXT),before,'aborted read cannot invent settled cancellation');
    assert.deepEqual(requests,[`GET /v1/executions/${requestId}`]);
    hang=false;
    if(abortOwner){host.harness.resume();await bounded(host.harness.abortTask(ids.owner,BACKGROUND_CONTEXT));await bounded(host.harness.waitForTask(ids.owner,BACKGROUND_CONTEXT));assert.equal((await host.harness.getTask(ids.owner,BACKGROUND_CONTEXT))!.state.outcome?.status,'aborted');}
    else await cancelUnsettledNativeExecutions(ids.owner,runtime(),BACKGROUND_CONTEXT);
    const retained=await host.harness.snapshot(NativeRemoteUncertainty,root.id,String(ids.owner),BACKGROUND_CONTEXT);
    assert.equal(retained!.requests.length,1);assert.equal(retained!.requests[0]!.requestId,requestId);assert.equal(retained!.requests[0]!.status,'UNKNOWN');assert.deepEqual(retained!.requests[0]!.target,source.target);
    const nativeTask=await host.harness.getTask(ids.child,BACKGROUND_CONTEXT);
    assert.equal((nativeTask!.input as any).request.request.input.amount,'9007199254740993.0001');
    await host.close();host=await openResearch(options);
    assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty,root.id,String(ids.owner),BACKGROUND_CONTEXT),retained);
    assert.deepEqual((await host.harness.getTask(ids.child,BACKGROUND_CONTEXT))!.input,nativeTask!.input,'reopen retains original command, exact amount, target and IDs');
    if(abortOwner){
      await assert.rejects(reconcileNativeExecution(ids.owner,runtime(),ids.child,BACKGROUND_CONTEXT),/native_reconciliation_identity_conflict/,'terminal owner may not adopt new scientific evidence');
      assert.equal((await remoteNativeExecutionStatus(source.target,requestId)).status,'UNKNOWN');
    } else assert.equal(await reconcileNativeExecution(ids.owner,runtime(),ids.child,BACKGROUND_CONTEXT),false);
    assert.equal((await host.harness.snapshot(NativeRemoteUncertainty,root.id,String(ids.owner),BACKGROUND_CONTEXT))!.requests[0]!.status,'UNKNOWN');
    assert.ok(requests.length>=2);assert.ok(requests.every(row=>row===`GET /v1/executions/${requestId}`),'neither abort nor reopen sends POST, DELETE or replacement identity');
    assert.deepEqual(source.failures,[]);
  }finally{await source.close();await host.close();await rm(directory,{recursive:true,force:true});}
});
