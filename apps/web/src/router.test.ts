import { afterEach, expect, it, vi } from 'vitest';
import { currentRoute } from './router';
afterEach(() => vi.unstubAllGlobals());
it.each(['#/pm', '#/pm/new', '#/pm/original-id'])('does not expose retired investment decision workspace %s', hash => {
  vi.stubGlobal('location', { hash });
  expect(currentRoute().unknown).toBe(true);
});
it.each(['research', 'observations', 'portfolios', 'parameter-scans', 'settings'])('retains %s workspace', page => {
  vi.stubGlobal('location', { hash: `#/${page}` });
  expect(currentRoute()).toMatchObject({ page, unknown: false });
});
