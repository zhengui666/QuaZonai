import { Button } from '@radix-ui/themes';
import type { ColumnDef } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';
import { EChart } from '../components/charts/EChart';
import { FinancialSeriesChart } from '../components/charts/FinancialSeriesChart';
import { DataTable } from '../components/ui/DataTable';
import { EmptyState } from '../components/ui/EmptyState';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { ForwardArrowIcon } from '../components/ui/ForwardArrowIcon';
import { PageHeader } from '../components/ui/PageHeader';
import { PageSkeleton } from '../components/ui/Skeleton';
import { StateBadge } from '../components/ui/StateBadge';
import { Section } from '../components/ui/Section';
import { Translated, useI18n } from '../i18n';
import { useCandidates, useMandates, usePortfolioPrograms } from '../lib/api/hooks';
import type { PortfolioProgram } from '../lib/api/types';
import { formatDateTime, formatNumber } from '../lib/format';
import { findMatrix, findNamedValues, findTimeSeries } from '../lib/metrics';

const programColumns: ColumnDef<PortfolioProgram, unknown>[] = [
  { accessorKey: 'mandate_name', header: 'Mandate', meta: { mobile: { placement: 'title' } }, cell: ({ row }) => <div><div className="qz-list-title">{row.original.mandate_name ? <bdi dir="auto">{row.original.mandate_name}</bdi> : <bdi dir="ltr">{row.original.mandate_version_id.slice(0, 8)}</bdi>}</div><div className="qz-list-subtitle qz-mono"><bdi dir="ltr">{row.original.id}</bdi></div></div> },
  { accessorKey: 'state', header: 'State', meta: { localizedSort: true, mobile: { placement: 'badge' } }, cell: ({ getValue }) => <StateBadge state={String(getValue())} /> },
  { accessorKey: 'candidate_count', header: 'Candidates', cell: ({ getValue }) => <span className="qz-number">{formatNumber(getValue() as number | undefined)}</span> },
  { accessorKey: 'updated_at', header: 'Updated', cell: ({ getValue }) => formatDateTime(getValue() as string | undefined) },
  { id: 'candidate', header: '', meta: { mobile: { placement: 'action' } }, cell: ({ row }) => row.original.current_candidate_id ? <Button asChild size="1" variant="ghost"><Link to={`/portfolio/candidates/${row.original.current_candidate_id}`}><Translated source="Candidate" /> <ForwardArrowIcon size={12} /></Link></Button> : <span className="qz-section-meta"><Translated source="Researching" /></span> },
];

export function PortfolioLabPage() {
  const { t } = useI18n();
  const mandates = useMandates();
  const programs = usePortfolioPrograms();
  const candidateIds = (programs.data ?? []).flatMap((item) => item.current_candidate_id ? [item.current_candidate_id] : []);
  const candidates = useCandidates(candidateIds);
  const current = candidates.find((query) => query.data)?.data;
  const allocation = useMemo(() => current?.members?.map((member) => ({ name: member.alpha_name ?? member.alpha_qualification_id.slice(0, 8), value: member.target_weight ?? member.target_contribution ?? 0 })) ?? [], [current?.members]);
  const risk = findNamedValues(current?.metrics, ['risk_exposure', 'factor_exposure', 'universe_exposure']);
  const matrix = findMatrix(current?.metrics, ['correlation_matrix', 'correlation']);
  const equity = findTimeSeries(current?.metrics, ['equity_curve', 'performance', 'portfolio_equity']);
  const benchmark = findTimeSeries(current?.metrics, ['benchmark_curve', 'benchmark']);
  const allocationOption = useMemo(() => ({ grid: { left: 130, right: 20, top: 10, bottom: 24 }, xAxis: { type: 'value' }, yAxis: { type: 'category', data: allocation.map((item) => item.name) }, tooltip: { trigger: 'axis' }, series: [{ type: 'bar', data: allocation.map((item) => item.value), itemStyle: { color: '#4f9b82' } }] }), [allocation]);
  const riskOption = useMemo(() => ({ grid: { left: 110, right: 20, top: 10, bottom: 24 }, xAxis: { type: 'value' }, yAxis: { type: 'category', data: risk.map((item) => item.name) }, tooltip: { trigger: 'axis' }, series: [{ type: 'bar', data: risk.map((item) => item.value), itemStyle: { color: '#4f9b82' } }] }), [risk]);
  const correlationOption = useMemo(() => {
    const data = matrix ? matrix.values.flatMap((row, y) => row.map((value, x) => [x, y, value])) : [];
    return { grid: { left: 80, right: 32, top: 16, bottom: 52 }, tooltip: {}, xAxis: { type: 'category', data: matrix?.labels ?? [] }, yAxis: { type: 'category', data: matrix?.labels ?? [] }, visualMap: { min: -1, max: 1, calculable: false, orient: 'horizontal', left: 'center', bottom: 0 }, series: [{ type: 'heatmap', data }] };
  }, [matrix]);

  if (mandates.isLoading || programs.isLoading) return <PageSkeleton />;
  if (mandates.error || programs.error) return <ErrorPanel error={mandates.error ?? programs.error} />;

  return (
    <>
      <PageHeader title="Portfolio Lab" description="Qualified Alpha assets are assembled under immutable Mandates and current Capital Context. The workbench visualizes allocation, risk and evidence but never exposes manual weighting or trading controls." />
      <Section title="Mandates" meta="Capital objectives are versioned separately from Alpha research"><div className="qz-grid-3">{(mandates.data ?? []).map((mandate) => {
        const configured = mandate.configuration_state === 'V1_CONFIGURED';
        return <div className="qz-panel qz-panel-pad" key={mandate.id}><div style={{ display: 'flex', justifyContent: 'space-between', gap: 10 }}><strong dir="auto" style={{ fontSize: 13 }}>{mandate.name}</strong><StateBadge state={configured ? (mandate.enabled ? 'ENABLED' : 'DISABLED') : 'LEGACY_UNAVAILABLE'} /></div><div className="qz-list-subtitle" dir="auto" style={{ marginTop: 8 }}>{configured ? mandate.latest_version?.objective ?? t('portfolio.versionedMandate') : 'Legacy configuration unavailable'}</div><div className="qz-section-meta qz-mono" dir="auto" style={{ marginTop: 12 }}>{configured ? mandate.latest_version?.id.slice(0, 12) ?? mandate.id.slice(0, 12) : mandate.id.slice(0, 12)}</div></div>;
      })}</div></Section>
      <Section title="Portfolio Programs" meta="Automatically created when qualified assets create a real assembly opportunity"><DataTable data={programs.data ?? []} columns={programColumns} searchPlaceholder="Filter portfolio programs…" emptyTitle="No portfolio programs" emptyDescription="Programs appear after a Mandate is enabled and qualified Alpha assets support portfolio research." getRowId={(row) => row.id} /></Section>
      <div className="qz-grid-2">
        <Section title="Portfolio equity" meta="Lightweight Charts · latest API candidate">{equity.length ? <div className="qz-panel qz-panel-pad"><FinancialSeriesChart ariaLabel="Portfolio equity and benchmark chart" series={[{ name: 'Portfolio', data: equity, kind: 'area' }, { name: 'Benchmark', data: benchmark }]} /></div> : <EmptyState title="No equity curve" description="The latest candidate has not returned a portfolio performance series." />}</Section>
        <Section title="Alpha allocation" meta="Read-only candidate composition">{allocation.length ? <div className="qz-panel qz-panel-pad"><EChart ariaLabel="Portfolio alpha allocation chart" option={allocationOption} /></div> : <EmptyState title="No allocation vector" description="No current candidate member weights or contribution targets were returned." />}</Section>
      </div>
      <div className="qz-grid-2">
        <Section title="Risk exposure" meta="Factor / universe exposure">{risk.length ? <div className="qz-panel qz-panel-pad"><EChart ariaLabel="Portfolio risk exposure chart" option={riskOption} /></div> : <EmptyState title="No exposure vector" description="Risk exposure remains unavailable until returned by the candidate evidence API." />}</Section>
        <Section title="Correlation matrix" meta="Cross-alpha / cross-universe dependence">{matrix ? <div className="qz-panel qz-panel-pad"><EChart ariaLabel="Portfolio correlation matrix" option={correlationOption} /></div> : <EmptyState title="No correlation matrix" description="The current candidate did not return matrix evidence." />}</Section>
      </div>
    </>
  );
}
