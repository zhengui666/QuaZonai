import { screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { HomePage } from '../pages/HomePage';
import { jsonResponse, renderApp } from './testUtils';

vi.mock('../components/charts/ResearchPulseChart', () => ({
  ResearchPulseChart: () => null,
}));

vi.mock('../lib/useEventStream', () => ({
  useEventStream: () => ({
    connected: true,
    events: [{
      id: 'event-1',
      kind: 'ALPHA_QUALIFIED',
      created_at: '2030-01-01T00:00:00Z',
      aggregate_type: 'PORTFOLIO_MANDATE',
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
      if (url.endsWith('/approvals')) return jsonResponse([{
        id: 'approval-1',
        candidate_id: 'candidate-12345678',
        purpose: 'PAPER',
        state: 'PENDING',
        valid_until: '2030-01-01T00:00:00Z',
        candidate: { mandate_name: 'Core Growth — EUR/USD' },
      }]);
      if ([
        '/research-programs',
        '/alpha-library',
        '/handoffs',
        '/portfolio-programs',
      ].some((path) => url.endsWith(path))) return jsonResponse([]);
      return jsonResponse({}, 404);
    });

    renderApp(<HomePage />, { locale: 'ar' });
    const identity = await screen.findByText((_, element) => (
      element?.tagName === 'BDI' && element.textContent === 'EUR/USD-'
    ));
    expect(identity).toHaveAttribute('dir', 'ltr');
    expect(screen.getByText('تفويض المحفظة')).toHaveAttribute('dir', 'auto');
    expect(screen.getByText('Core Growth — EUR/USD')).toHaveAttribute('dir', 'auto');
  });

  it('keeps data readiness visible when only Codex authentication is missing', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/readiness')) return jsonResponse({ RESEARCH_READY: false, RESEARCH_READY_REASONS: ['CODEX_AUTH_DISCONNECTED'] });
      if (url.endsWith('/system/health')) return jsonResponse({ codex: 'not_configured' });
      if (url.endsWith('/approvals')) return jsonResponse([]);
      if (['/research-programs', '/alpha-library', '/handoffs', '/portfolio-programs'].some((path) => url.endsWith(path))) return jsonResponse([]);
      return jsonResponse({}, 404);
    });

    renderApp(<HomePage />);
    const dataLabel = await screen.findByText('Data readiness');
    expect(dataLabel.parentElement).toHaveTextContent('Ready');
  });
});
