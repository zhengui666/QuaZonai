import assert from 'node:assert/strict';import test from 'node:test';
import {createServer} from 'node:http';import {mkdtemp,rm} from 'node:fs/promises';import {join} from 'node:path';import {tmpdir} from 'node:os';
import {BACKGROUND_CONTEXT as ctx} from '@earendil-works/chord/context';import {createModels} from '@earendil-works/pi-ai';
import {Harness,createRegistry,defineExtension} from '@earendil-works/pi-durable';import {openNodeSqliteStorage} from '@earendil-works/pi-durable/storage/sqlite/node';
import {CapitalExitCommand,control} from '../src/capital-exit.ts';import {Application,json} from '../src/facts.ts';
const sleep=(n:number)=>new Promise(r=>setTimeout(r,n));
test('official SQLite Harness reopens original pending/uncertain intents without POST or releasing reservations',{timeout:15000},async()=>{
 const calls:string[]=[];const server=createServer((req,res)=>{calls.push(`${req.method} ${req.url}`);res.statusCode=404;res.end('unknown_original');});await new Promise<void>(r=>server.listen(0,'127.0.0.1',r));const addr=server.address();assert.ok(addr&&typeof addr!=='string');
 const dir=await mkdtemp(join(tmpdir(),'qz-retired-capital-task-'));const path=join(dir,'original.sqlite');
 const registry=createRegistry();registry.install(defineExtension({name:'test.retired-capital',tasks:[CapitalExitCommand]}));
 const open=async()=>Harness.open(await openNodeSqliteStorage(path),{registry,models:createModels()},ctx);
 let h=await open();const connection={id:'original',name:'original',url:`http://127.0.0.1:${addr.port}`,clientId:'client',accountId:'account',mode:'paper' as const};
 try{
  const owner=await h.root(ctx);
  await owner.commit(async tx=>{const app=await tx.doc(Application);app.accountControls={original:{commands:{}}};
   for(const [id,state] of [['pending-original','PENDING'],['unknown-original','UNCERTAIN']] as const){
    const taskId=await tx.createTask(CapitalExitCommand,{key:'original',requestId:id,connection,reconcile:false,binary:'/unused',dataDir:dir},{ownership:{kind:'conversation'}});
    (app.accountControls.original as any).commands[id]={original:{action:'START',requestId:id},body:{preview_id:'old-preview'},path:'capital-exits',reserved:'123',state,result:null,error:null,taskId};
   }
  },ctx);
  await h.close(ctx);h=await open();h.resume();
  const deadline=Date.now()+5000;while(!calls.length&&Date.now()<deadline)await sleep(10);
  assert.ok(calls.includes('GET /capital-exits/commands/unknown-original'));assert.ok(calls.every(c=>c.startsWith('GET ')));
  const retained=control((await h.snapshot(Application,ctx))?.accountControls?.original);
  assert.equal(retained.commands['pending-original']?.state,'PENDING');assert.equal(retained.commands['unknown-original']?.state,'UNCERTAIN');
  for(const row of Object.values(retained.commands))assert.equal(row.reserved,'123');
  const task=await h.getTask(retained.commands['pending-original']!.taskId as never,ctx);assert.equal(task?.state.status,'terminal');
  await h.close(ctx);h=await open();
  assert.equal(control((await h.snapshot(Application,ctx))?.accountControls?.original).commands['unknown-original']?.state,'UNCERTAIN');
 }finally{await h.close(ctx);server.closeAllConnections();await new Promise<void>(r=>server.close(()=>r()));await rm(dir,{recursive:true,force:true});}
});
