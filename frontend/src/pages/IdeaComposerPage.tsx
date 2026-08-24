import { CheckCircleIcon, FlaskIcon, WarningCircleIcon } from '@phosphor-icons/react';
import { Button, Callout, RadioGroup, TextArea, TextField } from '@radix-ui/themes';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { PageHeader } from '../components/ui/PageHeader';
import { Section } from '../components/ui/Section';
import { useIdeaPreview, useStartResearch } from '../lib/api/hooks';

export function IdeaComposerPage() {
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
              <span className="qz-label">What should the research system investigate?</span>
              <TextArea
                size="3"
                rows={8}
                value={idea}
                onChange={(event) => setIdea(event.target.value)}
                placeholder="Example: Test whether short-horizon post-earnings drift in liquid US equities remains predictive after realistic turnover and capacity costs."
              />
              <span className="qz-help">State the market, economic intuition, horizon, or explicit exclusions if they matter. Do not specify model classes or optimization algorithms unless they are part of the investment hypothesis.</span>
            </label>
            <Button disabled={!canPreview || preview.isPending} onClick={() => preview.mutate(idea.trim())}>
              <FlaskIcon size={15} />{preview.isPending ? 'Analyzing…' : 'Preview research charter'}
            </Button>
            {preview.error ? <ErrorPanel error={preview.error} /> : null}
          </div>
        </Section>
        <Section title="Charter preview" meta="Frozen after Start Research">
          {!result ? (
            <div className="qz-empty"><div><strong>No charter yet</strong><div>Preview the idea to see the immutable research boundary before committing compute and evidence.</div></div></div>
          ) : (
            <div className="qz-panel qz-panel-pad qz-form-grid">
              {result.overlap ? (
                <Callout.Root color="amber" size="1">
                  <Callout.Icon><WarningCircleIcon /></Callout.Icon>
                  <Callout.Text><strong>{result.overlap.kind}</strong> · {result.overlap.rationale ?? result.overlap.recommendation ?? 'Existing research may overlap.'}</Callout.Text>
                </Callout.Root>
              ) : (
                <Callout.Root color="green" size="1">
                  <Callout.Icon><CheckCircleIcon /></Callout.Icon>
                  <Callout.Text>No material overlap detected.</Callout.Text>
                </Callout.Root>
              )}
              <div><div className="qz-label">Research question</div><div style={{ marginTop: 5, fontSize: 13 }}>{result.charter?.research_question ?? idea}</div></div>
              <div className="qz-grid-2">
                <div><div className="qz-label">Market scope</div><div className="qz-list-subtitle">{Array.isArray(result.charter?.market_scope) ? result.charter.market_scope.join(', ') : result.charter?.market_scope ?? 'System inferred'}</div></div>
                <div><div className="qz-label">Prediction horizon</div><div className="qz-list-subtitle">{result.charter?.prediction_horizon ?? 'System inferred'}</div></div>
              </div>
              {questions.length ? (
                <div className="qz-form-grid">
                  <div className="qz-label">Material clarification</div>
                  {questions.map((question) => (
                    <label className="qz-field" key={question.key}>
                      <span style={{ fontSize: 12 }}>{question.question}</span>
                      <TextField.Root value={answers[question.key] ?? ''} onChange={(event) => setAnswers((current) => ({ ...current, [question.key]: event.target.value }))} />
                    </label>
                  ))}
                </div>
              ) : null}
              {result.overlap ? (
                <RadioGroup.Root value={overlapAction} onValueChange={setOverlapAction}>
                  <RadioGroup.Item value="recommended">Use system recommendation</RadioGroup.Item>
                  <RadioGroup.Item value="new-program">Create related program</RadioGroup.Item>
                  <RadioGroup.Item value="independent-program">Create independent program with inherited evidence burden</RadioGroup.Item>
                </RadioGroup.Root>
              ) : null}
              <Button color="green" disabled={start.isPending || (questions.length > 0 && !answersComplete)} onClick={launch}>
                {start.isPending ? 'Starting…' : 'Start Research'}
              </Button>
              {start.error ? <ErrorPanel error={start.error} /> : null}
            </div>
          )}
        </Section>
      </div>
    </>
  );
}
