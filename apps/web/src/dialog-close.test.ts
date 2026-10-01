import { describe, expect, it } from 'vitest';
import { closeDecision } from './dialog-close';

describe('mutation editor dismissal', () => {
  it('never dismisses a pending write', () => {
    for (const dirty of [false, true]) for (const failed of [false, true]) for (const retained of [false, true]) {
      expect(closeDecision(true, dirty, failed, retained)).toBe('wait');
    }
  });
  it('confirms dirty inputs and failed attempts, including unchanged forms', () => {
    expect(closeDecision(false, true, false)).toBe('confirm');
    expect(closeDecision(false, false, true)).toBe('confirm');
    expect(closeDecision(false, true, true)).toBe('confirm');
  });
  it('closes a pristine unsubmitted form without an unnecessary warning', () => {
    expect(closeDecision(false, false, false)).toBe('close');
  });
  it('detaches a failed request only when its exact retry is retained outside the editor', () => {
    expect(closeDecision(false, true, true, true)).toBe('close');
    expect(closeDecision(false, true, true, false)).toBe('confirm');
  });
});
