import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { PortfolioCandidatePage } from '../pages/PortfolioCandidatePage';
import { renderApp } from './testUtils';

vi.mock('../components/graphs/RedundancyGraph', () => ({
  RedundancyGraph: () => null,
}));

vi.mock('../lib/api/hooks', () => ({
  useCandidate: () => ({
    isLoading: false,
    error: null,
    data: {
      id: 'candidate-precision',
      portfolio_program_id: 'program-1',
      state: 'QUALIFIED',
      mandate_version_id: 'mandate/v1',
      policy_version: 'policy/v1',
      risk_model_version: 'risk/v2',
      cost_model_version: 'cost/v3',
      capacity_model_version: 'capacity/v4',
      constraint_set_version: 'constraint-set-3',
      rebalance_policy_version: 'rebalance/v5',
      evaluation_episode_id: 'episode/v6',
      metrics: { search_adjusted_quality: 0.0004 },
      members: [],
    },
  }),
}));

describe('Portfolio candidate presentation', () => {
  it('preserves precise quality values and keeps frozen identifiers LTR in Arabic', () => {
    renderApp(<PortfolioCandidatePage />, { locale: 'ar' });
    const precise = new Intl.NumberFormat('ar', { maximumSignificantDigits: 15 }).format(0.0004);
    expect(screen.getByText(precise)).toBeInTheDocument();

    expect(screen.getByRole('heading', { level: 1 }).textContent).toContain('\u2066candidate-\u2069');

    for (const identifier of ['mandate/', 'policy/v1', 'risk/v2', 'cost/v3', 'capacity/v4', 'constraint-set-3', 'rebalance/v5', 'episode/v6']) {
      expect(screen.getByText(identifier)).toHaveAttribute('dir', 'ltr');
    }
  });
});
