import { afterEach, expect, it, vi } from 'vitest';
import { currentRoute } from './router';
afterEach(() => vi.unstubAllGlobals());
it.each(['#/pm', '#/pm/new', '#/pm/original-id'])('does not expose retired investment decision workspace %s', hash => {
  vi.stubGlobal('location', { hash });
  expect(currentRoute().unknown).toBe(true);
});
it.each(['research', 'observations', 'portfolios', 'settings'])('retains %s workspace', page => {
  vi.stubGlobal('location', { hash: `#/${page}` });
  expect(currentRoute()).toMatchObject({ page, unknown: false });
});

it.each(['#/parameter-scans', '#/parameter-scans/legacy%2Fid?tab=inputs', '#/parameter-scans/new'])('keeps a distinct route for the retired parameter scan notice: %s', hash => {
  vi.stubGlobal('location', { hash });
  expect(currentRoute()).toMatchObject({ page: 'parameter-scans', unknown: false });
});
