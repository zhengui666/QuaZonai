import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createServer} from 'node:http';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider} from '@earendil-works/pi-ai';
import {openResearch} from '../src/host.ts';
import {executeNativeValue,remoteNativeExecutionStatus} from '../src/native-execution.ts';
import {downstreamRequest} from '../../service/src/downstream.ts';

test('retired HTTP refuses new scientific/account writes before contact but retains original receipt reads',async()=>{
 const directory=await mkdtemp(join(tmpdir(),'qz-retired-http-'));const calls:string[]=[];
 const server=createServer((req,res)=>{calls.push(`${req.method} ${req.url}`);res.setHeader('content-type','application/json');res.end(JSON.stringify({schemaVersion:1,requestId:'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',status:'UNKNOWN',error:'retained_unknown'}));});
 await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));const address=server.address();assert.ok(address&&typeof address!=='string');
 const target={id:'historical',url:`http://127.0.0.1:${address.port}`};const models=createModels();models.setProvider(fauxProvider().provider);const host=await openResearch({database:join(directory,'research.sqlite'),models});
 try{
  await assert.rejects(host.withNativeHost(()=>executeNativeValue(target,'native-configured-backtest',{},[],directory,context)),/native_http_execution_retired/);
  const connection={...target,name:'old',mode:'paper' as const,clientId:'original-client',accountId:'original-account'};
  for(const action of ['START','RECOVER','STOP'])await assert.rejects(downstreamRequest(connection,'strategy/commands','POST',{action,requestId:'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'}),/native_http_control_retired/);
  assert.deepEqual(calls,[]);assert.equal((await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items.length,0);
  assert.equal((await remoteNativeExecutionStatus(target,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa')).status,'UNKNOWN');
  await downstreamRequest(connection,'strategy/commands/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','GET');
  assert.deepEqual(calls,['GET /v1/executions/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','GET /strategy/commands/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa']);
 }finally{await host.close();server.closeAllConnections();await new Promise<void>(resolve=>server.close(()=>resolve()));await rm(directory,{recursive:true,force:true});}
});
