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

    const kind = screen.getByText((_, element) => (
      element?.tagName === 'BDI' && element.textContent === 'Duplicate'
    ));
    const rationale = screen.getByText((_, element) => (
      element?.tagName === 'BDI' && element.textContent === 'Existing program covers EUR/USD drift.'
    ));
    expect(kind).toHaveAttribute('dir', 'auto');
    expect(rationale).toHaveAttribute('dir', 'auto');
  });
  it('isolates a localized overlap kind from an LTR API rationale in Arabic', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      if (String(input).endsWith('/ideas/preview')) {
        return jsonResponse({
          charter: {
            original_idea_text: 'Study EUR/USD post-event drift.',
            research_question: 'Does EUR/USD drift persist?',
            prediction_horizon: '1D',
            market_scope: 'US Equities',
          },
          clarification_required: false,
          overlap: { kind: 'DUPLICATE', rationale: 'Compare EUR/USD' },
        });
      }
      return jsonResponse({}, 404);
    });

    renderApp(<IdeaComposerPage />, { route: '/ideas', locale: 'ar' });
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'Study EUR/USD post-event drift over one day.' } });
    fireEvent.click(screen.getByRole('button', { name: 'معاينة ميثاق البحث' }));

    const kind = await screen.findByText((_, element) => (
      element?.tagName === 'BDI' && element.textContent === 'مكرر'
    ));
    const rationale = screen.getByText((_, element) => (
      element?.tagName === 'BDI' && element.textContent === 'Compare EUR/USD'
    ));
    expect(kind).toHaveAttribute('dir', 'auto');
    expect(rationale).toHaveAttribute('dir', 'auto');
    expect(kind.parentElement).toBe(rationale.parentElement);
    expect(kind.parentElement).toHaveTextContent('مكرر · Compare EUR/USD');
  });
});
