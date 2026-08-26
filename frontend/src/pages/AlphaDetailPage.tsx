import { Background, Controls, ReactFlow, type Edge, type Node } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { useMemo } from 'react';
import { useParams } from 'react-router-dom';
import { useReactFlowAriaLabelConfig } from '../components/graphs/reactFlowA11y';
import { EChart } from '../components/charts/EChart';
import { FinancialSeriesChart } from '../components/charts/FinancialSeriesChart';
import { EmptyState } from '../components/ui/EmptyState';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { KpiStrip } from '../components/ui/KpiStrip';
import { PageHeader } from '../components/ui/PageHeader';
import { PageSkeleton } from '../components/ui/Skeleton';
import { StateBadge } from '../components/ui/StateBadge';
import { Section } from '../components/ui/Section';
import { useI18n } from '../i18n';
import { useAlpha } from '../lib/api/hooks';
import { humanize } from '../lib/format';
import { findCalibration, findNamedValues, findTimeSeries } from '../lib/metrics';

export function AlphaDetailPage() {
  const { t } = useI18n();
  const ariaLabelConfig = useReactFlowAriaLabelConfig();
  const { id } = useParams();
  const query = useAlpha(id);
  const alpha = query.data;
  const performance = findTimeSeries(alpha?.metrics, ['performance', 'equity_curve', 'cumulative_return', 'performance_curve']);
  const benchmark = findTimeSeries(alpha?.metrics, ['benchmark', 'benchmark_curve']);
  const drawdown = findTimeSeries(alpha?.metrics, ['drawdown', 'drawdown_curve']);
  const degradation = findTimeSeries(alpha?.metrics, ['degradation', 'health_series', 'predictive_decay']);
  const calibration = findCalibration(alpha?.metrics, ['calibration', 'calibration_curve']);
  const importance = findNamedValues(alpha?.metrics, ['feature_importance', 'features', 'importance']);
  const importanceOption = useMemo(() => ({ grid: { left: 130, right: 20, top: 10, bottom: 24 }, xAxis: { type: 'value' }, yAxis: { type: 'category', data: importance.map((item) => item.name) }, tooltip: { trigger: 'axis' }, series: [{ type: 'bar', data: importance.map((item) => item.value), itemStyle: { color: '#4f9b82' } }] }), [importance]);
  const calibrationOption = useMemo(() => {
    const values = calibration.flatMap((item) => [item.predicted, item.observed]);
    const min = values.length ? Math.min(...values) : 0;
    const max = values.length ? Math.max(...values) : 1;
    return { grid: { left: 44, right: 18, top: 14, bottom: 32 }, tooltip: { trigger: 'axis' }, xAxis: { type: 'value', name: t('alpha.predicted'), min, max }, yAxis: { type: 'value', name: t('alpha.observed'), min, max }, series: [{ type: 'line', showSymbol: true, data: calibration.map((item) => [item.predicted, item.observed]), lineStyle: { color: '#4f9b82' }, itemStyle: { color: '#4f9b82' } }, { type: 'line', showSymbol: false, data: [[min, min], [max, max]], lineStyle: { color: '#7d8884', type: 'dashed' } }] };
  }, [calibration, t]);

  if (query.isLoading) return <PageSkeleton />;
  if (query.error) return <ErrorPanel error={query.error} />;
  if (!alpha) return <EmptyState title="Alpha qualification not found" description="The requested immutable qualification is unavailable." />;

  const lineage = alpha.lineage ?? [];
  const fallbackAlphaName = () => <><span>{t('alpha.name')}</span>{' '}<bdi dir="ltr">{alpha.id.slice(0, 8)}</bdi></>;
  const rootLabel = alpha.name ? <bdi dir="auto">{alpha.name}</bdi> : fallbackAlphaName();
  const nodes: Node[] = [
    {
      id: alpha.id,
      position: { x: 260, y: 0 },
      data: { label: <bdi dir="auto">{rootLabel}</bdi> },
      style: { background: 'var(--qz-accent-soft)', border: '1px solid var(--qz-accent)', color: 'var(--qz-text)', borderRadius: 8, width: 180, fontSize: 11 },
    },
    ...lineage.map((item, index) => ({
      id: item.id,
      position: { x: (index % 4) * 200, y: 140 + Math.floor(index / 4) * 110 },
      data: { label: <bdi dir="auto">{item.label} · {humanize(item.relationship)}</bdi> },
      style: { background: 'var(--qz-bg-elevated)', border: '1px solid var(--qz-border-strong)', color: 'var(--qz-text)', borderRadius: 8, width: 170, fontSize: 10 },
    })),
  ];
  const edges: Edge[] = lineage.map((item, index) => ({ id: `l-${index}`, source: item.id, target: alpha.id, style: { stroke: 'var(--qz-border-strong)' } }));

  return (
    <>
      <PageHeader title={alpha.name ?? fallbackAlphaName()} translateTitle={false} description="Qualification is explicitly scoped by Universe, horizon, role, calibration and independent evidence. Historical versions remain immutable when health or evidence changes." />
      <KpiStrip items={[{ label: 'Role', value: humanize(alpha.role) }, { label: 'Qualification', value: <StateBadge state={alpha.state} /> }, { label: 'Health', value: <StateBadge state={alpha.degradation_state ?? 'HEALTHY'} /> }, { label: 'Horizon', value: alpha.horizon ?? '—' }]} />
      <div className="qz-grid-2" style={{ marginTop: 20 }}>
        <Section title="Performance" meta="API evidence · Lightweight Charts">{performance.length ? <div className="qz-panel qz-panel-pad"><FinancialSeriesChart ariaLabel="Alpha performance and benchmark chart" series={[{ name: 'Alpha', data: performance, kind: 'area' }, { name: 'Benchmark', data: benchmark }]} /></div> : <EmptyState title="No performance series" description="The Alpha API has not returned a performance curve for this qualification." />}</Section>
        <Section title="Drawdown & degradation" meta="Forward health evidence">{drawdown.length || degradation.length ? <div className="qz-panel qz-panel-pad"><FinancialSeriesChart ariaLabel="Alpha drawdown and degradation chart" series={[{ name: 'Drawdown', data: drawdown, kind: 'area' }, { name: 'Health', data: degradation }]} /></div> : <EmptyState title="No degradation series" description="Health remains represented by the qualification state until time-series evidence is returned." />}</Section>
      </div>
      <div className="qz-grid-2">
        <Section title="Calibration" meta="Predicted vs observed">{calibration.length ? <div className="qz-panel qz-panel-pad"><EChart ariaLabel="Alpha calibration chart" option={calibrationOption} /></div> : <EmptyState title="No calibration curve" description="Calibration metadata exists independently; the API did not return curve points." />}</Section>
        <Section title="Feature importance" meta="Explainability evidence">{importance.length ? <div className="qz-panel qz-panel-pad"><EChart ariaLabel="Feature importance chart" option={importanceOption} /></div> : <EmptyState title="No feature importance" description="No explainability vector was returned for this Alpha qualification." />}</Section>
      </div>
      <Section title="Qualification lineage" meta="React Flow · immutable ancestry and reusable evidence"><div className="qz-flow"><ReactFlow ariaLabelConfig={ariaLabelConfig} nodes={nodes} edges={edges} fitView nodesDraggable={false} nodesConnectable={false}><Background gap={22} color="var(--qz-border)" /><Controls showInteractive={false} /></ReactFlow></div></Section>
      <Section title="Scope and evidence"><div className="qz-panel qz-panel-pad"><pre className="qz-code" dir="ltr">{JSON.stringify({ universe: alpha.universe ?? alpha.universe_version_id, scope: alpha.scope_json, metrics: alpha.metrics, evaluation_episode_id: alpha.evaluation_episode_id }, null, 2)}</pre></div></Section>
    </>
  );
}
