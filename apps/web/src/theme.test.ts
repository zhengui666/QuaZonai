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

// Workbench small text uses the same muted token on all three surfaces.
// Keep the light and dark pairs above WCAG AA, including tinted note panels.
it('keeps workbench secondary text readable on every surface', async () => {
  const { readFile } = await import('node:fs/promises');
  const css = await readFile(new URL('./styles.css', import.meta.url), 'utf8');
  const luminance = (hex: string) => {
    const rgb = [1, 3, 5].map(offset => parseInt(hex.slice(offset, offset + 2), 16) / 255)
      .map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
    return rgb[0]! * 0.2126 + rgb[1]! * 0.7152 + rgb[2]! * 0.0722;
  };
  const themes = [...css.matchAll(/:root(?:\[data-theme="dark"\])?\s*\{([^}]*--work-muted:[^}]*)\}/g)];
  expect(themes).toHaveLength(2);
  for (const match of themes) {
    const variables = Object.fromEntries([...match[1]!.matchAll(/(--[\w-]+):\s*(#[0-9a-f]{3,6})/g)]
      .map(variable => [variable[1]!, variable[2]!.length === 4 ? `#${[...variable[2]!.slice(1)].map(value => value.repeat(2)).join('')}` : variable[2]!]));
    const foreground = luminance(variables['--work-muted']!);
    for (const surface of ['--page-bg', '--work-surface', '--work-soft']) {
      const background = luminance(variables[surface]!);
      expect((Math.max(foreground, background) + 0.05) / (Math.min(foreground, background) + 0.05)).toBeGreaterThanOrEqual(4.5);
    }
  }
});
