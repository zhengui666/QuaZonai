import { Button } from '@radix-ui/themes';
import type { ColumnDef } from '@tanstack/react-table';
import { FinancialSeriesChart } from '../components/charts/FinancialSeriesChart';
import { DataTable } from '../components/ui/DataTable';
import { EmptyState } from '../components/ui/EmptyState';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { KpiStrip } from '../components/ui/KpiStrip';
import { PageHeader } from '../components/ui/PageHeader';
import { PageSkeleton } from '../components/ui/Skeleton';
import { StateBadge } from '../components/ui/StateBadge';
import { Section } from '../components/ui/Section';
import { Translated, useI18n } from '../i18n';
import { useHandoffs, useRevokeHandoff } from '../lib/api/hooks';
import type { HandoffOffer } from '../lib/api/types';
import { formatDateTime } from '../lib/format';
import { findTimeSeries } from '../lib/metrics';

type HandoffWithEvidence = HandoffOffer & { forward_evidence?: Record<string, unknown>; evidence_summary?: Record<string, unknown> };

function Revoke({ offer }: { offer: HandoffOffer }) {
  const { t } = useI18n();
  const mutation = useRevokeHandoff(offer.id);
  const revocable = ['APPROVED', 'PUBLISHING', 'AVAILABLE'].includes(offer.state);
  return revocable ? <Button size="1" variant="soft" color="red" disabled={mutation.isPending} onClick={() => mutation.mutate('OPERATOR_REVOKE')}>{mutation.isPending ? t('common.revoking') : t('handoff.revoke')}</Button> : <span className="qz-section-meta">{['CLAIMED', 'DOWNSTREAM_ACCEPTED', 'FEEDBACK_PENDING', 'FEEDBACK_IN_PROGRESS', 'FEEDBACK_PARTIAL', 'FEEDBACK_COMPLETE'].includes(offer.state) ? t('handoff.downstreamOwns') : t('common.historical')}</span>;
}

const columns: ColumnDef<HandoffOffer, unknown>[] = [
  { accessorKey: 'downstream_name', header: 'Downstream', cell: ({ row }) => <div><div className="qz-list-title">{row.original.downstream_name ?? row.original.downstream_system_id?.slice(0, 8) ?? '—'}</div><div className="qz-list-subtitle">{row.original.purpose ?? '—'} · <Translated source="Candidate" /> {row.original.candidate_id?.slice(0, 8) ?? '—'}</div></div> },
  { accessorKey: 'state', header: 'Package / offer', cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'feedback_state', header: 'Forward evidence', cell: ({ getValue }) => <StateBadge state={String(getValue() ?? 'PENDING')} /> },
  { accessorKey: 'claim_deadline', header: 'Claim deadline', cell: ({ getValue }) => formatDateTime(getValue() as string | null) },
  { accessorKey: 'package_contract_version', header: 'Package contract' },
  { accessorKey: 'feedback_contract_version', header: 'Feedback contract' },
  { id: 'action', header: '', cell: ({ row }) => <Revoke offer={row.original} />, enableSorting: false },
];

export function HandoffFeedbackPage() {
  const { t } = useI18n();
  const query = useHandoffs();
  if (query.isLoading) return <PageSkeleton />;
  if (query.error) return <ErrorPanel error={query.error} />;
  const handoffs = query.data ?? [];
  const evidenceCarrier = (handoffs as HandoffWithEvidence[]).find((item) => item.forward_evidence || item.evidence_summary);
  const evidence = evidenceCarrier?.forward_evidence ?? evidenceCarrier?.evidence_summary;
  const performance = findTimeSeries(evidence, ['performance', 'equity_curve', 'forward_performance', 'portfolio']);
  const benchmark = findTimeSeries(evidence, ['benchmark', 'benchmark_curve']);
  const drawdown = findTimeSeries(evidence, ['drawdown', 'drawdown_curve']);

  return (
    <>
      <PageHeader title="Handoff Center" description="Immutable Candidate Packages move through Available, Claimed, Accepted and Forward Evidence states. Once claimed, QuaZonai deliberately exposes no runtime, order, position or stop control." />
      <KpiStrip items={[
        { label: 'Available packages', value: handoffs.filter((item) => item.state === 'AVAILABLE').length },
        { label: 'Claimed', value: handoffs.filter((item) => item.state === 'CLAIMED').length },
        { label: 'Accepted', value: handoffs.filter((item) => item.state === 'DOWNSTREAM_ACCEPTED').length },
        { label: 'Feedback complete', value: handoffs.filter((item) => item.state === 'FEEDBACK_COMPLETE' || item.feedback_state === 'FEEDBACK_COMPLETE').length },
      ]} />
      <div style={{ marginTop: 20 }}><DataTable data={handoffs} columns={columns} searchPlaceholder="Filter handoffs…" emptyTitle="No handoffs" emptyDescription="Approved candidates appear here after their Candidate Package is published." getRowId={(row) => row.id} ariaLabel={t('nav.handoff')} /></div>
      <Section title="Forward Evidence" meta={evidenceCarrier ? <span>{t('common.candidate')} <bdi dir="ltr">{evidenceCarrier.candidate_id?.slice(0, 8) ?? '—'}</bdi> <bdi dir="ltr">· API ·</bdi> {t('alpha.evidence')}</span> : t('handoff.awaitingFeedback')}>
        {performance.length || drawdown.length ? <div className="qz-panel qz-panel-pad"><FinancialSeriesChart ariaLabel="Forward evidence performance and drawdown chart" series={[{ name: 'Forward performance', data: performance, kind: 'area' }, { name: 'Benchmark', data: benchmark }, { name: 'Drawdown', data: drawdown }]} /></div> : <EmptyState title="No forward evidence series" description="Forward evidence is only charted when the Handoff API returns contract-valid time-series observations. No synthetic performance is generated." />}
      </Section>
    </>
  );
}
