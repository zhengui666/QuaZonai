import {describe,it,expect} from 'vitest';
import {retryUncertainEnvironmentSave} from './instance-settings';
describe('environment configuration retry semantics',()=>{
 it('unlocks definitive invalid/stale inputs but retains an uncertain request',()=>{
  expect(retryUncertainEnvironmentSave(400)).toBe(false);
  expect(retryUncertainEnvironmentSave(409)).toBe(false);
  expect(retryUncertainEnvironmentSave(422)).toBe(false);
  expect(retryUncertainEnvironmentSave(503)).toBe(true);
  expect(retryUncertainEnvironmentSave()).toBe(true);
 });
});
