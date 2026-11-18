import { useTranslation } from 'react-i18next';
import { tr } from './i18n';
import { thinkingLabel, thinkingOptions, thinkingUnavailableReason, reasoningModeDescription } from './model-labels';
import { useEffect, useRef, useState } from 'react';
import Bubble from '@ant-design/x/es/bubble';
import XMarkdown, { type ComponentProps } from '@ant-design/x-markdown/es';
import { NativeStrategyResultView } from './research-results';
export { NativeStrategyResultView } from './research-results';
import './research-experience.css';
import { Alert, Button, Dropdown, Form, Input, Rate, Select, Table, Tabs } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import { ArrowLeftIcon, ArrowRightIcon, ArrowUpRightIcon, CaretDownIcon, DatabaseIcon, DotsThreeIcon, FlaskIcon, PlusIcon } from '@phosphor-icons/react';
import type { ResearchListItem } from '../../../packages/contracts/src/lists.ts';
import { modelSupportsThinking } from '../../../packages/contracts/src/index.ts';
import type { ThinkingLevel, ResearchDelegationView, DatasetView, ModelRef, ResearchDetail, ResearchReceipt, ServiceTier, SettingsView, TaskView, ResearchNativeBacktests } from '../../../packages/contracts/src/index.ts';
import { ApiError, api } from './api';
import { datasetResearchSettings } from '../../../packages/contracts/src/research.ts';
import { NativeInputFacts } from './native-input';
import { draftIsPersisted, readDraft, removeDraft, saveDraft, useCommand } from './commands';
import { useModelCatalog, useResearchDetail, useResource } from './data-hooks';
import { ListTools, listPathFor, useListWorkspace } from './list-workspace';
import { currentRoute, href, navigate, type Route } from './router';
import { Copy, DraftNotice, Empty, ErrorNotice, Facts, Loading, PendingCommand, Status, Timestamp, useConfirm } from './ui';
import { RecordTitle, TextDisclosure } from './disclosure';
import { shortTitle } from './titles';
import { ExecutionFailure, Reason } from './diagnostics';
import { Literature } from './literature';
import { publicLiteratureTopics } from './literature-input';
import { DeleteRecord } from './delete-record';

/** Only transport user intent; do not reconstruct retired scoring/fit specifications. */
export function nativeResearchInput(values: Record<string, string>) {
  const publicQueries = publicLiteratureTopics(values.publicLiteratureTopics);
  return { goal: values.goal, ...(publicQueries ? { publicLiteratureQueries: publicQueries } : {}), model: JSON.parse(values.model!) as ModelRef, datasetId: values.datasetId, ...(values.thinkingLevel ? { thinkingLevel: values.thinkingLevel } : {}), ...(values.serviceTier ? { serviceTier: values.serviceTier } : {}) };
}


const modelKey = (model: ModelRef) => JSON.stringify({ provider: model.provider, modelId: model.modelId });
export default function Research({ route }: { route: Route }) {
  useTranslation('core');
  if (route.creating) return <NewStudy key={route.params.get('dataset') ?? 'new'} route={route} />;
  return route.id ? <StudyDocument key={route.id} route={route} back={listPathFor(route)} /> : <ResearchWorkspace route={route} />;
}
function ResearchWorkspace({ route }: { route: Route }) {
  useTranslation('core');
  const list = useListWorkspace<ResearchListItem>(route, '/research');
  return <section className="list-workspace" aria-label={tr("Research list")}>
    <header className="list-heading"><h1>{tr("Research")}</h1><Button type="primary" className="new-study-button" icon={<PlusIcon size={15} />} onClick={() => navigate(list.detailPath('new'))}>{tr("New research")}</Button></header>
    <ListTools list={list} searchLabel={tr("Search research")} placeholder={tr("Search research names, questions, evidence, or datasets")} filters={[
      { key: 'status', label: tr("Research status") }, { key: 'datasetId', label: tr("Research dataset") },
      { key: 'modality', label: tr("Input type"), advanced: true }, { key: 'model', label: tr("Research model"), advanced: true },
    ]} />
    <ErrorNotice error={list.query.error} retry={() => { void list.query.refetch(); }} />
    {list.query.isPending ? <Loading /> : !list.data && list.query.error ? null : <Table<ResearchListItem> {...list.tableProps} rowKey="id" size="small" scroll={{ x: 1120 }} locale={{ emptyText: <Empty title={list.hasFilters ? tr("No matching research") : tr("No research yet")} /> }} columns={[
      { title: tr("Research name"), key: 'title', width: 240, ellipsis: { showTitle: false }, ...list.sortColumn('title'), render: (_value, row) => <a className="list-record-name" {...list.linkProps(row.id)}>{shortTitle(row.title, undefined, tr("Untitled research"))}</a> },
      { title: tr("Dataset"), key: 'datasetName', width: 190, ellipsis: { showTitle: false }, ...list.sortColumn('datasetName'), render: (_value, row) => <span className="list-cell-text">{row.datasetName ? shortTitle(row.datasetName) : tr("Not recorded")}</span> },
      { title: tr("Execution status"), key: 'status', width: 130, render: (_value, row) => <Status plain value={row.status} label={row.status === 'UNKNOWN' ? tr("Execution outcome unconfirmed") : row.status === 'STARTED' ? tr("Native execution in progress") : undefined} /> },
    ]} />}
  </section>;
}
export function hasActiveResearchExecution(detail: Pick<ResearchDetail, 'tasks' | 'submission'>): boolean {
  return detail.tasks.some(task => task.status !== 'terminal') || ['queued', 'placed'].includes(detail.submission?.status ?? '');
}
export function ResearchUncertainty({ detail }: { detail: Pick<ResearchDetail, 'executionUncertain' | 'executionUncertainty'> }) {
  useTranslation('core');
  if (!detail.executionUncertain) return null;
  return <Alert className="research-uncertainty" type="warning" showIcon title={tr("Execution outcome is not yet confirmed")} description={detail.executionUncertainty?.reason === 'nautilus_cancellation_unknown'
    ? tr("The stop request's outcome is unconfirmed. Keep the original task and wait for its execution evidence; do not resend it automatically. Earlier results do not prove that this execution has ended.")
    : tr("Waiting for the original execution evidence without repeating the run. Earlier completed results are preserved, but do not establish success or termination of this execution.")} />;
}
function StudyDocument({ route, back }: { route: Route; back: string }) {
  useTranslation('core');
  const cache = useQueryClient(); const query = useResearchDetail(route.id!);
  const [stopError, setStopError] = useState<unknown>(); const [stopping, setStopping] = useState(false);
  const confirm = useConfirm(); const detail = query.data;
  const tab = route.params.get('tab') ?? 'conversation';
  const selectTab = (value: string) => { const params = new URLSearchParams(route.params); params.set('tab', value); navigate(`/research/${route.id}?${params}`, true); };
  if (!detail) return <article className="study-document"><header className="study-heading"><h1>{tr("Research")}</h1></header>{query.isPending ? <Loading /> : <ErrorNotice error={query.error ?? new Error(tr("Research not found"))} retry={() => { void query.refetch(); }} />}<a className="text-link" href={href(back)}>{tr("Back to research list")}</a></article>;
  const active = hasActiveResearchExecution(detail);
  const stop = async () => { setStopping(true); setStopError(undefined); try { const value = await api<ResearchDetail>(`/research/${detail.id}/stop`, { method: 'POST' }); cache.setQueryData([`/research/${detail.id}`], value); await cache.invalidateQueries({ queryKey: ['/research'] }); } catch (error) { setStopError(error); } finally { setStopping(false); } };
  const stoppedLabel = detail.status === 'STOPPED' && active ? tr("Stopping") : detail.status === 'STARTED' ? tr("Native execution in progress") : undefined;
  const originalSubmission = detail.originalSubmission ?? detail.submission;
  const nativeSettings = datasetResearchSettings(detail.input.data);
  return <article className="study-document"><header className="study-heading"><div className="document-toolbar"><div className="document-identity"><a className="list-back text-link" href={href(back)}><ArrowLeftIcon size={14} />{tr("Research list")}</a><Status value={detail.executionUncertain ? 'UNKNOWN' : detail.status} label={detail.executionUncertain ? tr("Execution outcome unconfirmed") : stoppedLabel} /></div><Dropdown trigger={['click']} menu={{ items: [{ key: 'inputs', label: tr("View execution details"), onClick: () => selectTab('inputs') }, ...(active ? [{ key: 'stop', label: stopping ? tr("Stopping") : tr("Stop research"), danger: true, disabled: stopping, onClick: () => confirm(tr("Stop research?"), tr("Stop the original execution of this research."), () => { void stop(); }) }] : [])] }}><Button className="icon-button" type="text" aria-label={tr("Research actions")} icon={<DotsThreeIcon size={21} />} /></Dropdown><DeleteRecord key={detail.id} kind="research" id={String(detail.id)} back={back} /></div>
      <RecordTitle text={detail.title} fallback={tr("Research")} />
      <div className="study-context"><span><DatabaseIcon size={14} />{detail.dataset?.name ?? detail.datasetId ?? tr("Dataset name not recorded")}</span></div></header>
      <ResearchUncertainty detail={detail} /><ErrorNotice error={stopError} /><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
      <Tabs className="document-tabs" activeKey={tab} onChange={value => selectTab(value)} items={[
        { key: 'conversation', label: tr("Conversation"), children: <div className="research-chat"><ResearchConversation key={detail.id} messages={detail.messages} researchId={detail.id} nextCursor={detail.messagesNextCursor} />{active && !detail.executionUncertain && <Alert type="info" title={tr("Research in progress")} />}{detail.toolPolicy === 'NAUTILUS_ONLY' && <NativeResearchContinuation key={detail.id} detail={detail} active={active} changed={() => { void query.refetch(); void cache.invalidateQueries({ queryKey: ['/research'] }); }} />}</div> },
        { key: 'evidence', label: tr("Results and evaluation"), children: <div className="evidence-content"><ResearchAssessments assessments={detail.assessments} researchId={detail.id} /><NativeBacktestHistory key={detail.id} researchId={detail.id} active={active} />
          {nativeSettings.modality === 'TRADE_TICK' && <section className="result-section"><h3>{tr("Tick inputs and evidence scope")}</h3><NativeInputFacts settings={nativeSettings} preparation={detail.dataset?.preparation} coverage={detail.final?.inputCoverage ?? detail.dataset?.inputCoverage} /></section>}
          {!detail.final ? <Empty title={active ? tr("No native results yet") : tr("No native results were produced")} action={<Button onClick={() => selectTab('inputs')}>{tr("View execution")}</Button>} /> : <section className="independent-result"><div className="section-heading"><h3>{detail.final.nativeStrategyResult ? detail.executionUncertain ? tr("Previously completed backtest results") : tr("Native Nautilus historical strategy backtest") : tr("Execution results")}</h3></div>
            {detail.final.reason === 'explicit_native_parameter_batch_completed_displaying_last_case_not_best' && <Alert type="info" title={tr("The last result group is shown below, not a selected optimum or combined result")} description={tr("See the native backtest records for each original request and result. Search performance does not establish independent out-of-sample validity.")} />}{detail.final.nativeStrategyResult && <NativeStrategyResultView key={String(detail.final.nativeStrategyResult.resultTaskId ?? detail.submission?.id)} value={detail.final.nativeStrategyResult} researchId={detail.id} />}
            {detail.final.status !== 'COMPUTED' && <Status value={detail.final.status} label={detail.final.status === 'STARTED' ? tr("Native execution in progress") : detail.final.status === 'FAILED' ? tr("Native execution failed") : undefined} />}
            <ExecutionFailure error={detail.final.error} detail={detail.final.errorDetail} identity={tr("Research #{{id}}{{trial}}", { id: detail.id, trial: detail.final.trialId !== null ? tr(" / Trial #{{id}}", { id: detail.final.trialId }) : '' })} title={tr("Native execution failed")} />
            {detail.final.reason && detail.final.reason !== detail.final.error && <details className="reason-disclosure"><summary>{tr("Research conclusion")}</summary><p>{detail.final.reason}</p></details>}
          </section>}
        </div> },
        { key: 'literature', label: tr("Literature"), children: <Literature value={detail.literature} publicQueries={detail.input.publicLiteratureQueries} /> },
        { key: 'inputs', label: tr("Execution details"), children: <div className="input-content"><section><h3>{tr("Research input")}</h3><TextDisclosure label={tr("Original research objective")} value={detail.input.goal} collapsed /><Facts items={[[tr("Dataset"), detail.dataset?.name ?? detail.datasetId ?? tr("Not recorded")], [tr("Source"), detail.dataset?.source ?? tr("Not recorded")], [tr("Model"), detail.model], [tr("Requested reasoning effort"), thinkingLabel(detail.reasoningConfiguration?.requestedLevel ?? detail.input.thinkingLevel ?? tr("Not recorded"))], [tr("Saved model reasoning settings"), detail.reasoningConfiguration ? thinkingLabel(detail.reasoningConfiguration.effectiveLevel) : tr("Not recorded")], [tr("Execution method"), detail.reasoningConfiguration?.mode === 'PI_MULTI_AGENT' ? tr("Pi multi-agent analysis delegation (saved configuration, not proof of success)") : detail.reasoningConfiguration?.mode === 'NATIVE' ? tr("Native model reasoning (saved configuration, not proof of success)") : tr("Not recorded")], [tr("Processing tier"), detail.input.serviceTier === 'priority' ? 'Fast' : detail.input.serviceTier === 'default' ? tr("Standard") : tr("Default tier")], [tr("Coverage window"), detail.dataset ? <><Timestamp value={detail.dataset.start} /><span>  {tr("to")} </span><Timestamp value={detail.dataset.end} /></> : tr("Not recorded")], [tr("Created"), <Timestamp value={detail.createdAt} />]]} /></section><section><h3>{tr("Original identity")}</h3><Facts items={[[tr("Research"), <><code>{detail.id}</code><Copy value={String(detail.id)} label={tr("Copy research ID")} /></>], [tr("Request"), originalSubmission?.requestId ? <><code>{originalSubmission.requestId}</code><Copy value={originalSubmission.requestId} label={tr("Copy request ID")} /></> : tr("Not recorded")], [tr("Submission"), originalSubmission ? <code>{originalSubmission.id}</code> : tr("Not recorded")], [tr("Submission status"), originalSubmission ? <Status value={originalSubmission.status} /> : tr("Not recorded")], [tr("Terminal reason"), originalSubmission?.reason ? <Reason value={originalSubmission.reason} /> : tr("Not provided")]]} /></section>{detail.submission && detail.submission.id !== originalSubmission?.id && <section><h3>{tr("Current execution")}</h3><Facts items={[[tr("Request"), <code>{detail.submission.requestId ?? tr("Not recorded")}</code>], [tr("Submission"), <code>{detail.submission.id}</code>], [tr("Submission status"), <Status value={detail.submission.status} />], [tr("Terminal reason"), detail.submission.reason ? <Reason value={detail.submission.reason} /> : tr("Not provided")]]} /></section>}
          {!!detail.previousStops?.length && <section><h3>{tr("Historical stop records")}</h3>{detail.previousStops.map(stop => <Facts key={stop.previousSubmissionId} items={[[tr("Original submission"), <code>{stop.previousSubmissionId}</code>], [tr("Stop task"), <code>{String(stop.final.taskId ?? tr("Not recorded"))}</code>], [tr("Status"), <Status value={String(stop.final.status)} />], [tr("Stop reason"), typeof stop.final.reason === 'string' ? stop.final.reason : tr("Not recorded")]]} />)}</section>}
          {!!detail.previousNativeResults?.length && <section><h3>{tr("Historical native research results")}</h3>{detail.previousNativeResults.map(run => <div key={run.previousSubmissionId}><p>{tr("Original submission #")}{run.previousSubmissionId}  {tr("· Task #")}{String(run.final.taskId ?? tr("Not recorded"))}</p><Status value={String(run.final.status ?? 'UNKNOWN')} /></div>)}</section>}
          <ResearchDelegations delegations={detail.delegations} /><section><h3>{tr("Recent execution records")}</h3><Table<TaskView> rowKey="id" size="small" dataSource={detail.tasks} pagination={{ pageSize: 10, hideOnSinglePage: true }} scroll={{ x: 500 }} columns={[{ title: tr("Task"), render: (_value, task) => <div className="trial-name">{task.name}<code>#{task.id}</code></div> }, { title: tr("Status"), render: (_value, task) => <Status value={task.outcome ?? task.status} /> }, { title: tr("Error"), render: (_value, task) => task.error ? <Reason value={task.error} /> : <span className="quiet">{tr("Not provided")}</span> }]} /></section></div> },
      ]} />
  </article>;
}
export function ResearchDelegations({ delegations }: { delegations: ResearchDetail['delegations'] }) {
  useTranslation('core');
  if (!delegations?.length) return null;
  return <section aria-label={tr("Pi subagent analysis records")}><h3>{tr("Pi subagent analysis records")}</h3><p>{tr("Subagents analyze materials supplied by the parent agent; their answers are not scientific tool execution results. Status and answers come from the original Pi records.")}</p>
    <Table<ResearchDelegationView> rowKey="taskId" size="small" dataSource={delegations} pagination={{ pageSize: 10, hideOnSinglePage: true }} scroll={{ x: 700 }} columns={[
      { title: tr("Original identity"), render: (_value, task) => <div>{tr("Task")} <code>#{task.taskId}</code><br />{tr("Conversation")} <code>{task.conversationId ?? tr("Not recorded")}</code><br />{tr("Submission")} <code>{task.submissionId ?? tr("Not recorded")}</code></div> },
      { title: tr("Status"), render: (_value, task) => <Status value={task.status} /> },
      { title: tr("Underlying model configuration"), render: (_value, task) => <div>{task.model ? <code>{task.model.provider}/{task.model.modelId}</code> : tr("Not recorded")}<br />{task.effectiveLevel ? thinkingLabel(task.effectiveLevel) : tr("Not recorded")}</div> },
      { title: tr("Answers and errors"), render: (_value, task) => <>{task.answer !== null && <details><summary>{tr("Expand subagent answer")}</summary><div style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{task.answer}</div></details>}{task.error && <div role="alert" style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{task.error}</div>}{task.answer === null && !task.error && <span className="quiet">{tr("No answer or error recorded yet")}</span>}</> },
    ]} />
  </section>;
}
export function NativeBacktestHistory({ researchId, active }: { researchId: number; active: boolean }) {
  useTranslation('core');
  const [cursors, setCursors] = useState<(string | null)[]>([null]);
  const cursor = cursors.at(-1);
  const query = useResource<ResearchNativeBacktests>(`/research/${researchId}/native-backtests${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ''}`, { poll: active ? 5000 : undefined });
  return <section aria-label={tr("Native backtest records")}><h3>{tr("All native backtest records")}</h3><p>{tr("Preserved in execution-task order, including failed and unfinished groups. This is not a ranking or out-of-sample validation. Original evidence includes the full request and Nautilus results.")}</p>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    <Table loading={query.isPending} rowKey="taskId" size="small" pagination={false} dataSource={query.data?.items ?? []} columns={[
      { title: tr("Original task"), dataIndex: 'taskId' }, { title: tr("Status"), dataIndex: 'status', render: value => <Status value={value} /> },
      { title: tr("Original request and result"), render: (_value, row) => <a href={row.evidenceHref} target="_blank" rel="noreferrer">{tr("Open original JSON")}</a> },
    ]} />
    <Button disabled={cursors.length <= 1 || query.isFetching} onClick={() => setCursors(previous => previous.slice(0, -1))}>{tr("Previous page")}</Button>
    <Button disabled={!query.data?.nextCursor || query.isFetching} onClick={() => { if (query.data?.nextCursor) setCursors(previous => [...previous, query.data!.nextCursor]); }}>{tr("Next page")}</Button>
    <Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>{tr("Refresh")}</Button>
  </section>;
}

export function ResearchRating({ value }: { value: unknown }) {
  useTranslation('core');
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 0 || value > 5) return <span className="quiet">{tr("Unrated")}</span>;
  return <span className="research-rating"><span aria-hidden="true"><Rate disabled value={value} count={5} style={{ color: '#fadb14', fontSize: 17 }} /></span><span>{value}  {tr("/ 5 stars")}</span></span>;
}
function AssessmentLimitations({ value }: { value: Record<string, unknown> }) {
  useTranslation('core');
  const labels: Record<string, string> = {
    CPCV_NOT_RUN: tr("Combinatorial purged cross-validation has not been completed."),
    NO_AUTOMATIC_SPLITTING_OR_SELECTION: tr("This run did not automatically split samples or select a preferred strategy."),
    CROSS_CONVERSATION_TRIAL_HISTORY_NOT_AUDITED: tr("Trial history in other research conversations has not been reviewed."),
    INDEPENDENT_SOURCE_DATA_AUDIT_NOT_RUN: tr("An independent data-source review has not been completed."),
    PRE_EXECUTION_METHOD_MISSING: tr("No explicit research method was recorded before execution."),
    LOGIC_EVIDENCE_INSUFFICIENT: tr("Current logic evidence is insufficient."),
    ORIGINAL_NATIVE_DATA_EVIDENCE_MISSING: tr("Completed native backtest evidence matching the original configuration is missing."),
  };
  const items = [...(Array.isArray(value.reasons) ? value.reasons : []), ...(Array.isArray(value.limitations) ? value.limitations : [])];
  if (!items.length) return null;
  return <div className="assessment-limitations"><h4>{tr("Evidence limitations")}</h4><ul>{items.slice(0, 20).map((item, index) => <li key={index}>{typeof item === 'string' && labels[item] ? labels[item] : tr("Additional evidence-scope limitations are not expanded here. Ask for an explanation in the conversation.")}</li>)}</ul></div>;
}

export function ResearchAssessments({ assessments, researchId }: { assessments: ResearchDetail['assessments']; researchId?: number }) {
  useTranslation('core');
  if (!assessments?.length) return null;
  const record = (value: unknown): Record<string, unknown> => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
  const stars = (value: unknown) => <ResearchRating value={value} />;
  return <section className="research-original-ratings" aria-label={tr("Original two-track research ratings")}><h2>{tr("Two-track research ratings")}</h2>
    {assessments.map(row => {
      const result = row.result ?? {}; const judgment = record(result.judgment); const data = record(result.dataJudgment);
      const evidence = record(result.evidence); const logic = record(record(record(evidence.logic).result).judgment);
      return <article key={row.taskId} id={`assessment-${row.taskId}`}>
        <h3>{row.mode === 'LOGIC' ? row.reviewTiming === 'POST_EXECUTION' ? tr("Post-execution logic review (does not replace pre-execution assessment)") : tr("Pre-execution logic review") : tr("Combined research report")}</h3>
        <p>{row.configuration === 'CURRENT' ? tr("Current configuration") : tr("Historical configuration; not applicable to current inputs")}</p><Status value={row.status} />
        <Facts items={row.mode === 'LOGIC' ? [[tr("Logic rating"), stars(judgment.stars)]] : [[tr("Logic rating"), stars(logic.stars)], [tr("Data rating"), stars(data.stars)], [tr("Overall rating"), stars(judgment.stars)], [tr("Rating status"), result.ratingStatus === 'RATED_CONDITIONAL_STATIC_SCENARIO' ? tr("Conditional scenario evaluation") : result.ratingStatus === 'NOT_RATED_INSUFFICIENT_EVIDENCE' ? tr("Insufficient evidence; unrated") : typeof judgment.stars === 'number' ? tr("Evaluation recorded; read it alongside its limitations") : tr("Unrated")]]} />
        {record(result.instrumentDefinitionAssumption).mode === 'STATIC_SCENARIO' && <p role="note">{tr("Conditional scenario: historical specifications are unverified")}</p>}
        {typeof judgment.rationale === 'string' && <p><strong>{row.mode === 'LOGIC' ? tr("Logic evidence:") : tr("Overall rationale:")}</strong>{readableResearchText(judgment.rationale)}</p>}{typeof data.rationale === 'string' && <p><strong>{tr("Data evidence:")}</strong>{readableResearchText(data.rationale)}</p>}{typeof logic.rationale === 'string' && <p><strong>{tr("Logic evidence:")}</strong>{readableResearchText(logic.rationale)}</p>}
        {row.error && <Alert type="warning" title={tr("This evaluation is incomplete")} description={tr("See execution details")} />}
        {[[tr("Risks"), judgment.risks], [tr("Uncertainty"), judgment.uncertainties], [tr("Data risks"), data.risks], [tr("Data limitations"), data.uncertainties]].map(([label, values]) => Array.isArray(values) && values.length > 0 ? <div key={String(label)}><h4>{String(label)}</h4><ul>{values.filter(value => typeof value === 'string').slice(0, 20).map((value, index) => <li key={index}>{readableResearchText(value)}</li>)}</ul></div> : null)}<AssessmentLimitations value={record(evidence.ratingReadiness)} />{researchId && <a className="text-link" href={href(`/research/${researchId}?tab=inputs`)}>{tr("View original execution records")}</a>}

      </article>;
    })}
  </section>;
}

const structuredPlaceholder = "Structured data is omitted here. Ask for a readable explanation in the conversation.";
export function readableResearchText(text: string): string {
  const structured = (value: string) => {
    try { const parsed: unknown = JSON.parse(value.trim()); return parsed !== null && typeof parsed === 'object'; } catch { return false; }
  };
  if (structured(text)) return tr(structuredPlaceholder);
  const fenced = text.replace(/```([^\n`]*)\n([\s\S]*?)(?:```|$)/g, (original, language: string, body: string) =>
    /^(json|jsonc)\b/i.test(language.trim()) || structured(body) ? tr(structuredPlaceholder) : original);
  // Presentation policy: withhold JSON-shaped fragments, including unfinished streams.
  // This does not validate or modify the stored message or infer any business fields.
  const startsStructure = (index: number) => {
    // A single bracketed integer in prose is a citation, not a report array.
    // Standalone arrays were handled above; fenced JSON is also always withheld.
    if (fenced[index] === '[' && /^\[\d+\]/.test(fenced.slice(index)) && fenced[index - 1] !== '`') return false;
    let next = index + 1;
    while (next < fenced.length && /\s/.test(fenced[next]!)) next++;
    if (next === fenced.length) return true;
    if (fenced[index] === '{') return fenced[next] === '"' || fenced[next] === '}';
    return /[\[\]{"\d-]/.test(fenced[next]!) || ['true', 'false', 'null'].some(literal => fenced.startsWith(literal, next));
  };
  let output = ''; let copied = 0; let candidate = -1;
  const stack: string[] = []; let quoted = false; let escaped = false;
  for (let index = 0; index < fenced.length; index++) {
    const char = fenced[index];
    if (candidate < 0) { if ((char !== '{' && char !== '[') || !startsStructure(index)) continue; candidate = index; }
    if (quoted) { if (escaped) escaped = false; else if (char === '\\') escaped = true; else if (char === '"') quoted = false; continue; }
    if (char === '"') { quoted = true; continue; }
    if (char === '{' || char === '[') stack.push(char);
    else if (char === '}' || char === ']') {
      if (stack.pop() !== (char === '}' ? '{' : '[')) { output += fenced.slice(copied, candidate) + tr(structuredPlaceholder); copied = index + 1; stack.length = 0; candidate = -1; continue; }
      if (stack.length === 0) {
        output += fenced.slice(copied, candidate) + tr(structuredPlaceholder); copied = index + 1;
        candidate = -1;
      }
    }
  }
  return candidate >= 0 ? output + fenced.slice(copied, candidate) + tr(structuredPlaceholder) : output + fenced.slice(copied);
}
export const safeMarkdownComponents = {
  a: ({ href, children }: Partial<ComponentProps>) => {
    let safe: string | undefined;
    try { const url = new URL(String(href)); if (['https:', 'http:'].includes(url.protocol) && !url.username && !url.password) safe = url.href; } catch { /* Unsupported links remain text. */ }
    return safe ? <a href={safe} target="_blank" rel="noopener noreferrer">{children}</a> : <span>{children}</span>;
  },
  img: ({ alt }: Partial<ComponentProps>) => <span className="quiet">{typeof alt === 'string' && alt ? tr("Image: {{alt}}", { alt }) : tr("Image not loaded automatically")}</span>,
  code: ({ children, lang }: Partial<ComponentProps>) => /^(json|jsonc)$/i.test(lang ?? '') ? <span className="quiet">{tr("Structured data is omitted here. Ask for a readable explanation in the conversation.")}</span> : <code>{children}</code>,
};
export function ResearchMarkdown({ content }: { content: string }) {
  useTranslation('core'); return <XMarkdown content={readableResearchText(content)} escapeRawHtml components={safeMarkdownComponents} />; }
export function mergeResearchMessages(older: NonNullable<ResearchDetail['messages']>, newer: NonNullable<ResearchDetail['messages']>) {
  return [...new Map([...older, ...newer].map(message => [message.entryId, message])).values()].sort((a, b) => a.entryId - b.entryId);
}
export type ResearchHistoryState = { messages: NonNullable<ResearchDetail['messages']>; cursor?: string | null; serverCursor?: string | null; generation: number };
export function syncResearchWindow(state: ResearchHistoryState, incoming: NonNullable<ResearchDetail['messages']>, nextCursor?: string | null): ResearchHistoryState {
  const lastSeen = state.messages.at(-1)?.entryId;
  const jumped = lastSeen !== undefined && incoming.length > 0 && incoming[0]!.entryId > lastSeen
    || (incoming.length === 0 || lastSeen === undefined) && nextCursor != null && nextCursor !== state.serverCursor;
  return { messages: mergeResearchMessages(state.messages, incoming), cursor: jumped ? nextCursor : state.cursor, serverCursor: nextCursor, generation: state.generation + (jumped ? 1 : 0) };
}
export function acceptResearchHistoryPage(state: ResearchHistoryState, page: { messages: NonNullable<ResearchDetail['messages']>; nextCursor: string | null }, requestedGeneration: number): ResearchHistoryState {
  return { ...state, messages: mergeResearchMessages(page.messages, state.messages), cursor: requestedGeneration === state.generation ? page.nextCursor : state.cursor };
}
export function ResearchConversation(props: { messages: ResearchDetail['messages']; researchId?: number; nextCursor?: string | null }) {
  useTranslation('core');
  return <ConversationSession key={props.researchId ?? 'standalone'} {...props} />;
}
function ConversationSession({ messages, researchId, nextCursor }: { messages: ResearchDetail['messages']; researchId?: number; nextCursor?: string | null }) {
  useTranslation('core');
  const [history, setHistory] = useState<ResearchHistoryState>({ messages: messages ?? [], serverCursor: nextCursor, generation: 0 });
  useEffect(() => { setHistory(previous => syncResearchWindow(previous, messages ?? [], nextCursor)); }, [messages, nextCursor]);
  const [loading, setLoading] = useState(false); const [error, setError] = useState<unknown>(); const inFlight = useRef(false);
  const next = history.cursor === undefined ? nextCursor : history.cursor;
  const loadOlder = async () => {
    if (!researchId || !next || inFlight.current) return;
    const requestedGeneration = history.generation;
    inFlight.current = true; setLoading(true); setError(undefined);
    try { const page = await api<{ messages: NonNullable<ResearchDetail['messages']>; nextCursor: string | null }>(`/research/${researchId}/messages?cursor=${encodeURIComponent(next)}`); setHistory(previous => acceptResearchHistoryPage(previous, page, requestedGeneration)); }
    catch (failure) { setError(failure); } finally { inFlight.current = false; setLoading(false); }
  };
  const allMessages = mergeResearchMessages(history.messages, messages ?? []);
  if (!allMessages.length && !next) return <section className="research-conversation" aria-label={tr("Research conversation")}><Empty title={tr("Start with a research question")} /></section>;
  return <section className="research-conversation" aria-label={tr("Research conversation")}>{next && <Button className="history-button" loading={loading} onClick={() => { void loadOlder(); }}>{tr("Load earlier messages")}</Button>}<ErrorNotice error={error} /><Bubble.List autoScroll={false} items={allMessages.map(message => {
    const user = 'role' in message && message.role === 'user';
    return { key: message.entryId, role: user ? 'user' : 'ai', placement: user ? 'end' as const : 'start' as const,
      header: user ? tr("You") : tr("Research assistant"), content: message.text, variant: user ? 'filled' as const : 'borderless' as const,
      contentRender: (content: string) => user ? <p className="user-message-text">{readableResearchText(content)}</p> : <ResearchMarkdown content={content} /> };
  })} /></section>;
}

export function canContinueResearch(detail: ResearchDetail, active: boolean): boolean {
  if (detail.toolPolicy !== 'NAUTILUS_ONLY' || detail.executionUncertain || active || !detail.submission) return false;
  const stopped = detail.final?.status === 'STOPPED' && detail.final.trialId === null;
  const ownerStopped = detail.submission.status === 'unanswered' && detail.submission.reason === 'aborted'
    && (stopped && detail.final?.reason === 'owner_stop' || !!detail.final?.nativeStrategyResult);
  const modelFailedAfterResult = detail.submission.status === 'unanswered' && detail.submission.reason === 'model_error' && !!detail.final?.nativeStrategyResult;
  return !!ownerStopped || modelFailedAfterResult || detail.submission.status === 'done'
    && (!detail.final || !!detail.final.nativeStrategyResult || stopped);
}

/** Reuses the existing resume endpoint and original submission identity, including uncertain retries. */
export function NativeResearchContinuation({ detail, active, changed }: { detail: ResearchDetail; active: boolean; changed: () => void }) {
  useTranslation('core');
  const key = `native-research-resume:${detail.id}`;
  type Intent = { submissionId: number; instruction: string };
  const [pending, setPending] = useState<Intent | undefined>(() => readDraft<Intent>(key));
  const [instruction, setInstruction] = useState(pending?.instruction ?? '');
  const [busy, setBusy] = useState(false); const inFlight = useRef(false); const [error, setError] = useState<unknown>();
  const available = canContinueResearch(detail, active);
  const send = async () => {
    if (inFlight.current || !pending && (!available || !instruction.trim())) return;
    const intent = pending ?? { submissionId: detail.submission!.id, instruction: instruction.trim() };
    saveDraft(key, intent); setPending(intent); inFlight.current = true; setBusy(true); setError(undefined);
    try {
      await api(`/research/${detail.id}/resume`, { method: 'POST', body: intent });
      removeDraft(key); setPending(undefined); setInstruction(''); changed();
    } catch (failure) {
      setError(failure);
      if (failure instanceof ApiError && failure.status >= 400 && failure.status < 500 && failure.status !== 401) { removeDraft(key); setPending(undefined); }
    } finally { inFlight.current = false; setBusy(false); }
  };
  if (!available && !pending && !error) return null;
  return <section className="research-composer" aria-label={tr("Continue native research")}>
    <Input.TextArea aria-label={tr("Follow-up research instructions")} placeholder={tr("Answer the research assistant's question or add your thoughts")} value={instruction} disabled={!!pending || busy} onChange={event => setInstruction(event.target.value)} />
    <Button onClick={() => { void send(); }} loading={busy} disabled={busy || !pending && (!available || !instruction.trim())}>{pending ? tr("Check original continuation request") : tr("Send · Continue research")}</Button>
    {pending && <DraftNotice saved={draftIsPersisted(key)} />}<ErrorNotice error={error} />
  </section>;
}
function NewStudy({ route }: { route: Route }) {
  useTranslation('core');
  const cache = useQueryClient(); const catalog = useModelCatalog(); const datasets = useResource<DatasetView[]>('/datasets?view=summary'); const settings = useResource<SettingsView>('/settings');
  const [form] = Form.useForm(); const [draft] = useState(() => readDraft<Record<string, unknown>>('research')); const [draftSaved, setDraftSaved] = useState(() => draftIsPersisted('research')); const initialized = useRef(false);
  const listParams = new URLSearchParams(route.params); listParams.delete('from'); listParams.delete('dataset');
  const returnPath = listPathFor({ ...route, params: listParams });
  const command = useCommand<ResearchReceipt>('research', result => { removeDraft('research'); void cache.invalidateQueries({ queryKey: ['/research'] }); if (currentRoute().page === 'research' && currentRoute().creating) navigate(returnPath.replace('/research', `/research/${result.conversationId}`)); });
  const origin = route.params.get('from') === 'data' ? { path: '/settings?section=data', label: tr("Data") } : { path: returnPath, label: tr("Research") };
  const [thinkingChanged, setThinkingChanged] = useState(false);
  const thinkingLevel = Form.useWatch('thinkingLevel', form) as string | undefined;
  const serviceTier = Form.useWatch('serviceTier', form) as ServiceTier | undefined;
  const selectedModelKey = Form.useWatch('model', form) as string | undefined;
  const selectedModel = catalog.data?.models.find(model => modelKey(model) === selectedModelKey);
  const modelOptions = catalog.data?.models.map(model => ({ value: modelKey(model), label: `${model.name} / ${model.provider}` })) ?? [];
  if (selectedModelKey && !modelOptions.some(option => option.value === selectedModelKey)) { const recorded = JSON.parse(selectedModelKey) as ModelRef; modelOptions.push({ value: selectedModelKey, label: `${recorded.provider}/${recorded.modelId} / ${tr("Currently unavailable")}` }); }
  const selectedDatasetId = Form.useWatch('datasetId', form) as string | undefined;
  const selectedDataset = datasets.data?.find(dataset => dataset.id === selectedDatasetId);
  const datasetDetail = useResource<DatasetView>(`/datasets/${encodeURIComponent(selectedDatasetId ?? '')}`, { enabled: !!selectedDataset });
  useEffect(() => {
    if (initialized.current || !catalog.data || !datasets.data || !settings.data) return;
    initialized.current = true;
    if (command.values) { form.setFieldsValue({ ...command.values, publicLiteratureTopics: Array.isArray(command.values.publicLiteratureQueries) ? command.values.publicLiteratureQueries.join('\n') : '', model: modelKey(command.values.model as ModelRef) }); return; }
    const preferred = settings.data.defaultModel && catalog.data.models.find(model => modelKey(model) === modelKey(settings.data!.defaultModel!));
    form.setFieldsValue({ ...(preferred ? { model: modelKey(preferred), thinkingLevel: settings.data.thinkingLevel ?? preferred.effectiveThinkingLevel, serviceTier: !draft?.model || draft.model === modelKey(preferred) ? settings.data.serviceTier ?? undefined : undefined } : {}), ...draft, ...(route.params.get('dataset') ? { datasetId: route.params.get('dataset') } : {}) });
  }, [catalog.data, datasets.data, settings.data]);
  useEffect(() => {
    if (!command.pending && selectedModel && thinkingLevel && !modelSupportsThinking(selectedModel, thinkingLevel as ThinkingLevel)) { setThinkingChanged(true); }
  }, [selectedModel, thinkingLevel, command.pending]);
  useEffect(() => {
    if (!command.pending && selectedModel && serviceTier && !selectedModel.serviceTiers.includes(serviceTier)) { form.setFieldValue('serviceTier', undefined); setDraftSaved(saveDraft('research', form.getFieldsValue())); }
  }, [selectedModel, serviceTier, command.pending]);
  const preview = Form.useWatch([], form) as Record<string, string> | undefined;
  const [specError, setSpecError] = useState<unknown>();
  return <div className="authoring-page"><a className="text-link back-link" href={href(origin.path)}><ArrowLeftIcon size={14} />{origin.label}</a><div className="authoring-heading"><div className="authoring-symbol"><FlaskIcon weight="duotone" size={24} /></div><h1>{tr("New research")}</h1></div>
    <ErrorNotice error={catalog.error ?? datasets.error ?? settings.error} retry={() => { void catalog.refetch(); void datasets.refetch(); void settings.refetch(); }} />
    {catalog.isPending || datasets.isPending || settings.isPending ? <Loading /> : !catalog.data || !datasets.data || !settings.data ? null : <>
    {catalog.data && !catalog.data.models.length && <div className="prerequisite">{tr("No models available")} <a href={href('/settings')}>{tr("Connect a model")} <ArrowUpRightIcon size={13} /></a></div>}
    {datasets.data && !datasets.data.some(item => item.ready) && <div className="prerequisite">{tr("No datasets are ready")} <a href={href('/settings?section=data')}>{tr("View data")} <ArrowUpRightIcon size={13} /></a></div>}
    <PendingCommand command={command} />
    <Form form={form} layout="vertical" validateTrigger="onBlur" scrollToFirstError={{ focus: true }} onValuesChange={(_changed, values: Record<string, unknown>) => { if (!command.pending) setDraftSaved(saveDraft('research', values)); }} onFinish={(values: Record<string, string>) => { try { const input = nativeResearchInput(values); setSpecError(undefined); void command.submit(input); } catch (error) { setSpecError(error); } }}  disabled={!!command.pending || command.busy}>
      <div className="form-section-heading"><FlaskIcon size={16} /><h2>{tr("Research question")}</h2></div>
      <Form.Item label={tr("Research question")} extra={tr("For a parameter grid, specify the strategy, candidate values for each parameter, time windows, and costs. For example: fast periods [8, 16], slow periods [32, 64]. QuaZonai generates combinations, Pi orchestrates them, and Nautilus runs the backtests. Every original result is retained; no optimum is selected automatically.")} name="goal" rules={[{ required: true, whitespace: true, message: tr("Enter a research question") }]}><Input.TextArea autoSize={{ minRows: 2, maxRows: 6 }} placeholder={tr("For example: run a Nautilus strategy on this dataset using the original configuration, and show the native backtest report.")} /></Form.Item>
      <Form.Item label={tr("Public literature search topics (optional)")} name="publicLiteratureTopics" extra={tr("Sent to public search services; up to 3 lines, 500 characters per line")} rules={[{ validator: (_rule, value?: string) => { try { publicLiteratureTopics(value); return Promise.resolve(); } catch { return Promise.reject(new Error(tr("Enter up to 3 public topics, one per line, at most 500 characters each, without control characters"))); } } }]}><Input.TextArea autoSize={{ minRows: 2, maxRows: 5 }} placeholder="market microstructure&#10;momentum transaction costs" /></Form.Item>
      <div className="form-section-heading"><DatabaseIcon size={16} /><h2>{tr("Markets and data")}</h2></div>
      <Form.Item label={tr("Dataset")} name="datasetId" rules={[{ required: true, message: tr("Select a dataset") }, { validator: (_rule, value: string) => !value || datasets.data?.some(item => item.id === value && item.ready) ? Promise.resolve() : Promise.reject(new Error(tr("Dataset is not ready"))) }]}><Select showSearch optionFilterProp="label" suffixIcon={<CaretDownIcon size={14} />} placeholder={tr("Choose a registered dataset")} options={datasets.data?.map(item => ({ value: item.id, label: `${item.nativeModality === 'TRADE_TICK' ? 'TradeTick / ' : ''}${item.name} / ${item.currency}`, disabled: !item.ready }))} /></Form.Item>
      {selectedDataset && <div className="dataset-context"><span><DatabaseIcon size={13} />{selectedDataset.source}</span><code>{selectedDataset.id}</code><code>{selectedDataset.currency}</code>{(selectedDataset.start || selectedDataset.end) && <span><Timestamp value={selectedDataset.start} />  {tr("to")} <Timestamp value={selectedDataset.end} /></span>}<span>{selectedDataset.ready ? tr("Ready") : tr("Not ready")}</span></div>}
      <ErrorNotice error={datasetDetail.error} retry={() => { void datasetDetail.refetch(); }} />
      {selectedDataset && datasetDetail.isPending && <Loading />}
      {datasetDetail.data?.researchSettings?.modality === 'TRADE_TICK' && <NativeInputFacts settings={datasetDetail.data.researchSettings} preparation={datasetDetail.data.preparation} coverage={datasetDetail.data.inputCoverage} />}
      <Form.Item label={tr("Model")} name="model" rules={[{ required: true, message: tr("Select a model") }, { validator: (_rule, value: string) => !value || catalog.data?.models.some(model => modelKey(model) === value) ? Promise.resolve() : Promise.reject(new Error(tr("The selected model is unavailable. Choose another model."))) }]}><Select showSearch optionFilterProp="label" suffixIcon={<CaretDownIcon size={14} />} placeholder={tr("Choose an available model")} options={modelOptions} onChange={value => { const model = catalog.data?.models.find(item => modelKey(item) === value); setThinkingChanged(false); form.setFieldValue('thinkingLevel', settings.data?.defaultModel && modelKey(settings.data.defaultModel) === value ? settings.data.thinkingLevel ?? model?.effectiveThinkingLevel : model?.effectiveThinkingLevel); form.setFieldValue('serviceTier', settings.data?.defaultModel && modelKey(settings.data.defaultModel) === value ? settings.data.serviceTier ?? undefined : undefined); setDraftSaved(saveDraft('research', form.getFieldsValue())); }} /></Form.Item>
      {selectedModel && <p className="model-context"><code>{selectedModel.provider}/{selectedModel.modelId}</code></p>}
      <Form.Item label={tr("Processing tier")} name="serviceTier" extra={selectedModel && !selectedModel.serviceTiers.length ? tr("This model provider does not support tier settings") : undefined}><Select allowClear disabled={!selectedModel?.serviceTiers.length || !!command.pending || command.busy} placeholder={!selectedModel ? tr("Select a model first") : selectedModel.serviceTiers.length ? tr("Use the default tier") : tr("Determined by the model provider")} suffixIcon={<CaretDownIcon size={14} />} options={selectedModel?.serviceTiers.map(value => ({ value, label: value === 'priority' ? 'Fast' : tr("Standard") }))} /></Form.Item>
      <div className="form-columns"><Form.Item label={tr("Reasoning effort")} name="thinkingLevel" extra={[thinkingChanged ? tr("The previous reasoning effort is unavailable. Select another option.") : undefined, reasoningModeDescription(selectedModel, thinkingLevel), thinkingUnavailableReason(selectedModel)].filter(Boolean).join('；') || undefined} rules={[{ required: !!selectedModel, message: tr("Select a reasoning effort") }, { validator: (_rule, value?: string) => !value || !selectedModel || modelSupportsThinking(selectedModel, value as ThinkingLevel) ? Promise.resolve() : Promise.reject(new Error(tr("The previous reasoning effort is unavailable. Select another option."))) }]}><Select onChange={() => setThinkingChanged(false)} suffixIcon={<CaretDownIcon size={14} />} disabled={!selectedModel || !!command.pending || command.busy} options={thinkingOptions(selectedModel)} /></Form.Item></div>
      <details className="submission-preview" open><summary>{tr("Submission preview")}</summary>{reasoningModeDescription(selectedModel, thinkingLevel) && <p>{reasoningModeDescription(selectedModel, thinkingLevel)}</p>}<Facts items={[[tr("Research question"), preview?.goal || tr("Not entered")], [tr("Markets and data"), selectedDataset ? `${selectedDataset.name} / ${selectedDataset.currency}` : tr("Not selected")], [tr("Model and reasoning"), selectedModel ? `${selectedModel.provider}/${selectedModel.modelId} / ${thinkingLevel ? thinkingLabel(thinkingLevel) : tr("Not selected")}` : tr("Not selected")], [tr("Processing tier"), serviceTier === 'priority' ? 'Fast' : serviceTier === 'default' ? tr("Standard") : tr("Default tier")]]} /></details>
      <ErrorNotice error={specError ?? command.error} /><DraftNotice saved={draftSaved || !!command.pending} /><div className="form-actions"><Button disabled={false} onClick={() => navigate(origin.path)}>{tr("Cancel")}</Button>{command.pending ? <Button type="primary" disabled={command.busy} aria-busy={command.busy} loading={command.busy} onClick={() => { void command.submit(); }}>{command.busy ? tr("Submitting") : tr("Check submission")}</Button> : <Button type="primary" htmlType="submit" loading={command.busy} disabled={!catalog.data?.models.length || !datasets.data?.some(item => item.ready)}>{tr("Start research")} <ArrowRightIcon size={15} /></Button>}</div>
    </Form></>}
  </div>;
}
