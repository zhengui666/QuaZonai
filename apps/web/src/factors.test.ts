import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
import { FactorIdentity } from './factors';
import { currentRoute } from './router';
it('retains account provenance as an inert escaped identifier without retired research links', () => {
  const html = renderToStaticMarkup(createElement(FactorIdentity, {id:'factor-<id>'}));
  expect(html).toBe('<code>factor-&lt;id&gt;</code>');
  expect(html).not.toContain('href');
});
it('does not expose the retired factor product route', () => {
  vi.stubGlobal('location', {hash:'#/factors'});
  try { expect(currentRoute().unknown).toBe(true); } finally { vi.unstubAllGlobals(); }
});
