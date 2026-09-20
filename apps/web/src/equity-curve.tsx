import { Alert, Button, Input, Select, Skeleton, Space, Tag, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { lazy, Suspense, useId, useState } from 'react';
import { api, ApiFailure, dataOf } from './api';
import type { Schema } from './api';
import { checkedEquity, equityRange, equityResolutions } from './equity-data';
import type { EquityResolution } from './equity-data';
import { ErrorNotice, NoData, QueryPanel, useOnline } from './ui';

const EquityPlot = lazy(() => import('./equity-plot'));
const unavailable: Record<Schema['EquityUnavailableReason'], string> = {
  SIMULATION_FAILED: '回测未完成',
  INVALID_EVIDENCE: '权益证据不可用',
  NO_SIMULATION: '本次研究未产生权益数据',
  LEGACY_SNAPSHOTS_UNAVAILABLE: '此历史回测没有权益快照',
};

export default function EquityCurve({ evaluation }: { evaluation: Schema['EvaluationView'] }) {
  const online = useOnline();
  const inputId = useId();
  const [draft, setDraft] = useState({ start: '', end: '' });
  const [range, setRange] = useState<{ start_ns?: string; end_ns?: string }>({});
  const [resolution, setResolution] = useState<EquityResolution>('AUTO');
  const [reset, setReset] = useState(0);
  const [rangeError, setRangeError] = useState<unknown>();
  const query = useQuery({
    queryKey: ['equity-curve', evaluation.project_id, evaluation.subject_candidate_id, evaluation.id, evaluation.run_id, range.start_ns, range.end_ns, resolution],
    enabled: online && evaluation.evaluation_kind === 'PORTFOLIO',
    queryFn: async ({ signal }) => checkedEquity(dataOf(await api.GET('/api/v2/evaluations/{id}/equity-curve', {
      params: { path: { id: evaluation.id }, query: { ...range, resolution } }, signal,
    })), evaluation),
  });
  function applyRange() {
    try {
      if (!draft.start && !draft.end) setRange({});
      else if (draft.start && draft.end) setRange(equityRange(Date.parse(draft.start), Date.parse(draft.end)));
      else throw new ApiFailure('EQUITY_RANGE_REQUIRED', '请选择开始和结束时间。');
      setReset(value => value + 1);
      setRangeError(undefined);
    } catch (error) { setRangeError(error); }
  }
  const curve = query.isError ? undefined : query.data?.curve;
  const ready = curve?.status === 'READY' ? curve : undefined;
  if (evaluation.evaluation_kind !== 'PORTFOLIO') return null;
  return <section aria-label="历史回测组合价值" className="full-width" style={{ minWidth: 0 }}>
    <Space orientation="vertical" size="middle" className="full-width">
      <Typography.Title level={3}>{ready ? `组合价值（${ready.series.base_currency}）` : '组合价值'}</Typography.Title>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 12, alignItems: 'end' }}>
        <div style={{ maxWidth: '100%', width: 240 }}>
          <label htmlFor={`${inputId}-start`}>开始（本地时间）</label>
          <Input id={`${inputId}-start`} type="datetime-local" step="0.001" value={draft.start} disabled={!online}
            onChange={event => setDraft(value => ({ ...value, start: event.target.value }))} />
        </div>
        <div style={{ maxWidth: '100%', width: 240 }}>
          <label htmlFor={`${inputId}-end`}>结束（本地时间）</label>
          <Input id={`${inputId}-end`} type="datetime-local" step="0.001" value={draft.end} disabled={!online}
            onChange={event => setDraft(value => ({ ...value, end: event.target.value }))} />
        </div>
        <Button onClick={applyRange} disabled={!online} loading={query.isFetching}>查看区间</Button>
        <Select<EquityResolution> aria-label="权益显示粒度" value={resolution} options={equityResolutions}
          onChange={setResolution} disabled={!online} style={{ minWidth: 90 }} />
        <Button disabled={!online} onClick={() => {
          setDraft({ start: '', end: '' }); setRange({}); setResolution('AUTO');
          setRangeError(undefined); setReset(value => value + 1);
        }}>完整区间</Button>
      </div>
      <ErrorNotice error={rangeError} />
      {!online ? <Alert type="warning" showIcon title="离线，权益数据暂不可读取" /> :
        <QueryPanel pending={query.isPending} error={query.error} reload={() => { void query.refetch(); }}>
          {curve?.status === 'UNAVAILABLE' && <NoData text={unavailable[curve.reason_code]} />}
          {ready && <>
            <Space wrap>
              <Typography.Text type="secondary">{equityResolutions.find(item => item.value === ready.series.resolution)?.label} · UTC</Typography.Text>
              {evaluation.origin !== 'REAL' && <Tag>{evaluation.origin}</Tag>}
              {evaluation.evidence_status === 'INCOMPLETE' && <Tag>证据不完整</Tag>}
              <Typography.Text type="secondary">{ready.series.points.length} / {ready.series.window_point_count} 点</Typography.Text>
            </Space>
            {ready.series.points.length === 0 ? <NoData text="所选区间无权益观测" /> :
              <Suspense fallback={<Skeleton active paragraph={{ rows: 4 }} />}>
                <EquityPlot key={`${ready.source_artifact_id}/${range.start_ns ?? ''}/${range.end_ns ?? ''}/${resolution}/${reset}`} series={ready.series} />
              </Suspense>}
          </>}
        </QueryPanel>}
    </Space>
  </section>;
}
