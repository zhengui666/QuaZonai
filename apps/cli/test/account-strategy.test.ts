import assert from 'node:assert/strict';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { run } from '../src/commands.ts';
import { saveProfile } from '../src/profile.ts';

test('native strategy CLI uses explicit command identity and read-only receipt reconciliation', async () => {
  const requests: { method: string; path: string; body: string }[] = [];
  const server = createServer(async (request, response) => {
    let body=''; for await (const chunk of request) body+=chunk;
    requests.push({method:request.method!,path:request.url!,body});
    if (body.includes('lost-start')) {request.socket.destroy();return;}
    response.writeHead(200,{'content-type':'application/json'});response.end(JSON.stringify({status:'UNCERTAIN',receipt:{persisted:false,result:{state:'STOPPED'}}}));
  });
  server.listen(0,'127.0.0.1');await once(server,'listening');
  const directory=await mkdtemp(join(tmpdir(),'qz-native-strategy-cli-'));
  const previous=process.env.XDG_CONFIG_HOME;process.env.XDG_CONFIG_HOME=directory;
  try {
    await saveProfile({schema_version:1,origin:`http://127.0.0.1:${(server.address() as {port:number}).port}`,token:'qzc.synthetic',development_http:true,ca_certificate:null});
    const output:unknown[]=[];
    await run(['account-strategy','status','account/a'],v=>output.push(v));
    await run(['account-strategy','stop','account/a','--request-id','stop-original'],v=>output.push(v));
    await assert.rejects(run(['account-strategy','start','account/a','--request-id','lost-start'],()=>{}),/write_outcome_unknown_retain_original_request/);
    await run(['account-strategy','receipt','account/a','lost-start'],()=>{});
    assert.deepEqual(requests.map(r=>[r.method,r.path]),[
      ['GET','/api/v3/observations/account%2Fa/strategy'],['POST','/api/v3/observations/account%2Fa/strategy/commands'],
      ['POST','/api/v3/observations/account%2Fa/strategy/commands'],['GET','/api/v3/observations/account%2Fa/strategy/commands/lost-start'],
    ]);
    assert.deepEqual(JSON.parse(requests[1]!.body),{requestId:'stop-original',action:'STOP'});
    assert.deepEqual(JSON.parse(requests[2]!.body),{requestId:'lost-start',action:'START'});
    assert.equal((output[1] as {status:string}).status,'UNCERTAIN','Never relabel a raw stopped result as a durable receipt.');
    await assert.rejects(run(['account-strategy','start','account/a'],()=>{}),/original_request_id_required/);
    await assert.rejects(run(['observation','automatic','account/a'],()=>{}),/unknown_observation_command/);
    const file=join(directory,'old.json');await writeFile(file,JSON.stringify({requestId:'old',portfolioId:'p',mode:'paper'}));
    await assert.rejects(run(['observation','start','--file',file],()=>{}),/native_strategy_only_account_binding_required/);
    assert.equal(requests.length,4);
  } finally {if(previous===undefined)delete process.env.XDG_CONFIG_HOME;else process.env.XDG_CONFIG_HOME=previous;server.close();await once(server,'close');await rm(directory,{recursive:true,force:true});}
});
