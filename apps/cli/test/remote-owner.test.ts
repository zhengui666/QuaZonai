import assert from 'node:assert/strict';
import test from 'node:test';
import { remoteOwnerInspection, inspectedRemoteOwner, inspectRemoteOwner } from '../src/remote-owner.ts';
const expectedIdentity = {nautilusSource:'git',nautilusVersion:'0.69.0',nautilusRevision:'fixed',precision:'high',jobVersion:'3'};
const config = {host:'example.invalid',user:'owner',knownHostsFile:'/pinned/known_hosts',ownerExecutable:'/opt/owner/runner',expectedIdentity};

test('remote inspection is one strict authenticated SSH capabilities call, not an account command', () => {
 const {args,expected} = remoteOwnerInspection(config);
 assert.deepEqual(expected,expectedIdentity);
 assert.deepEqual(args.slice(-3),['--','example.invalid','/opt/owner/runner capabilities']);
 for(const option of ['BatchMode=yes','StrictHostKeyChecking=yes','UpdateHostKeys=no','GlobalKnownHostsFile=none','ForwardAgent=no','ClearAllForwardings=yes','PermitLocalCommand=no','ControlMaster=no','ControlPath=none','ConnectionAttempts=1'])assert.ok(args.includes(option));
 assert.deepEqual(args.slice(0,3),['-F','none','-T']);
 assert.ok(!args.some(x=>x==='BOOT'||x==='START'||x==='account-runtime'));
});

test('rejects arbitrary SSH options, commands, keys, and incomplete identity before execution', () => {
 for(const mutation of [{host:'-oProxyCommand=x'},{user:'a;cmd'},{ownerExecutable:'/bin/owner;id'},{ownerExecutable:'/a/../owner'},
  {knownHostsFile:'/tmp/a\nProxyCommand=x'},{knownHostsFile:'/tmp/%h'},{privateKey:'secret'},{port:0},{port:65536},{expectedIdentity:{...expectedIdentity,nautilusRevision:''}}]) {
  assert.throws(()=>remoteOwnerInspection({...config,...mutation}));
 }
});

test('inspection only reports matching declared capabilities, strips unrelated stdout and never establishes a binding', () => {
 const capabilities = {schemaVersion:1,protocol:'quazonai-native-process-v1',running:false,identity:expectedIdentity,
  account:{compiled:true,registration:true,command:'account-runtime',requiresExplicitStart:true},
  research:{singleStrategyRegistration:true,configuredRunFactory:false},secret:'not returned'};
 const result=inspectedRemoteOwner(capabilities,expectedIdentity);
 assert.equal(result.qualification,'DECLARED_IDENTITY_MATCH_INSPECTION_ONLY');
 assert.equal(result.accountConnectionRequested,false);assert.equal(result.tradingStartRequested,false);assert.equal(result.sourceAttested,false);
 assert.equal(result.secret,undefined);
 for(const mutation of [{running:true},{protocol:'other'},{identity:{...expectedIdentity,precision:'standard'}},
  {account:{...capabilities.account,requiresExplicitStart:false}},{account:{...capabilities.account,compiled:false}}])assert.throws(()=>inspectedRemoteOwner({...capabilities,...mutation},expectedIdentity));
});

test('already cancelled inspection cannot contact the remote host', async () => {
 await assert.rejects(inspectRemoteOwner(config,AbortSignal.abort()),/remote_owner_inspection_cancelled/);
});
