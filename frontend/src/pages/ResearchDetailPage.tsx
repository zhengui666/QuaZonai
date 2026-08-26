import { ArchiveIcon, PauseIcon, PlayIcon, RewindIcon } from '@phosphor-icons/react';
import { Button, Dialog, TextArea } from '@radix-ui/themes';
import type { ColumnDef } from '@tanstack/react-table';
import { useState, type ReactNode } from 'react';
import { useParams } from 'react-router-dom';
import { CandlestickChart } from '../components/charts/CandlestickChart';
import { MissionGraph } from '../components/graphs/MissionGraph';
import { DataTable } from '../components/ui/DataTable';
import { EmptyState } from '../components/ui/EmptyState';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { KpiStrip } from '../components/ui/KpiStrip';
import { PageHeader } from '../components/ui/PageHeader';
import { PageSkeleton } from '../components/ui/Skeleton';
import { StateBadge } from '../components/ui/StateBadge';
import { Section } from '../components/ui/Section';
import { Translated, useI18n } from '../i18n';
import { failedMissionForms, runningMissionForms, structuredEventForms, succeededMissionForms } from '../i18n/researchPlural';
import { useProgram, useProgramAction, useProgramActivity, useProgramMissions } from '../lib/api/hooks';
import type { ActivityEvent, OhlcPoint } from '../lib/api/types';
import { formatDateTime, formatNumber, humanize, localizeSystemInferred } from '../lib/format';

type BranchSummary = { id: string; missions: number; running: number; succeeded: number; failed: number };
const branchColumns: ColumnDef<BranchSummary, unknown>[] = [
  { accessorKey: 'id', header: 'Branch', meta: { messageKey: 'research.branch' }, cell: ({ getValue }) => <span className="qz-mono">{String(getValue()).slice(0, 16)}</span> },
  { accessorKey: 'missions', header: 'Missions', meta: { messageKey: 'research.missions' }, cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number)}</span> },
  { accessorKey: 'running', header: 'Running', meta: { messageKey: 'research.running' }, cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number)}</span> },
  { accessorKey: 'succeeded', header: 'Succeeded', meta: { messageKey: 'research.succeeded' }, cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number)}</span> },
  { accessorKey: 'failed', header: 'Failed', meta: { messageKey: 'research.failed' }, cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number)}</span> },
];
const evidenceColumns: ColumnDef<ActivityEvent, unknown>[] = [
  { accessorKey: 'kind', header: 'Event', meta: { messageKey: 'research.event', localizedSort: true }, cell: ({ getValue }) => humanize(String(getValue())) },
  { accessorKey: 'mission_id', header: 'Mission', meta: { messageKey: 'research.mission' }, cell: ({ getValue }) => <span className="qz-mono">{String(getValue() ?? '—').slice(0, 12)}</span> },
  { accessorKey: 'created_at', header: 'Observed', meta: { messageKey: 'research.observed' }, cell: ({ getValue }) => formatDateTime(getValue() as string) },
  { id: 'summary', header: 'Evidence / result', meta: { messageKey: 'research.evidenceResult' }, cell: ({ row }) => <span className="qz-list-subtitle" dir="auto" style={{ whiteSpace: 'normal' }}>{eventSummary(row.original)}</span> },
];

function eventSummary(event: ActivityEvent): ReactNode {
  const value = event.payload?.summary ?? event.payload?.result ?? event.payload?.classification ?? event.payload?.evidence;
  if (value === undefined || value === null) return <Translated source="Structured event recorded." />;
  return typeof value === 'string' ? <bdi dir="auto">{value}</bdi> : <bdi dir="ltr">{JSON.stringify(value)}</bdi>;
}

function extractOhlc(events: Array<{ payload?: Record<string, unknown> }>): OhlcPoint[] {
  for (const event of events) {
    const series = event.payload?.ohlc;
    if (Array.isArray(series)) {
      const valid = series.filter((item): item is OhlcPoint => Boolean(item && typeof item === 'object' && 'open' in item && 'close' in item)) as OhlcPoint[];
      if (valid.length) return valid;
    }
  }
  return [];
}

function ProgramActionDialog({ id, action, label, icon }: { id: string; action: 'pause' | 'resume' | 'archive' | 'restore'; label: string; icon: ReactNode }) {
  const { t, text } = useI18n();
  const mutation = useProgramAction(id, action);
  const [reason, setReason] = useState('');
  const needsReason = action === 'pause' || action === 'archive';
  const localizedLabel = text(label);
  return <Dialog.Root><Dialog.Trigger><Button size="1" variant="soft">{icon}{localizedLabel}</Button></Dialog.Trigger><Dialog.Content maxWidth="440px"><Dialog.Title>{t('research.actionTitle', { action: localizedLabel })}</Dialog.Title><Dialog.Description size="2" mb="4">{t('research.actionDesc')}</Dialog.Description>{needsReason ? <TextArea dir="auto" placeholder={t('research.reasonPlaceholder')} value={reason} onChange={(event) => setReason(event.target.value)} /> : null}<div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 18 }}><Dialog.Close><Button variant="soft" color="gray">{t('common.cancel')}</Button></Dialog.Close><Button disabled={mutation.isPending || (needsReason && !reason.trim())} onClick={() => mutation.mutate(needsReason ? reason.trim() : undefined)}>{mutation.isPending ? t('common.applying') : localizedLabel}</Button></div></Dialog.Content></Dialog.Root>;
}

export function ResearchDetailPage() {
  const { t, plural } = useI18n();
  const systemInferred = t('common.systemInferred');
  const { id } = useParams();
  const program = useProgram(id);
  const missions = useProgramMissions(id);
  const activity = useProgramActivity(id);
  if (program.isLoading || missions.isLoading || activity.isLoading) return <PageSkeleton />;
  if (program.error || missions.error || activity.error) return <ErrorPanel error={program.error ?? missions.error ?? activity.error} />;
  if (!program.data || !id) return <EmptyState title="Program not found" description="The requested research program no longer exists or is unavailable." />;

  const current = program.data;
  const missionList = missions.data ?? [];
  const events = activity.data ?? [];
  const ohlc = extractOhlc(events);
  const running = missionList.filter((mission) => mission.state === 'RUNNING').length;
  const succeeded = missionList.filter((mission) => mission.state === 'SUCCEEDED').length;
  const failed = missionList.filter((mission) => mission.state === 'FAILED').length;
  const branchMap = new Map<string, BranchSummary>();
  for (const mission of missionList) {
    const branchId = mission.branch_id ?? 'unassigned';
    const branch = branchMap.get(branchId) ?? { id: branchId, missions: 0, running: 0, succeeded: 0, failed: 0 };
    branch.missions += 1;
    if (mission.state === 'RUNNING') branch.running += 1;
    if (mission.state === 'SUCCEEDED') branch.succeeded += 1;
    if (mission.state === 'FAILED') branch.failed += 1;
    branchMap.set(branchId, branch);
  }
  const branchRows = [...branchMap.values()];
  const evidenceEvents = events.filter((event) => /EXPERIMENT|EVALUAT|EVIDENCE|SEARCH|DISCLOS|QUALIF|CALIBR|PROMOT/i.test(event.kind));
  const headerTitle = current.title ?? current.charter?.research_question ?? `${t('research.program')} \u2066${current.id.slice(0, 8)}\u2069`;
  const headerDescription = current.charter?.original_idea_text ?? t('research.autonomousProgram');
  const programReason = current.cooling_reason ?? current.blocked_reason ?? current.wake_reason;
  const missionSummary = [
    plural(runningMissionForms, running),
    plural(succeededMissionForms, succeeded),
    plural(failedMissionForms, failed),
  ].join(' · ');

  return (
    <>
      <PageHeader
        title={headerTitle}
        description={headerDescription}
        translateTitle={false}
        translateDescription={false}
        actions={<>{current.state === 'ACTIVE' ? <ProgramActionDialog id={id} action="pause" label="Pause" icon={<PauseIcon size={14} />} /> : current.state === 'PAUSED' ? <ProgramActionDialog id={id} action="resume" label="Resume" icon={<PlayIcon size={14} />} /> : null}{current.state !== 'ARCHIVED' ? <ProgramActionDialog id={id} action="archive" label="Archive" icon={<ArchiveIcon size={14} />} /> : <ProgramActionDialog id={id} action="restore" label="Restore" icon={<RewindIcon size={14} />} />}</>}
      />
      <KpiStrip items={[
        { label: 'Program state', value: <StateBadge state={current.state} />, note: programReason ? <span dir="auto">{programReason}</span> : t('research.autonomousScheduling') },
        { label: 'Missions', value: missionList.length, note: missionSummary },
        { label: 'Branches', value: current.branch_count ?? branchRows.length, note: t('research.lineagePaths') },
        { label: 'Alphas', value: current.alpha_count ?? '—', note: t('research.qualifiedOrEval') },
      ]} />
      <Section title="Frozen research charter" meta={t('research.created', { date: formatDateTime(current.charter?.created_at ?? current.created_at) })}>
        <div className="qz-panel qz-panel-pad qz-grid-2">
          <div><div className="qz-label">{t('idea.researchQuestion')}</div><div dir="auto" style={{ fontSize: 13, marginTop: 5 }}>{current.charter?.research_question ?? '—'}</div></div>
          <div><div className="qz-label">{t('idea.predictionHorizon')}</div><div className="qz-list-subtitle" dir="auto">{localizeSystemInferred(current.charter?.prediction_horizon, systemInferred) ?? '—'}</div></div>
          <div><div className="qz-label">{t('idea.marketScope')}</div><div className="qz-list-subtitle">{Array.isArray(current.charter?.market_scope) ? current.charter.market_scope.map((scope, index) => <span key={`${scope}-${index}`}>{index ? ', ' : null}<bdi dir="auto">{localizeSystemInferred(scope, systemInferred)}</bdi></span>) : <bdi dir="auto">{localizeSystemInferred(current.charter?.market_scope, systemInferred) ?? '—'}</bdi>}</div></div>
          <div><div className="qz-label">{t('research.explicitExclusions')}</div><div className="qz-list-subtitle">{current.charter?.explicit_exclusions?.length ? current.charter.explicit_exclusions.map((exclusion, index) => <span key={`${exclusion}-${index}`}>{index ? ', ' : null}<bdi dir="auto">{exclusion}</bdi></span>) : t('common.none')}</div></div>
        </div>
      </Section>
      <div className="qz-grid-2">
        <Section title="Research branches" meta="Derived from Mission lineage returned by the API">{branchRows.length ? <DataTable data={branchRows} columns={branchColumns} ariaLabel={t('research.researchBranches')} initialPageSize={20} enableVirtualization={false} /> : <EmptyState title="No branches yet" description="Branch lineage appears after the research program schedules scoped Missions." />}</Section>
        <Section title="Mission DAG" meta="React Flow · read-only dependencies">{missionList.length ? <MissionGraph missions={missionList} /> : <EmptyState title="No Missions scheduled" description="The program may be cooling, paused, or waiting for a data capability." />}</Section>
      </div>
      {ohlc.length ? <Section title="Market context" meta="TradingView Lightweight Charts · evidence context only"><div className="qz-panel qz-panel-pad"><CandlestickChart data={ohlc} /></div></Section> : null}
      <Section title="Experiment & evidence ledger" meta="Independent evaluation, Search Ledger and exposure-related domain events">
        {evidenceEvents.length ? <DataTable data={evidenceEvents} columns={evidenceColumns} ariaLabel={t('research.evidenceLedger')} initialPageSize={20} getRowId={(event) => String(event.id)} /> : <EmptyState title="No experiment evidence yet" description="Structured experiment, evaluation and evidence events will appear here when returned by the Program activity API." />}
      </Section>
      <Section title="Agent activity" meta={plural(structuredEventForms, events.length)}>
        {events.length ? <div className="qz-panel qz-panel-pad qz-timeline">{events.slice(0, 40).map((event, index) => <div key={String(event.id)} className="qz-timeline-item" data-active={index === 0}><div className="qz-timeline-title"><bdi dir="auto">{humanize(event.kind)}</bdi></div><div className="qz-timeline-meta qz-number">{formatDateTime(event.created_at)}{event.mission_id ? <> · <bdi dir="ltr">{event.mission_id.slice(0, 8)}</bdi></> : null}</div>{event.payload?.summary ? <div className="qz-timeline-body" dir="auto">{eventSummary(event)}</div> : null}</div>)}</div> : <EmptyState title="No activity yet" description="Agent commands, test exits, Domain events and material evidence appear here without exposing hidden chain-of-thought." />}
      </Section>
    </>
  );
}
