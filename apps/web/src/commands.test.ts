import { afterEach, expect, test, vi } from 'vitest';
import { decimalUnits, readDraft, removeDraft, saveDraft } from './commands';
afterEach(() => vi.unstubAllGlobals());
test('allocation uses exact 18-digit arithmetic without normalizing original strings', () => {
  expect(decimalUnits('0.500000000000000001')! + decimalUnits('0.499999999999999999')!).toBe(decimalUnits('1'));
  expect(decimalUnits('1000000000000000000000000.000000000000000001')).toBe(1000000000000000000000000000000000000000001n);
  expect(decimalUnits('0')).toBe(0n); expect(decimalUnits('-0.25')).toBe(-250000000000000000n);
  for (const value of ['', '1e3', 'Infinity', 'NaN', '.5', '1.', '0.1234567890123456789']) expect(decimalUnits(value)).toBeUndefined();
});
test('nonsecret drafts survive route remounts in memory when storage is unavailable', () => {
  vi.stubGlobal('sessionStorage', { getItem() { throw new Error('unavailable'); }, setItem() { throw new Error('unavailable'); }, removeItem() { throw new Error('unavailable'); } });
  const original = { goal: 'Original question', maxTrials: '0' }; saveDraft('memory-test', original); expect(readDraft('memory-test')).toEqual(original); removeDraft('memory-test'); expect(readDraft('memory-test')).toBeUndefined();
});
test('a malformed serialized draft does not break the authoring page', () => {
  vi.stubGlobal('sessionStorage', { getItem() { return '{invalid'; } }); expect(readDraft('malformed-test')).toBeUndefined();
});
