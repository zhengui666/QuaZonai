import { fireEvent, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IdeaComposerPage } from '../pages/IdeaComposerPage';
import { translateKey } from '../i18n';
import { jsonResponse, renderApp } from './testUtils';

describe('IdeaComposer text direction', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('localizes the clarification-round limit', () => {
    renderApp(<IdeaComposerPage />, { route: '/ideas', locale: 'ar' });

    expect(screen.getByText(translateKey('ar', 'idea.oneRound', { count: 1 }))).toBeInTheDocument();
  });

  it('lets user-authored and draft facts determine their own direction', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      if (String(input).endsWith('/idea-drafts')) {
        return jsonResponse({
          id: 'draft-1',
          original_idea_text: 'Study EUR/USD post-event drift.',
          stage: 'CLARIFYING',
          outcome: null,
          next_action: 'ANSWER_CLARIFICATIONS',
          blocking_reasons: ['CLARIFICATION_REQUIRED'],
          revision: 1,
          clarification_questions: [{ key: 'symbol', question: 'Which market symbol: EUR/USD?' }],
          charter: {
            original_idea_text: 'Study EUR/USD post-event drift.',
            research_question: 'Does EUR/USD drift persist?',
            prediction_horizon: '1D',
            market_scope: 'US Equities',
          },
        });
      }
      return jsonResponse({}, 404);
    });

    renderApp(<IdeaComposerPage />, { route: '/ideas' });
    const ideaInput = screen.getByPlaceholderText(/Test whether short-horizon/i);
    expect(ideaInput).toHaveAttribute('dir', 'auto');

    fireEvent.change(ideaInput, { target: { value: 'Study EUR/USD post-event drift over one day.' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create draft' }));

    expect(await screen.findByText('Does EUR/USD drift persist?')).toHaveAttribute('dir', 'auto');
    expect(screen.getByText('US Equities')).toHaveAttribute('dir', 'auto');
    expect(screen.getByText('1D')).toHaveAttribute('dir', 'auto');
    expect(screen.getByText('Which market symbol: EUR/USD?')).toHaveAttribute('dir', 'auto');
    expect(screen.getByRole('textbox', { name: 'Which market symbol: EUR/USD?' })).toHaveAttribute('dir', 'auto');
  });

  it('isolates mixed market scopes and localizes the system-inferred charter sentinel', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      if (String(input).endsWith('/idea-drafts')) {
        return jsonResponse({
          id: 'draft-1',
          original_idea_text: 'Study mixed markets.',
          stage: 'READY',
          outcome: null,
          next_action: 'START_PROGRAM',
          blocking_reasons: [],
          revision: 1,
          clarification_questions: [],
          charter: {
            original_idea_text: 'Study mixed markets.',
            research_question: 'Does mixed-market drift persist?',
            prediction_horizon: 'System inferred',
            market_scope: ['بورصة الرياض', 'EUR/USD', 'System inferred'],
          },
        });
      }
      return jsonResponse({}, 404);
    });

    renderApp(<IdeaComposerPage />, { route: '/ideas', locale: 'ar' });
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'Study mixed market behavior over a meaningful horizon.' } });
    fireEvent.click(screen.getByRole('button', { name: 'إنشاء مسودة' }));

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
  });
});
