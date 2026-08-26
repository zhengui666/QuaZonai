import { screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PortfolioLabPage } from '../pages/PortfolioLabPage';
import { jsonResponse, renderApp } from './testUtils';

describe('Portfolio mandate text direction', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('lets API-authored mandate names and objectives establish their own direction', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/portfolio-mandates')) return jsonResponse([{
        id: 'mandate-1',
        name: 'Core Growth — EUR/USD',
        enabled: true,
        latest_version_id: 'version-12345678',
        spec_json: { objective: 'Target 8% (SPY/QQQ)' },
      }]);
      if (url.endsWith('/portfolio-programs')) return jsonResponse([]);
      return jsonResponse({}, 404);
    });

    renderApp(<PortfolioLabPage />, { route: '/portfolio', locale: 'ar' });

    expect(await screen.findByText('Core Growth — EUR/USD')).toHaveAttribute('dir', 'auto');
    expect(screen.getByText('Target 8% (SPY/QQQ)')).toHaveAttribute('dir', 'auto');
  });
});
