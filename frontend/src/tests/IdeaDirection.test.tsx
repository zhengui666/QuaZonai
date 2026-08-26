import { fireEvent, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IdeaComposerPage } from '../pages/IdeaComposerPage';
import { jsonResponse, renderApp } from './testUtils';

describe('IdeaComposer text direction', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('lets user-authored and API-authored facts determine their own direction', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/ideas/preview')) {
        return jsonResponse({
          charter: {
            original_idea_text: 'Study EUR/USD post-event drift.',
            research_question: 'Does EUR/USD drift persist?',
            prediction_horizon: '1D',
            market_scope: 'US Equities',
          },
          clarification_required: true,
          clarification_questions: [{ key: 'symbol', question: 'Which market symbol: EUR/USD?' }],
          overlap: {
            kind: 'DUPLICATE',
            rationale: 'Existing program covers EUR/USD drift.',
          },
        });
      }
      return jsonResponse({}, 404);
    });

    renderApp(<IdeaComposerPage />, { route: '/ideas' });
    const ideaInput = screen.getByPlaceholderText(/Test whether short-horizon/i);
    expect(ideaInput).toHaveAttribute('dir', 'auto');

    fireEvent.change(ideaInput, { target: { value: 'Study EUR/USD post-event drift over one day.' } });
    fireEvent.click(screen.getByRole('button', { name: /Preview research charter/i }));

    expect(await screen.findByText('Does EUR/USD drift persist?')).toHaveAttribute('dir', 'auto');
    expect(screen.getByText('US Equities')).toHaveAttribute('dir', 'auto');
    expect(screen.getByText('1D')).toHaveAttribute('dir', 'auto');
    expect(screen.getByText('Which market symbol: EUR/USD?')).toHaveAttribute('dir', 'auto');
    expect(screen.getByRole('textbox', { name: 'Which market symbol: EUR/USD?' })).toHaveAttribute('dir', 'auto');

    const overlap = screen.getByText((_, element) => (
      element?.getAttribute('dir') === 'auto' && element.textContent === 'DUPLICATE · Existing program covers EUR/USD drift.'
    ));
    expect(overlap).toHaveAttribute('dir', 'auto');
  });
});
