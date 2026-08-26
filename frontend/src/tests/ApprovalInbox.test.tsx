import { fireEvent, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApprovalInboxPage, formatDeployableCapital } from '../pages/ApprovalInboxPage';
import { jsonResponse, renderApp } from './testUtils';

describe('ApprovalInbox', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('shows exactly the immutable recommendation and can approve to a compatible downstream', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      if (url.endsWith('/approvals')) return jsonResponse([{
        id: 'a-1',
        candidate_id: 'candidate-12345678',
        purpose: 'PAPER',
        state: 'PENDING',
        recommendation_rationale: 'Material improvement with independent evidence.',
        valid_until: '2030-01-01T00:00:00Z',
        candidate: { id: 'candidate-12345678', portfolio_program_id: 'pp-1', state: 'READY', mandate_name: 'Core Growth' },
        evidence_summary: { search_adjusted_quality: .71 },
        capital_context: { base_currency: 'USD', deployable_capital: 100000 },
      }]);
      if (url.endsWith('/downstream-systems')) return jsonResponse([
        { id: 'd-1', name: 'Paper Lab', environment_type: 'PAPER', enabled: true },
        { id: 'd-2', name: 'Live Primary', environment_type: 'LIVE', enabled: true },
      ]);
      if (url.endsWith('/approvals/a-1/approve') && init?.method === 'POST') return jsonResponse({ state: 'APPROVED' });
      return jsonResponse({}, 404);
    });

    renderApp(<ApprovalInboxPage />, { route: '/approvals' });

    expect(await screen.findByText('Material improvement with independent evidence.')).toBeInTheDocument();
    expect(screen.queryByText('Live Primary')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Approve' }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith('/api/v1/approvals/a-1/approve', expect.objectContaining({ method: 'POST' }));
    });
  });

  it('formats deployable capital with the selected locale', () => {
    expect(formatDeployableCapital('ar', 100000)).toBe(new Intl.NumberFormat('ar').format(100000));
    expect(formatDeployableCapital('es', '100000')).toBe(new Intl.NumberFormat('es').format(100000));
  });
});
