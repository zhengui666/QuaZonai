import { ArrowRightIcon, FlaskIcon, TargetIcon } from '@phosphor-icons/react';
import { Button } from '@radix-ui/themes';
import { Link } from 'react-router-dom';
import { ResearchPulseChart } from '../components/charts/ResearchPulseChart';
import { EmptyState } from '../components/ui/EmptyState';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { KpiStrip } from '../components/ui/KpiStrip';
import { PageHeader } from '../components/ui/PageHeader';
import { PageSkeleton } from '../components/ui/Skeleton';
import { StateBadge } from '../components/ui/StateBadge';
import { Section } from '../components/ui/Section';
import { useAlphaLibrary, useApprovals, useHandoffs, useHealth, usePortfolioPrograms, useProgramMissionMatrix, usePrograms, useReadiness } from '../lib/api/hooks';
import { formatDateTime, humanize } from '../lib/format';
import { useEventStream } from '../lib/useEventStream';

function ready(value: unknown) { return typeof value === 'boolean' ? value : Boolean((value as { ready?: boolean } | undefined)?.ready); }
function healthState(value: unknown) {
  if (typeof value === 'boolean') return value ? 'READY' : 'NOT_READY';
  if (value && typeof value === 'object') {
    const item = value as Record<string, unknown>;
    if (typeof item.state === 'string') return item.state;
    if (typeof item.ready === 'boolean') return item.ready ? 'READY' : 'NOT_READY';
    if (typeof item.status === 'string') return item.status;
  }
  return value ? 'READY' : 'UNKNOWN';
}

export function HomePage() {
  const programs = usePrograms();
  const approvals = useApprovals();
  const readiness = useReadiness();
  const health = useHealth();
  const alphas = useAlphaLibrary();
  const handoffs = useHandoffs();
  const portfolio = usePortfolioPrograms();
  const programIds = (programs.data ?? []).filter((item) => !['ARCHIVED', 'PAUSED'].includes(item.state)).map((item) => item.id);
  const missionQueries = useProgramMissionMatrix(programIds);
  const { events, connected } = useEventStream();
  const baseQueries = [programs, approvals, readiness, health, alphas, handoffs, portfolio];

  if (baseQueries.some((query) => query.isLoading)) return <PageSkeleton />;
  const error = baseQueries.find((query) => query.error)?.error;
  if (error) return <ErrorPanel error={error} />;

  const items = programs.data ?? [];
  const missions = missionQueries.flatMap((query) => query.data ?? []);
  const pending = (approvals.data ?? []).filter((item) => item.state === 'PENDING');
  const active = items.filter((item) => item.state === 'ACTIVE').length;
  const cooling = items.filter((item) => item.state === 'COOLING').length;
  const blocked = items.filter((item) => item.state === 'BLOCKED').length;
  const runningMissions = missions.filter((mission) => mission.state === 'RUNNING').length;
  const discoveryMissions = missions.filter((mission) => mission.state === 'RUNNING' && /ALPHA|DISCOVERY/i.test(mission.type)).length;
  const evaluationMissions = missions.filter((mission) => /EVAL|VALIDAT|SEALED|REVIEW/i.test(mission.type));
  const evaluationRunning = evaluationMissions.filter((mission) => mission.state === 'RUNNING').length;
  const candidateReady = (portfolio.data ?? []).filter((item) => /CANDIDATE|READY/i.test(item.state)).length;
  const availableHandoffs = (handoffs.data ?? []).filter((item) => item.state === 'AVAILABLE').length;
  const pulse = [
    { label: 'Programs', active, cooling, evidence: events.filter((event) => /EVALUAT|EVIDENCE|QUALIF/i.test(event.kind)).length },
    { label: 'Missions', active: runningMissions, cooling: blocked, evidence: missions.filter((mission) => mission.state === 'SUCCEEDED').length },
    { label: 'Evidence', active: (alphas.data ?? []).length, cooling: evaluationRunning, evidence: events.length },
  ];

  return (
    <>
      <PageHeader
        title="Research command center"
        description="Autonomous research, independent evaluation, portfolio construction and handoff readiness in one operational cockpit. Human attention stays concentrated on ideas and immutable candidate decisions."
        actions={<><Button asChild size="2"><Link to="/ideas"><FlaskIcon size={15} />Propose idea</Link></Button><Button asChild size="2" variant="soft"><Link to="/approval"><TargetIcon size={15} />Review approvals</Link></Button></>}
      />
      <KpiStrip items={[
        { label: 'Active programs', value: active, note: `${cooling} cooling · ${blocked} blocked` },
        { label: 'Running missions', value: runningMissions, note: `${discoveryMissions} alpha discovery` },
        { label: 'Alpha library', value: (alphas.data ?? []).length, note: `${evaluationRunning} evaluations running` },
        { label: 'Evaluation status', value: evaluationRunning ? 'RUNNING' : evaluationMissions.length ? 'CURRENT' : 'IDLE', note: `${evaluationMissions.length} evaluation missions observed` },
      ]} />
      <div className="qz-split" style={{ marginTop: 20 }}>
        <Section title="Research pulse" meta="Material progress, not token or command counts"><div className="qz-panel qz-panel-pad"><ResearchPulseChart data={pulse} /></div></Section>
        <Section title="Action center" meta={`${pending.length} decision${pending.length === 1 ? '' : 's'}`}>
          {pending.length ? <div className="qz-panel qz-panel-pad qz-list">{pending.slice(0, 5).map((approval) => <div className="qz-list-row" key={approval.id}><div className="qz-list-main"><div className="qz-list-title">{approval.purpose} candidate · {approval.candidate?.mandate_name ?? approval.candidate_id.slice(0, 8)}</div><div className="qz-list-subtitle">Valid until {formatDateTime(approval.valid_until ?? approval.expires_at)}</div></div><Button asChild size="1" variant="ghost"><Link to="/approval">Review <ArrowRightIcon size={12} /></Link></Button></div>)}</div> : <EmptyState title="No decisions waiting" description="Research continues autonomously. You will only be interrupted by a material candidate or required administration." />}
        </Section>
      </div>
      <Section title="Portfolio readiness" meta="Construction and handoff pipeline">
        <KpiStrip items={[
          { label: 'Candidates ready', value: candidateReady, note: `${(portfolio.data ?? []).length} portfolio programs` },
          { label: 'Approval pending', value: pending.length, note: pending.length ? 'Human decision required' : 'No approval queue' },
          { label: 'Handoff available', value: availableHandoffs, note: `${(handoffs.data ?? []).filter((item) => item.state === 'CLAIMED').length} claimed` },
          { label: 'Paper readiness', value: ready(readiness.data?.PAPER_HANDOFF_READY) ? 'READY' : 'NOT READY', note: ready(readiness.data?.LIVE_HANDOFF_READY) ? 'Live also ready' : 'Live independently gated' },
        ]} />
      </Section>
      <Section title="System health" meta="Backend-authoritative readiness">
        <div className="qz-grid-3">
          <div className="qz-panel qz-panel-pad"><div className="qz-label">Agent worker heartbeat</div><div style={{ marginTop: 8 }}><StateBadge state={healthState(health.data?.agent_worker)} /></div><div className="qz-list-subtitle">Mission process and Codex child lifecycle</div></div>
          <div className="qz-panel qz-panel-pad"><div className="qz-label">Codex readiness</div><div style={{ marginTop: 8 }}><StateBadge state={healthState(health.data?.codex)} /></div><div className="qz-list-subtitle">App Server authentication and runtime availability</div></div>
          <div className="qz-panel qz-panel-pad"><div className="qz-label">Data readiness</div><div style={{ marginTop: 8 }}><StateBadge state={ready(readiness.data?.RESEARCH_READY) ? 'READY' : healthState(health.data?.data)} /></div><div className="qz-list-subtitle">Governed Discovery data required for research</div></div>
        </div>
      </Section>
      <Section title="Recent material events" meta={connected ? 'Live SSE connection' : 'Reconnecting automatically'}>
        {events.length ? <div className="qz-panel qz-panel-pad qz-timeline">{events.slice(0, 10).map((event, index) => <div className="qz-timeline-item" data-active={index === 0} key={String(event.id)}><div className="qz-timeline-title">{humanize(event.kind)}</div><div className="qz-timeline-meta qz-number">{formatDateTime(event.created_at)}</div>{event.aggregate_type ? <div className="qz-timeline-body">{event.aggregate_type} {event.aggregate_id?.slice(0, 8)}</div> : null}</div>)}</div> : <EmptyState title="Waiting for material events" description="The live stream omits raw market traffic and low-value agent chatter." />}
      </Section>
    </>
  );
}
