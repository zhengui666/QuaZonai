import { describe, expect, it } from 'vitest';
import { resolveTheme } from './theme';

describe('theme preference', () => {
  it.each([true, false])('retains an explicit preference (system dark: %s)', dark => {
    expect(resolveTheme('light', dark)).toBe('light');
    expect(resolveTheme('dark', dark)).toBe('dark');
  });
  it.each([null, undefined, '', 'auto', 'DARK', {}, 1])('falls back to the system for %j', preference => {
    expect(resolveTheme(preference, true)).toBe('dark');
    expect(resolveTheme(preference, false)).toBe('light');
  });
});
