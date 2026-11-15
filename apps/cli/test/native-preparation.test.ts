import assert from 'node:assert/strict';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { run } from '../src/commands.ts';
import { saveProfile } from '../src/profile.ts';

test('explicit preparation CLI preserves original bodies without global selection or research execution', async () => {
  const calls: {method:string|undefined;url:string|undefined;body:string}[]=[];
  const server=createServer(async(request,response)=>{let body='';for await(const chunk of request)body+=chunk;calls.push({method:request.method,url:request.url,body});response.writeHead(200,{'content-type':'application/json'});response.end('{"accepted":true}');});
  server.listen(0,'127.0.0.1');await once(server,'listening');
  const directory=await mkdtemp(join(tmpdir(),'qz-cli-native-prepare-'));const previous=process.env.XDG_CONFIG_HOME;process.env.XDG_CONFIG_HOME=directory;
  try {
    const address=server.address() as {port:number};await saveProfile({schema_version:1,origin:`http://127.0.0.1:${address.port}`,token:'qzc.synthetic-native-preparation',development_http:true,ca_certificate:null});
    const file=join(directory,'request.json');const body='{"requestId":"original","expectedRevision":3,"path":"/controlled/new.json"}\n';await writeFile(file,body);
    for(const args of [['datasets','registry'],['native-runner','status'],['native-runner','build-status','original:1'],['datasets','register','--file',file],['native-runner','import','--file',file]])assert.equal(await run(args,()=>{}),0);
    assert.deepEqual(calls.map(c=>[c.method,c.url]),[['GET','/api/v3/data-sources'],['GET','/api/v3/settings/instances'],['GET','/api/v3/settings/instances/build/original%3A1'],['POST','/api/v3/data-sources/qz.data.registered/datasets'],['POST','/api/v3/settings/instances/import']]);
    assert.equal(calls[3].body,body);assert.equal(calls[4].body,body);
    await assert.rejects(run(['native-runner','select'],()=>{}),/unknown_native_runner_command/);
    await assert.rejects(run(['datasets','register','--file',file,'--request-id','other'],()=>{}),/request_id_conflict/);assert.equal(calls.length,5);
  } finally {if(previous===undefined)delete process.env.XDG_CONFIG_HOME;else process.env.XDG_CONFIG_HOME=previous;server.close();await rm(directory,{recursive:true,force:true});}
});
