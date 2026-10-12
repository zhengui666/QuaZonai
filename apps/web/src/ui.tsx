import { Alert, Button, Empty, Skeleton, Tag } from 'antd';
import { ReloadOutlined } from '@ant-design/icons';
import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { useEffect, useState, type ReactNode } from 'react';
import type { JsonObject, Metric, ModelRef, Page, ResearchSummary } from '../../../packages/contracts/src/index.ts';
import { api, shortError } from './api';

export function useResource<T>(path: string, poll: number | false = false, enabled = true) {
  return useQuery({ queryKey: [path], queryFn: ({ signal }) => api<T>(path, { signal }), refetchInterval: poll, enabled });
}
export function useResearch(poll: number | false = false) {
  const query = useInfiniteQuery({
    queryKey: ['/research'], initialPageParam: undefined as JsonObject | undefined,
    queryFn: ({ signal, pageParam }) => api<Page<ResearchSummary>>('/research' + (pageParam ? `?cursor=${encodeURIComponent(JSON.stringify(pageParam))}` : ''), { signal }),
    getNextPageParam: page => page.cursor, refetchInterval: poll,
  });
  const items = query.data ? [...new Map(query.data.pages.flatMap(page => page.items).map(item => [item.id, item])).values()] : undefined;
  return { ...query, data: items ? { items } : undefined };
}
export function MoreResearch({ query }: { query: { hasNextPage: boolean; isFetching: boolean; fetchNextPage: () => Promise<unknown> } }) {
  return query.hasNextPage ? <Button onClick={() => { void query.fetchNextPage(); }} loading={query.isFetching}>载入更多</Button> : null;
}
export function useMedia(query: string) {
  const [matches, setMatches] = useState(() => matchMedia(query).matches);
  useEffect(() => {
    const media = matchMedia(query); const update = () => setMatches(media.matches);
    update(); media.addEventListener('change', update);
    return () => media.removeEventListener('change', update);
  }, [query]);
  return matches;
}
export function ErrorNotice({ error, retry }: { error: unknown; retry?: () => void }) {
  return error ? <Alert className="error-notice" role="alert" type="error" showIcon title={shortError(error)}
    action={retry && <Button size="small" onClick={retry}>重试</Button>} /> : null;
}
type QueryState = { isPending: boolean; error: unknown; data?: unknown; refetch: () => Promise<unknown> };
export function QueryPanel({ query, children }: { query: QueryState; children: ReactNode }) {
  if (query.isPending) return <div role="status" aria-label="加载中" className="loading-state"><Skeleton active={false} paragraph={{ rows: 3 }} /></div>;
  return <><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />{query.data !== undefined && children}</>;
}
export function NoData({ text = '暂无记录' }: { text?: string }) {
  return <div className="empty-state"><Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={text} /></div>;
}
export function PageHeading({ title, count, action }: { title: string; count?: number; action?: ReactNode }) {
  return <header className="page-heading"><div><h1>{title}</h1>{count !== undefined && <span className="heading-count">{count} 项</span>}</div><div className="heading-actions">{action}</div></header>;
}
export function RefreshButton({ loading, onClick, label = '刷新' }: { loading: boolean; onClick: () => void; label?: string }) {
  return <Button className="refresh-button" icon={<ReloadOutlined aria-hidden />} loading={loading} aria-label={label} title={label} onClick={onClick} />;
}
const labels: Record<string, string> = {
  queued: '排队中', pending: '等待中', running: '运行中', stopping: '停止中', stopped: '已停止',
  cancelled: '已取消', completed: '已完成', succeeded: '已完成', failed: '失败', error: '异常',
  ready: '就绪', loaded: '已加载', disabled: '已停用', pending_restart: '待重启',
  active: '运行中', accepted: '已接纳', submitted: '已提交', delivered: '已交付',
  acknowledged: '已确认', expired: '已过期', rejected: '已拒绝', issued: '已签发',
  done: '已完成', complete: '已完成', success: '成功', stopping_requested: '停止中',
};
export function isActive(status: string) { return ['queued', 'pending', 'running', 'active'].includes(status); }
export function isReady(status: string) { return status === 'ready'; }
export function Status({ value }: { value: string | null | undefined }) {
  const tone = value && ['ready', 'completed', 'succeeded', 'loaded', 'acknowledged', 'success', 'done', 'complete'].includes(value) ? 'good'
    : value && ['failed', 'error', 'rejected'].includes(value) ? 'bad'
      : value && ['running', 'active', 'pending', 'queued', 'stopping', 'pending_restart', 'issued', 'submitted'].includes(value) ? 'busy' : 'quiet';
  return <Tag className={`status status-${tone}`}><span aria-hidden className="status-dot" />{value ? labels[value] ?? value : '—'}</Tag>;
}
export function display(value: string | number | null | undefined) { return value === null || value === undefined || value === '' ? '—' : value; }
export function number(value: string | number | null | undefined) {
  if (value === null || value === undefined || value === '') return '—';
  const parsed = Number(value);
  return Number.isFinite(parsed) ? new Intl.NumberFormat('zh-CN', { maximumFractionDigits: 4 }).format(parsed) : '—';
}
export function time(value: string | null | undefined) {
  if (!value || Number.isNaN(Date.parse(value))) return '—';
  return new Intl.DateTimeFormat('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hour12: false }).format(new Date(value));
}
export function modelKey(model: ModelRef | null | undefined) { return model ? `${model.provider}::${model.modelId}` : undefined; }
export function MetricGrid({ metrics }: { metrics: Metric[] }) {
  return metrics.length ? <dl className="metric-grid">{metrics.map(metric => <div key={metric.key}>
    <dt>{metric.label}</dt><dd>{display(metric.value)}{metric.value !== null && metric.value !== '' && metric.unit && <small>{metric.unit}</small>}</dd>
  </div>)}</dl> : <div className="muted metric-empty">暂无指标</div>;
}
export function Surface({ title, extra, children, className = '' }: { title?: string; extra?: ReactNode; children: ReactNode; className?: string }) {
  return <section className={`surface ${className}`}>{title && <div className="section-heading"><h2>{title}</h2>{extra}</div>}{children}</section>;
}
