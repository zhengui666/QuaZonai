import { screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { HandoffFeedbackPage } from '../pages/HandoffFeedbackPage';
import { jsonResponse, renderApp } from './testUtils';

describe('Handoff feedback text direction', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('isolates candidate identities and API tokens in forward-evidence metadata', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/handoffs')) return jsonResponse([{
        id: 'handoff-1',
        state: 'AVAILABLE',
        candidate_id: 'abc12345-long-candidate-id',
        downstream_name: 'مختبر ورقي',
        forward_evidence: {},
      }]);
      return jsonResponse({}, 404);
    });

    renderApp(<HandoffFeedbackPage />, { route: '/handoff', locale: 'ar' });
    const candidateIds = await screen.findAllByText((_, element) => (
      element?.tagName === 'BDI' && element.textContent === 'abc12345'
    ));
    const tableCandidateId = candidateIds.find((element) => element.closest('td'));
    expect(tableCandidateId).toHaveAttribute('dir', 'ltr');
  });
});
