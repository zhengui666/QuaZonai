import { useState, useRef } from 'react';
import { Alert, Descriptions, Pagination, Table, Tag } from 'antd';
import { api } from './api';
import { ErrorNotice } from './ui';
import type { ResearchDetail } from '../../../packages/contracts/src/index.ts';

const record = (value: unknown): Record<string, unknown> => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
const scalar = (value: unknown) => typeof value === 'string' || typeof value === 'number' ? String(value) : '未提供';
export function unavailableReason(value: unknown): string {
  const reasons: Record<string, string> = {
    OFFICIAL_PORTFOLIO_RETURNS_UNAVAILABLE: '尚无可用的官方日权益收益序列。',
    UNPRICED_PORTFOLIO_SNAPSHOTS: '部分账户快照缺少估值，不能比较收益表现。',
    DISABLED_BY_DISPOSE_ON_COMPLETION_CONFIG: '本次配置不保留扩展报告。',
    UPSTREAM_DISPOSE_ON_COMPLETION: '本次配置不保留扩展报告。',
    NO_COMPLETED_NATIVE_RESULT: '尚无已完成的官方运行结果。',
    NO_RETAINED_ENGINE: '本次运行未保留可读取的扩展报告。',
    OMITTED_BY_DISPOSE_ON_COMPLETION_CONFIG: '本次配置选择运行结束后不保留扩展报告。',
    OFFICIAL_RUN_RESULT_UNAVAILABLE: '官方运行尚无可读取的完成结果。',
    NATIVE_CONFIGURED_ENGINE_UNAVAILABLE: '本次配置运行未保留可读取的引擎报告。',
  };
  return typeof value === 'string' && reasons[value] ? reasons[value] : '相关证据尚不可用，不能据此评价策略表现。';
}
export function NativeStrategyResultView({ value, researchId }: { value: NonNullable<ResearchDetail['final']>['nativeStrategyResult']; researchId?: number }) {
  const [page, setPage] = useState(1);
  const [remote, setRemote] = useState<{ items: unknown[]; total: number; resultTaskId: number }>();
  const [loading, setLoading] = useState(false); const [error, setError] = useState<unknown>(); const inFlight = useRef(false);
  const loadPage = async (next: number) => {
    if (!researchId || typeof value?.resultTaskId !== 'number' || inFlight.current) return;
    inFlight.current = true; setLoading(true); setError(undefined);
    try { const result = await api<{ items: unknown[]; total: number; resultTaskId: number }>(`/research/${researchId}/native-runs?resultTaskId=${value.resultTaskId}&offset=${(next - 1) * 20}`); if (result.resultTaskId !== value.resultTaskId) throw new Error('结果已更新，请重新打开结果页。'); setRemote(result); setPage(next); }
    catch (failure) { setError(failure); } finally { inFlight.current = false; setLoading(false); }
  };
  if (!value) return null;
  const request = record(value.request); const venue = record(request.venue); const strategy = record(request.strategy);
  const equity = record(value.equityPerformance);
  const statistics = Array.isArray(equity.statistics) ? equity.statistics : [];
  const runs = remote?.items ?? (Array.isArray(value.nativeResults) ? value.nativeResults : []);
  const total = remote?.total ?? (typeof value.nativeResultsTotal === 'number' ? value.nativeResultsTotal : runs.length);
  const canonical = record(value.canonicalResult);
  const tables = [['orders', '订单'], ['fills', '成交'], ['positions', '持仓'], ['portfolio_snapshots', '权益快照']] as const;
  return <section className="research-result" aria-label="官方回测结果">
    <header className="result-title"><div><span className="eyebrow">NAUTILUS · 历史回测</span><h3>让证据说明表现</h3></div><Tag>官方原值</Tag></header>
    <p className="quiet">回测完成不等于样本外验证通过，也不构成模拟盘或实盘交易许可。</p>
    {record(value.instrumentDefinition).mode === 'STATIC_SCENARIO' && <Alert type="warning" title="静态规格情景" description="未验证当时真实规格；不能当作实际历史可交易性或实盘证据。" />}
    <Descriptions size="small" column={{ xs: 1, sm: 2 }} items={[
      { key: 'strategy', label: '策略', children: scalar(strategy.name ?? strategy.kind ?? request.name) },
      { key: 'instrument', label: '标的', children: scalar(request.instrument_id) },
      { key: 'native', label: 'Nautilus', children: scalar(value.nativeVersion) },
      { key: 'currency', label: '权益币种', children: scalar(equity.currency) },
    ]} />
    <details className="result-card"><summary>本次回测条件</summary><Descriptions size="small" column={1} items={[
      { key: 'start', label: '回放开始（原始纳秒时间）', children: scalar(request.event_start_ns) },
      { key: 'end', label: '回放结束（原始纳秒时间）', children: scalar(request.event_end_ns) },
      { key: 'balance', label: '初始余额与币种', children: Array.isArray(venue.starting_balances) ? venue.starting_balances.filter(value => typeof value === 'string').slice(0, 20).join('；') || '未提供' : '当前报告未展开初始余额' },
      { key: 'maker', label: '挂单费率（原始小数）', children: scalar(venue.maker_fee) },
      { key: 'taker', label: '吃单费率（原始小数）', children: scalar(venue.taker_fee) },
    ]} /><p className="result-footnote">以上是当前请求的假设，不以数据集全范围替代实际回放窗口，也不等于真实账户余额。</p></details>
    <section className="result-card"><h4>账户权益表现</h4>
      {equity.status !== 'AVAILABLE' && <Alert type="info" title="暂不可评价" description={unavailableReason(equity.reason ?? value.equityUnavailableReason)} />}
      <Table pagination={false} size="small" rowKey={(_, i) => String(i)} dataSource={statistics.filter(item => ['CAGR', 'Sharpe Ratio'].some(name => String(record(item).native_key).startsWith(name))).slice(0, 8).map(record)} columns={[
        { title: '官方统计', dataIndex: 'native_key', render: value => scalar(value) },
        { title: '年化口径', dataIndex: 'annualization_days', render: value => `${scalar(value)} 天` },
        { title: '原值', dataIndex: 'value', render: value => equity.status === 'AVAILABLE' ? scalar(value) : '不可用' },
      ]} locale={{ emptyText: '尚无可用的账户权益统计' }} />
      <p className="result-footnote">252 / 365 是年化频率，不是观测天数。短样本年化是外推；缺少权益证据时不替换为持仓收益，也不补零。</p>
      <Descriptions size="small" column={2} items={[
        { key: 'days', label: '已观察天数', children: scalar(equity.observed_utc_days) },
        { key: 'returns', label: '收益序列天数', children: scalar(equity.return_days) },
      ]} />
    </section>
    {total > 0 && <section className="result-card"><h4>官方运行记录</h4><ErrorNotice error={error} /><Table loading={loading} size="small" pagination={false} rowKey={(_, index) => String(index)} dataSource={runs.slice(0, 20).map(record)} columns={[
      { title: '订单数', dataIndex: 'total_orders', render: scalar },
      { title: '持仓数', dataIndex: 'total_positions', render: scalar },
      { title: '事件数', dataIndex: 'total_events', render: scalar },
      { title: '回放迭代', dataIndex: 'iterations', render: scalar },
    ]} />{researchId ? <Pagination current={page} pageSize={20} total={total} onChange={next => { void loadPage(next); }} disabled={loading} showSizeChanger={false} hideOnSinglePage aria-label="官方运行记录分页" /> : total > 20 ? <p>仅展示前 20 条运行记录。</p> : null}<p className="result-footnote">运行计数不是收益指标，不用于评价策略优劣。</p></section>}
    {value.canonicalResult === null && <Alert type="info" title="扩展报告未保留" description={`${unavailableReason(value.canonicalUnavailableReason)} 这不等于回测没有运行。`} />}
    {tables.some(([key]) => canonical[key] !== undefined) && <section className="result-card"><h4>原始订单与账户证据</h4>{tables.map(([key, label]) => {
      const table = canonical[key]; if (table === undefined) return null;
      const manifest = record(table);
      return <p key={key}><strong>{label}</strong> · {Array.isArray(table) ? `${table.length} 条已记录` : manifest.format === 'qz.native-table/v1' ? '已按页保存' : '已保留原始证据'}。明细尚未在此页面加载。</p>;
    })}<p className="result-footnote">页面不会下载或展开整份大型报告；可以在对话中要求读取指定范围的原始记录。</p></section>}
  </section>;
}
