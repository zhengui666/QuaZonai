import { CheckCircleIcon, FlaskIcon, WarningCircleIcon } from '@phosphor-icons/react';
import { Button, Callout, RadioGroup, TextArea, TextField } from '@radix-ui/themes';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { PageHeader } from '../components/ui/PageHeader';
import { Section } from '../components/ui/Section';
import { useI18n } from '../i18n';
import { useIdeaPreview, useStartResearch } from '../lib/api/hooks';

export function IdeaComposerPage() {
  const { t } = useI18n();
  const [idea, setIdea] = useState('');
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [overlapAction, setOverlapAction] = useState('recommended');
  const preview = useIdeaPreview();
  const start = useStartResearch();
  const navigate = useNavigate();
  const canPreview = idea.trim().length >= 12;
  const result = preview.data;
  const questions = result?.clarification_questions ?? [];
  const answersComplete = questions.every((question) => answers[question.key]?.trim());

  async function launch() {
    const program = await start.mutateAsync({ idea: idea.trim(), answers, overlap_action: overlapAction });
    navigate(`/research/${program.id}`);
  }

  return (
    <>
      <PageHeader
        title="Idea Composer"
        description="Describe the investment question in natural language. QuaZonai resolves implementation details itself and only asks when ambiguity would materially change the research boundary."
      />
      <div className="qz-split">
        <Section title="Research idea" meta="One clarification round maximum">
          <div className="qz-panel qz-panel-pad qz-form-grid">
            <label className="qz-field">
              <span className="qz-label">{t('idea.question')}</span>
              <TextArea
                dir="auto"
                size="3"
                rows={8}
                value={idea}
                onChange={(event) => setIdea(event.target.value)}
                placeholder={t('idea.placeholder')}
              />
              <span className="qz-help">{t('idea.help')}</span>
            </label>
            <Button disabled={!canPreview || preview.isPending} onClick={() => preview.mutate(idea.trim())}>
              <FlaskIcon size={15} />{preview.isPending ? t('idea.analyzing') : t('idea.preview')}
            </Button>
            {preview.error ? <ErrorPanel error={preview.error} /> : null}
          </div>
        </Section>
        <Section title="Charter preview" meta="Frozen after Start Research">
          {!result ? (
            <div className="qz-empty"><div><strong>{t('idea.noCharter')}</strong><div>{t('idea.noCharterDesc')}</div></div></div>
          ) : (
            <div className="qz-panel qz-panel-pad qz-form-grid">
              {result.overlap ? (
                <Callout.Root color="amber" size="1">
                  <Callout.Icon><WarningCircleIcon /></Callout.Icon>
                  <Callout.Text dir="auto"><strong>{result.overlap.kind}</strong> · {result.overlap.rationale ?? result.overlap.recommendation ?? t('idea.overlapFallback')}</Callout.Text>
                </Callout.Root>
              ) : (
                <Callout.Root color="green" size="1">
                  <Callout.Icon><CheckCircleIcon /></Callout.Icon>
                  <Callout.Text>{t('idea.noOverlap')}</Callout.Text>
                </Callout.Root>
              )}
              <div><div className="qz-label">{t('idea.researchQuestion')}</div><div dir="auto" style={{ marginTop: 5, fontSize: 13 }}>{result.charter?.research_question ?? idea}</div></div>
              <div className="qz-grid-2">
                <div><div className="qz-label">{t('idea.marketScope')}</div><div dir="auto" className="qz-list-subtitle">{Array.isArray(result.charter?.market_scope) ? result.charter.market_scope.join(', ') : result.charter?.market_scope ?? t('common.systemInferred')}</div></div>
                <div><div className="qz-label">{t('idea.predictionHorizon')}</div><div dir="auto" className="qz-list-subtitle">{result.charter?.prediction_horizon ?? t('common.systemInferred')}</div></div>
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
              {result.overlap ? (
                <RadioGroup.Root value={overlapAction} onValueChange={setOverlapAction}>
                  <RadioGroup.Item value="recommended">{t('idea.useRecommendation')}</RadioGroup.Item>
                  <RadioGroup.Item value="new-program">{t('idea.createRelated')}</RadioGroup.Item>
                  <RadioGroup.Item value="independent-program">{t('idea.createIndependent')}</RadioGroup.Item>
                </RadioGroup.Root>
              ) : null}
              <Button color="green" disabled={start.isPending || (questions.length > 0 && !answersComplete)} onClick={launch}>
                {start.isPending ? t('common.starting') : t('idea.startResearch')}
              </Button>
              {start.error ? <ErrorPanel error={start.error} /> : null}
            </div>
          )}
        </Section>
      </div>
    </>
  );
}
