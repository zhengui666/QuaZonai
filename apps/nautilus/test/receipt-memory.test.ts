import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawn } from 'node:child_process';

// The worker, its HTTP client, and replay all share a deliberately small heap.
// A whole parsed result would exceed this limit; consuming output chunks must not.
test('large original receipt survives constrained-heap execution, restart and exact replay', { timeout: 120_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-receipt-memory-'));
  const server = new URL('../src/server.ts', import.meta.url).href;
  try {
    const producer = join(directory, 'producer.mjs');
    await writeFile(producer, `import {appendFileSync} from 'node:fs';
import {once} from 'node:events';
appendFileSync(${JSON.stringify(join(directory, 'calls'))}, 'run\\n');
for await (const _ of process.stdin) {}
const row=JSON.stringify({message:'x'.repeat(2048),clock:'1790000000000000001'});
async function out(s){if(!process.stdout.write(s))await once(process.stdout,'drain');}
await out('{"rows":[');for(let i=0;i<100000;i++)await out((i?',':'')+row);await out(']}');`);
    const binary = join(directory, 'job');
    await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${producer}'\n`, { mode: 0o700 });
    const runner = join(directory, 'runner.mjs');
    await writeFile(runner, `import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {once} from 'node:events';
import {readFile,open} from 'node:fs/promises';
import {createNautilusWorker,executionId} from ${JSON.stringify(server)};
const options={binary:${JSON.stringify(binary)},dataDir:${JSON.stringify(join(directory, 'state'))},catalogRoots:[]};
let worker,url;async function start(){worker=await createNautilusWorker(options);worker.server.listen(0,'127.0.0.1');await once(worker.server,'listening');url='http://127.0.0.1:'+worker.server.address().port;}
async function compare(stream,file){const expected=await open(file,'r');let bytes=0;try{for await(const chunk of stream){const actual=Buffer.from(chunk);const buffer=Buffer.alloc(actual.length);let offset=0;while(offset<buffer.length){const read=await expected.read(buffer,offset,buffer.length-offset,null);assert.ok(read.bytesRead>0,'unexpected receipt end');offset+=read.bytesRead;}assert.ok(actual.equals(buffer),'receipt bytes differ');bytes+=actual.length;}assert.equal(bytes,(await expected.stat()).size);return bytes;}finally{await expected.close();}}
await start();const request={schemaVersion:1,invocationId:randomUUID(),implementationDigest:worker.implementationDigest,command:'review-source',input:{},args:[],files:{}};request.requestId=executionId(request);
let response=await fetch(url+'/v1/execute',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(request)});
while(response.status===202){await response.body.cancel();await new Promise(r=>setTimeout(r,30));response=await fetch(url+'/v1/executions/'+request.requestId);}
assert.equal(response.status,200);
const file=${JSON.stringify(join(directory, 'state', 'executions'))}+'/'+request.requestId+'/receipt.json';const original=await compare(response.body,file);assert.ok(original>200000000);
await worker.close();await start();response=await fetch(url+'/v1/executions/'+request.requestId);assert.equal(response.status,200);assert.equal(await compare(response.body,file),original);
response=await fetch(url+'/v1/execute',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(request)});assert.equal(response.status,200);assert.equal(await compare(response.body,file),original);
assert.equal(await readFile(${JSON.stringify(join(directory, 'calls'))},'utf8'),'run\\n');await worker.close();console.log('bounded receipt bytes='+original);`);
    const child = spawn(process.execPath, ['--max-old-space-size=64', '--experimental-strip-types', runner], { stdio: ['ignore', 'pipe', 'pipe'] });
    let output = ''; child.stdout.on('data', chunk => { output += chunk; }); child.stderr.on('data', chunk => { output += chunk; });
    const code = await new Promise<number | null>((resolve, reject) => { child.once('error', reject); child.once('close', resolve); });
    assert.equal(code, 0, output);
    assert.match(output, /bounded receipt bytes=/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
