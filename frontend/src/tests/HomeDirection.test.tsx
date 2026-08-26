import { screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { HomePage } from '../pages/HomePage';
import { jsonResponse, renderApp } from './testUtils';

vi.mock('../lib/useEventStream', () => ({
  useEventStream: () => ({
    connected: true,
    events: [{
      id: 'event-1',
      kind: 'ALPHA_QUALIFIED',
      created_at: '2030-01-01T00:00:00Z',
      aggregate_type: 'Portfolio Program',
      aggregate_id: 'EUR/USD-12345678',
    }],
  }),
}));

describe('HomePage text direction', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('isolates API event identities at the rendered boundary', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/readiness') || url.endsWith('/system/health')) return jsonResponse({});
      if ([
        '/research-programs',
        '/approvals',
        '/alpha-library',
        '/handoffs',
        '/portfolio-programs',
      ].some((path) => url.endsWith(path))) return jsonResponse([]);
      return jsonResponse({}, 404);
    });

    renderApp(<HomePage />);
    const identity = await screen.findByText((_, element) => (
      element?.tagName === 'BDI' && element.textContent === 'Portfolio Program EUR/USD-'
    ));
    expect(identity).toHaveAttribute('dir', 'auto');
  });
});
