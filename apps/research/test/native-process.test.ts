import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {runNativeProcess,cancelNativeProcess,nativeProcessRunning} from '../src/native-process.ts';
import {JsonReader} from '../src/native-json.ts';

test('native process transport streams output and only cancels its original live handle',async()=>{
 const directory=await mkdtemp(join(tmpdir(),'qz-process-'));
 const script=join(directory,'child.mjs'),marker=join(directory,'starts');
 await writeFile(script,`import{appendFileSync}from'node:fs';appendFileSync(process.argv[2],'started\\n');let input='';for await(const c of process.stdin)input+=c;if(JSON.parse(input).wait){setInterval(()=>{},1000);}else{process.stdout.write(JSON.stringify({original:JSON.parse(input)}));}`);
 try{
  assert.equal(await cancelNativeProcess('lost-original'),'UNKNOWN');
  assert.deepEqual(await runNativeProcess('original',process.execPath,script,[marker],{exact:'0.000000000000000001'},new JsonReader()),{original:{exact:'0.000000000000000001'}});
  assert.equal(await cancelNativeProcess('original'),'UNKNOWN','Completion is not cancellation.');
  const pending=runNativeProcess('waiting',process.execPath,script,[marker],{wait:true},new JsonReader());
  const failure=assert.rejects(pending,/native_process_exit_unknown/);
  assert.equal(nativeProcessRunning('waiting'),true);
  await assert.rejects(runNativeProcess('waiting',process.execPath,script,[marker],{},new JsonReader()),/original_already_running/);
  assert.equal(await cancelNativeProcess('waiting'),'PROCESS_EXITED_BY_SIGNAL');await failure;
  assert.equal(nativeProcessRunning('waiting'),false);
  const starts=(await readFile(marker,'utf8')).trim().split('\n');assert.ok(starts.length<=2,'Repeated request cannot launch a replacement while active.');
 }finally{await rm(directory,{recursive:true,force:true});}
});
test('process output is consumed with backpressure rather than accumulated as one string',async()=>{
 const directory=await mkdtemp(join(tmpdir(),'qz-process-large-'));const script=join(directory,'large.mjs');
 await writeFile(script,`import{once}from'node:events';for await(const _ of process.stdin){};for(let i=0;i<512;i++){if(!process.stdout.write('x'.repeat(65536)))await once(process.stdout,'drain');}`);
 let bytes=0,largest=0;
 try{
  const result=await runNativeProcess('large',process.execPath,script,[],{}, {async write(chunk){bytes+=Buffer.byteLength(chunk);largest=Math.max(largest,chunk.length);await new Promise(r=>setImmediate(r));},finish(){return {bytes};}});
  assert.deepEqual(result,{bytes:32*1024*1024});assert.ok(largest<1024*1024);
 }finally{await rm(directory,{recursive:true,force:true});}
});

test('TERM-ignoring process with inherited descendant pipes has bounded cancellation and no claim of business rollback',async()=>{
 const directory=await mkdtemp(join(tmpdir(),'qz-process-stubborn-'));const script=join(directory,'stubborn.mjs'),ready=join(directory,'ready');
 await writeFile(script,`import{spawn}from'node:child_process';import{writeFileSync}from'node:fs';process.on('SIGTERM',()=>{});spawn(process.execPath,['-e',"process.on('SIGTERM',()=>{});setInterval(()=>{},1000)"],{stdio:'inherit'});writeFileSync(process.argv[2],'ready');setInterval(()=>{},1000);`);
 try{
  const run=runNativeProcess('stubborn',process.execPath,script,[ready],{},new JsonReader());const failed=assert.rejects(run);
  for(let i=0;i<100;i++){try{await readFile(ready);break;}catch{await new Promise(r=>setTimeout(r,10));}}
  assert.equal(await readFile(ready,'utf8'),'ready');const start=Date.now();
  assert.equal(await cancelNativeProcess('stubborn'),'PROCESS_EXITED_BY_SIGNAL');await failed;
  assert.ok(Date.now()-start<4000);assert.equal(await cancelNativeProcess('stubborn'),'UNKNOWN');
 }finally{await rm(directory,{recursive:true,force:true});}
});
