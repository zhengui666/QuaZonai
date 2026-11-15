import assert from 'node:assert/strict';
import test from 'node:test';
import {remoteAccountTransport,exchangeRemoteAccount,remoteAccountHello} from '../src/remote-account.ts';

test('local and SSH use one account frame and no raw engine operation',()=>{
 const hello=remoteAccountHello();assert.equal(hello.kind,'HELLO');assert.equal(hello.version,1);
 const ssh=remoteAccountTransport({kind:'SSH',host:'example.invalid',user:'owner',knownHostsFile:'/pinned/hosts',relayExecutable:'/opt/qz/quazonai',socketPath:'/private/deploy/account.sock'});
 assert.equal(ssh.kind,'SSH');if(ssh.kind!=='SSH')throw Error();
 assert.equal(ssh.args.at(-1),'/opt/qz/quazonai remote-account relay --socket-path /private/deploy/account.sock');
 assert.ok(ssh.args.includes('StrictHostKeyChecking=yes'));
 assert.deepEqual(remoteAccountTransport({kind:'UNIX',socketPath:'/private/deploy/account.sock'}),{kind:'UNIX',socketPath:'/private/deploy/account.sock'});
});
test('transport rejects arbitrary command, environment, forwarded credentials and relative socket',()=>{
 const valid={kind:'SSH',host:'example.invalid',user:'owner',knownHostsFile:'/pinned/hosts',relayExecutable:'/opt/qz/quazonai',socketPath:'/private/deploy/account.sock'};
 for(const bad of [{...valid,command:'START'},{...valid,env:{HYPERLIQUID_PK:'x'}},{...valid,relayExecutable:'/bin/x;id'},{...valid,socketPath:'/x/../secret'},{kind:'UNIX',socketPath:'relative'}])assert.throws(()=>remoteAccountTransport(bad));
});
test('pre-aborted exchange never invokes SSH or a local socket',async()=>{
 await assert.rejects(exchangeRemoteAccount({kind:'UNIX',socketPath:'/no/account.sock'},remoteAccountHello(),AbortSignal.abort()),/cancelled_before_send/);
});
