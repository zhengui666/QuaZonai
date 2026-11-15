import {describe,it,expect} from 'vitest';
import {remoteAccountBindInput} from './remote-account-configuration';
const binding={deploymentId:'original-deploy',sourceId:'original-source',clientId:'client',accountId:'owner',runnerIdentity:'fixed',nativeSessionId:'original-session'};
const probe={probeId:'probe-original',targetId:'approved-target',binding,configuration:{mode:'paper'}};
describe('explicit remote binding',()=>{
 it('copies the exact probed binding without a start, rewritten session or arbitrary SSH destination',()=>{
  expect(remoteAccountBindInput('remote-name','approved-target',probe)).toEqual({id:'remote-name',targetId:'approved-target',probeId:'probe-original',binding});
  const saved=remoteAccountBindInput('remote-name','approved-target',probe);saved.binding.nativeSessionId='changed';expect(probe.binding.nativeSessionId).toBe('original-session');
 });
 it('explicit successor retains the previous binding identity and requires a new name',()=>{
  expect(remoteAccountBindInput('remote-new','approved-target',probe,'remote-old').supersedes).toBe('remote-old');
  expect(()=>remoteAccountBindInput('remote-old','approved-target',probe,'remote-old')).toThrow();
 });
 it('refuses an unprobed, changed target or invalid local identity',()=>{
  expect(()=>remoteAccountBindInput('remote-name','other-target',probe)).toThrow();
  expect(()=>remoteAccountBindInput('remote-name','approved-target',undefined)).toThrow();
  expect(()=>remoteAccountBindInput('../account','approved-target',probe)).toThrow();
 });
});
