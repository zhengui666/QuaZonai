import { fireEvent, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IdeaComposerPage } from '../pages/IdeaComposerPage';
import { jsonResponse, renderApp } from './testUtils';

const idea = 'Study post earnings drift in liquid US equities for one day.';

describe('IdeaComposer', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('creates a draft, submits answers, then starts research', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/idea-drafts')) {
        return jsonResponse({
          id: 'draft-1',
          original_idea_text: idea,
          stage: 'CLARIFYING',
          outcome: null,
          next_action: 'ANSWER_CLARIFICATIONS',
          blocking_reasons: ['CLARIFICATION_REQUIRED'],
          revision: 1,
          clarification_questions: [{ key: 'horizon', question: 'What is the holding horizon?' }],
          charter: null,
        }, 201);
      }
      if (url.endsWith('/idea-drafts/draft-1/answers')) {
        return jsonResponse({
          id: 'draft-1',
          original_idea_text: idea,
          stage: 'READY',
          outcome: null,
          next_action: 'START_PROGRAM',
          blocking_reasons: [],
          revision: 2,
          clarification_questions: [{ key: 'horizon', question: 'What is the holding horizon?' }],
          charter: {
            original_idea_text: idea,
            research_question: 'Does drift persist?',
            prediction_horizon: '1D',
            market_scope: 'US Equities',
          },
        });
      }
      if (url.endsWith('/universes')) return jsonResponse({ items: [{ id: 'u-1', universe_key: 'US', version_no: 1, name: 'US Equities', state: 'ACTIVE', spec: {}, created_at: '2026-01-01T00:00:00Z' }] });
      if (url.endsWith('/idea-drafts/draft-1/start')) return jsonResponse({ id: 'program-1', state: 'ACTIVE' }, 201);
      return jsonResponse({}, 404);
    });

    renderApp(<IdeaComposerPage />, { route: '/ideas' });
    fireEvent.change(screen.getByPlaceholderText(/Test whether short-horizon/i), { target: { value: idea } });
    fireEvent.click(screen.getByRole('button', { name: 'Create draft' }));

    expect(await screen.findByText('What is the holding horizon?')).toBeInTheDocument();
    fireEvent.change(screen.getByRole('textbox', { name: 'What is the holding horizon?' }), { target: { value: '1D' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save clarifications' }));
    expect(await screen.findByText('Does drift persist?')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Start Research' }));

    await waitFor(() => expect(fetchMock.mock.calls.map(([path]) => path)).toEqual([
      '/api/v1/idea-drafts',
      '/api/v1/idea-drafts/draft-1/answers',
      '/api/v1/universes',
      '/api/v1/idea-drafts/draft-1/start',
    ]));
    const answerCall = fetchMock.mock.calls.find(([path]) => String(path).endsWith('/answers'));
    const startCall = fetchMock.mock.calls.find(([path]) => String(path).endsWith('/start'));
    expect(JSON.parse(answerCall?.[1]?.body as string)).toEqual({
      answers: { horizon: '1D' },
      expected_revision: 1,
    });
    expect(JSON.parse(startCall?.[1]?.body as string)).toEqual({ expected_revision: 2 });
  });

  it('shows a contract error instead of proceeding on a malformed draft response', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(() => jsonResponse({}, 201));

    renderApp(<IdeaComposerPage />, { route: '/ideas' });
    fireEvent.change(screen.getByPlaceholderText(/Test whether short-horizon/i), { target: { value: idea } });
    fireEvent.click(screen.getByRole('button', { name: 'Create draft' }));

    expect(await screen.findByText('CONTRACT_MISMATCH')).toBeInTheDocument();
    expect(screen.getByText('Expected a complete idea draft response.')).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('button', { name: 'Start Research' })).not.toBeInTheDocument();
  });
});
