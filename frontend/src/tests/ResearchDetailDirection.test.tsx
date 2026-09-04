import { screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Route, Routes } from 'react-router-dom';
import { ResearchDetailPage } from '../pages/ResearchDetailPage';
import { jsonResponse, renderApp } from './testUtils';

describe('ResearchDetailPage charter direction', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('isolates mixed market scopes and localizes the system-inferred charter sentinel', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/research-programs/p-1/mission-graph')) return jsonResponse({ nodes: [] });
      if (url.endsWith('/research-programs/p-1')) {
        return jsonResponse({
          id: 'p-1',
          state: 'ACTIVE',
          created_at: '2030-01-01T00:00:00Z',
          charter: {
            original_idea_text: 'Study mixed markets.',
            research_question: 'Does mixed-market drift persist?',
            prediction_horizon: 'System inferred',
            market_scope: ['بورصة الرياض', 'EUR/USD', 'System inferred'],
            explicit_exclusions: ['استبعاد غير سائل', 'GBP/JPY'],
          },
        });
      }
      return jsonResponse({}, 404);
    });

    renderApp(
      <Routes><Route path="/research/:id" element={<ResearchDetailPage />} /></Routes>,
      { route: '/research/p-1', locale: 'ar' },
    );

    const arabicScope = await screen.findByText((_, element) => element?.tagName === 'BDI' && element.textContent === 'بورصة الرياض');
    const eurUsdScope = screen.getByText((_, element) => element?.tagName === 'BDI' && element.textContent === 'EUR/USD');
    const inferredScope = screen.getByText((_, element) => element?.tagName === 'BDI' && element.textContent === 'استنتجه النظام');
    expect(arabicScope).toHaveAttribute('dir', 'auto');
    expect(eurUsdScope).toHaveAttribute('dir', 'auto');
    expect(inferredScope).toHaveAttribute('dir', 'auto');
    const scopeContainer = arabicScope.parentElement?.parentElement;
    expect(scopeContainer).not.toHaveAttribute('dir');
    expect(scopeContainer).toHaveTextContent('بورصة الرياض, EUR/USD, استنتجه النظام');
    expect(screen.getAllByText('استنتجه النظام')).toHaveLength(2);
    expect(screen.queryByText('System inferred')).not.toBeInTheDocument();

    const arabicExclusion = screen.getByText((_, element) => element?.tagName === 'BDI' && element.textContent === 'استبعاد غير سائل');
    const gbpJpyExclusion = screen.getByText((_, element) => element?.tagName === 'BDI' && element.textContent === 'GBP/JPY');
    expect(arabicExclusion).toHaveAttribute('dir', 'auto');
    expect(gbpJpyExclusion).toHaveAttribute('dir', 'auto');
    const exclusionContainer = arabicExclusion.parentElement?.parentElement;
    expect(exclusionContainer).not.toHaveAttribute('dir');
    expect(exclusionContainer).toHaveTextContent('استبعاد غير سائل, GBP/JPY');

    expect(fetchMock).toHaveBeenCalledWith('/api/v1/research-programs/p-1/mission-graph', expect.anything());
  });
});
