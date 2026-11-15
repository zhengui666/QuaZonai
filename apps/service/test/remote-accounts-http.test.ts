import {test} from 'node:test';
import assert from 'node:assert/strict';
import {setup} from './support.ts';

// Real service/auth hooks, but no remote probe, SSH, owner launch or trading.
const prefix='/api/v3';
const origin='http://127.0.0.1:8080';
const trusted={id:'fixture',name:'Fixture only',transport:{kind:'SSH',host:'fixture.invalid',user:'fixture',knownHostsFile:'/fixture/known_hosts',relayExecutable:'/fixture/relay',socketPath:'/fixture/account.sock'}};

test('disabled remote transport preserves authenticated HTTP behavior and never probes unknown targets',async()=>{
 const state=await setup();
 try{
  const denied=await state.service.app.inject({url:`${prefix}/accounts/remote/targets`});assert.equal(denied.statusCode,401,denied.body);
  const listed=await state.service.app.inject({url:`${prefix}/accounts/remote/targets`,headers:state.headers});assert.equal(listed.statusCode,200,listed.body);assert.deepEqual(listed.json(),[]);
  const probe=await state.service.app.inject({method:'POST',url:`${prefix}/accounts/remote/probe`,headers:state.headers,payload:{targetId:'not-configured'}});
  assert.equal(probe.statusCode,409,probe.body);assert.equal(probe.json().error,'remote_account_target_not_allowed');
 }finally{await state.cleanup();}
});

test('active remote feature retains auth and guards actual HTTP routes including observations',async()=>{
 const state=await setup();
 try{
  state.options.config.remoteAccounts={targets:[trusted],origins:[origin]};await state.restart();
  const headers={...state.headers,host:'127.0.0.1:8080',origin};
  const targets=`${prefix}/accounts/remote/targets`;
  const denied=await state.service.app.inject({url:targets,remoteAddress:'127.0.0.1',headers:{host:headers.host,origin}});assert.equal(denied.statusCode,401,denied.body);
  for(const extra of [{origin:'http://evil.invalid'},{origin:'null'},{host:'evil.invalid'},{origin:origin+'.evil.invalid'}]){
   const response=await state.service.app.inject({url:targets,remoteAddress:'127.0.0.1',headers:{...headers,...extra}});assert.equal(response.statusCode,403,response.body);assert.equal(response.json().error,'remote_account_local_origin_required');
  }
  const peer=await state.service.app.inject({url:targets,remoteAddress:'192.0.2.1',headers});assert.equal(peer.statusCode,403,peer.body);
  const fallback=await state.service.app.inject({url:targets,remoteAddress:'127.0.0.1',headers:{...state.headers,host:headers.host,'sec-fetch-site':'same-origin',referer:origin+'/accounts'}});
  assert.equal(fallback.statusCode,200,fallback.body);assert.deepEqual(fallback.json(),[{id:'fixture',name:'Fixture only'}]);
  const missingGet=await state.service.app.inject({url:targets,remoteAddress:'127.0.0.1',headers:{...state.headers,host:headers.host}});assert.equal(missingGet.statusCode,403,missingGet.body);
  for(const url of [`${prefix}/accounts/remote/probe`,`${prefix}/accounts/remote/bind`,`${prefix}/observations/nonexistent/strategy/commands`,`${prefix}/observations/nonexistent/capital-exits/commands`]){
   for(const originHeaders of [{},{origin:'http://evil.invalid'}]){
    const response=await state.service.app.inject({method:'POST',url,remoteAddress:'127.0.0.1',headers:{...state.headers,host:headers.host,...originHeaders},payload:{}});
    assert.equal(response.statusCode,403,response.body);assert.equal(response.json().error,'remote_account_local_origin_required');
   }
  }
  // Matches the clients' normalized no-argument mutation: {} plus JSON. It
  // reaches deterministic target validation rather than the Origin/JSON guard.
  const empty=await state.service.app.inject({method:'POST',url:`${prefix}/accounts/remote/probe`,remoteAddress:'127.0.0.1',headers:{...headers,'content-type':'application/json'},payload:'{}'});
  assert.notEqual(empty.statusCode,403,empty.body);assert.notEqual(empty.statusCode,415,empty.body);assert.equal(empty.json().error,'targetId_required');
  const nonJson=await state.service.app.inject({method:'POST',url:`${prefix}/accounts/remote/probe`,remoteAddress:'127.0.0.1',headers:{...headers,'content-type':'text/plain'},payload:'{}'});assert.equal(nonJson.statusCode,415,nonJson.body);
  // Forwarded headers cannot promote an external socket peer to local owner.
  const forwarded=await state.service.app.inject({url:targets,remoteAddress:'192.0.2.2',headers:{...headers,'x-forwarded-for':'127.0.0.1'}});assert.equal(forwarded.statusCode,403,forwarded.body);
 }finally{await state.cleanup();}
});
