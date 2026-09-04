import { FlaskIcon } from '@phosphor-icons/react';
import { Button, TextArea, TextField } from '@radix-ui/themes';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { PageHeader } from '../components/ui/PageHeader';
import { Section } from '../components/ui/Section';
import { useI18n } from '../i18n';
import { ApiError, answerIdeaDraft, createIdeaDraft, startIdeaDraft } from '../lib/api/client';
import type { IdeaDraft, ResearchProgram } from '../lib/api/types';
import { localizeSystemInferred } from '../lib/format';

function requireDraft(value: unknown): IdeaDraft {
  if (!value || typeof value !== 'object') {
    throw new ApiError(
      { kind: 'contract', message: 'Expected a complete idea draft response.' },
      0,
      'CONTRACT_MISMATCH',
    );
  }
  const draft = value as Partial<IdeaDraft>;
  if (
    typeof draft.id !== 'string'
    || !Number.isInteger(draft.revision)
    || typeof draft.stage !== 'string'
    || (draft.next_action !== null && typeof draft.next_action !== 'string')
    || !Array.isArray(draft.blocking_reasons)
    || !Array.isArray(draft.clarification_questions)
    || !draft.clarification_questions.every((question) => (
      Boolean(question)
      && typeof question.key === 'string'
      && typeof question.question === 'string'
    ))
  ) {
    throw new ApiError(
      { kind: 'contract', message: 'Expected a complete idea draft response.' },
      0,
      'CONTRACT_MISMATCH',
    );
  }
  return draft as IdeaDraft;
}

function requireProgram(value: unknown): ResearchProgram {
  if (!value || typeof value !== 'object' || typeof (value as Partial<ResearchProgram>).id !== 'string') {
    throw new ApiError(
      { kind: 'contract', message: 'Expected a research program response.' },
      0,
      'CONTRACT_MISMATCH',
    );
  }
  return value as ResearchProgram;
}

export function IdeaComposerPage() {
  const { t } = useI18n();
  const [idea, setIdea] = useState('');
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [draft, setDraft] = useState<IdeaDraft | null>(null);
  const [error, setError] = useState<unknown>();
  const [pending, setPending] = useState(false);
  const navigate = useNavigate();
  const canCreate = idea.trim().length >= 12;
  const questions = draft?.clarification_questions ?? [];
  const answersComplete = questions.every((question) => answers[question.key]?.trim());
  const needsAnswers = draft?.next_action === 'ANSWER_CLARIFICATIONS';

  async function createDraft() {
    setPending(true);
    setError(undefined);
    try {
      setDraft(requireDraft(await createIdeaDraft({ original_idea_text: idea.trim() })));
    } catch (requestError) {
      setError(requestError);
    } finally {
      setPending(false);
    }
  }

  async function submitAnswers() {
    if (!draft || draft.next_action !== 'ANSWER_CLARIFICATIONS') return;
    setPending(true);
    setError(undefined);
    try {
      setDraft(requireDraft(await answerIdeaDraft(draft.id, {
        answers,
        expected_revision: draft.revision,
      })));
    } catch (requestError) {
      setError(requestError);
    } finally {
      setPending(false);
    }
  }

  async function launch() {
    if (!draft) return;
    setPending(true);
    setError(undefined);
    try {
      if (draft.next_action !== 'START_PROGRAM') {
        throw new ApiError(
          { kind: 'contract', message: 'The idea draft is not ready to start.' },
          0,
          'CONTRACT_MISMATCH',
        );
      }
      const program = requireProgram(await startIdeaDraft(draft.id, { expected_revision: draft.revision }));
      navigate(`/research/${program.id}`);
    } catch (requestError) {
      setError(requestError);
    } finally {
      setPending(false);
    }
  }

  return (
    <>
      <PageHeader
        title="Idea Composer"
        description="Describe the investment question in natural language. QuaZonai resolves implementation details itself and only asks when ambiguity would materially change the research boundary."
      />
      <div className="qz-split">
        <Section title="Research idea" meta={t('idea.oneRound', { count: 1 })}>
          <div className="qz-panel qz-panel-pad qz-form-grid">
            <label className="qz-field">
              <span className="qz-label">{t('idea.question')}</span>
              <TextArea
                dir="auto"
                size="3"
                rows={8}
                value={idea}
                onChange={(event) => {
                  setIdea(event.target.value);
                  setDraft(null);
                  setAnswers({});
                  setError(undefined);
                }}
                placeholder={t('idea.placeholder')}
              />
              <span className="qz-help">{t('idea.help')}</span>
            </label>
            <Button disabled={!canCreate || pending || Boolean(draft)} onClick={() => void createDraft()}>
              <FlaskIcon size={15} />{pending ? t('common.saving') : t('idea.createDraft')}
            </Button>
            {error ? <ErrorPanel error={error} /> : null}
          </div>
        </Section>
        <Section title={t('idea.charterPreview')} meta={t('idea.frozenAfter')}>
          {!draft ? (
            <div className="qz-empty"><div><strong>{t('idea.noCharter')}</strong><div>{t('idea.noCharterDesc')}</div></div></div>
          ) : (
            <div className="qz-panel qz-panel-pad qz-form-grid">
              <div><div className="qz-label">{t('idea.researchQuestion')}</div><div dir="auto" style={{ marginTop: 5, fontSize: 13 }}>{draft.charter?.research_question ?? idea}</div></div>
              <div className="qz-grid-2">
                <div><div className="qz-label">{t('idea.marketScope')}</div><div className="qz-list-subtitle">{Array.isArray(draft.charter?.market_scope) ? draft.charter.market_scope.map((scope, index) => <span key={`${scope}-${index}`}>{index ? ', ' : null}<bdi dir="auto">{localizeSystemInferred(scope, t('common.systemInferred'))}</bdi></span>) : <bdi dir="auto">{localizeSystemInferred(draft.charter?.market_scope, t('common.systemInferred')) ?? t('common.systemInferred')}</bdi>}</div></div>
                <div><div className="qz-label">{t('idea.predictionHorizon')}</div><div dir="auto" className="qz-list-subtitle">{localizeSystemInferred(draft.charter?.prediction_horizon, t('common.systemInferred')) ?? t('common.systemInferred')}</div></div>
              </div>
              {questions.length ? (
                <div className="qz-form-grid">
                  <div className="qz-label">{t('idea.materialClarification')}</div>
                  {questions.map((question) => (
                    <label className="qz-field" key={question.key}>
                      <span dir="auto" style={{ fontSize: 12 }}>{question.question}</span>
                      <TextField.Root dir="auto" value={answers[question.key] ?? ''} onChange={(event) => setAnswers((current) => ({ ...current, [question.key]: event.target.value }))} />
                    </label>
                  ))}
                </div>
              ) : null}
              <Button color="green" disabled={pending || (needsAnswers && !answersComplete)} onClick={() => void (needsAnswers ? submitAnswers() : launch())}>
                {pending ? (needsAnswers ? t('common.saving') : t('common.starting')) : (needsAnswers ? t('idea.saveClarifications') : t('idea.startResearch'))}
              </Button>
            </div>
          )}
        </Section>
      </div>
    </>
  );
}
