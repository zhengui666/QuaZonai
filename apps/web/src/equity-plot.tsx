'use client';
import { useEffect, useMemo, useRef, useState } from 'react';
import * as echarts from 'echarts/core';
import { LineChart } from 'echarts/charts';
import { AriaComponent, DataZoomComponent, GridComponent, MarkLineComponent, TooltipComponent } from 'echarts/components';
import { SVGRenderer } from 'echarts/renderers';
import { Button, Radio, Table, theme } from 'antd';
import type { EquityPoint } from '../../../packages/contracts/src/index.ts';
import { equityCoordinates, equityPerformance, formatEquity, orderedEquityPoints } from './equity-values';
import { Empty, Exact, Timestamp } from './ui';

echarts.use([LineChart, GridComponent, TooltipComponent, DataZoomComponent, MarkLineComponent, AriaComponent, SVGRenderer]);
const percent = (value: number | null | undefined) => value == null ? '未提供' : `${value > 0 ? '+' : ''}${value.toLocaleString('zh-CN', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}%`;
const axisNumber = (value: number) => value.toLocaleString('zh-CN', { maximumFractionDigits: 2, notation: Math.abs(value) >= 1e6 ? 'compact' : 'standard' });

export default function EquityPlot({ points, currency, initialCapital }: { points: EquityPoint[]; currency: string; initialCapital?: string }) {
  const element = useRef<HTMLDivElement>(null); const chart = useRef<echarts.EChartsType | undefined>(undefined);
  const ordered = useMemo(() => orderedEquityPoints(points), [points]);
  const coordinates = useMemo(() => equityCoordinates(points), [points]);
  const performance = useMemo(() => equityPerformance(ordered, initialCapital), [ordered, initialCapital]);
  const clockGroups = useMemo(() => {
    const groups = new Map<number, number[]>();
    coordinates.forEach(([at], index) => { const group = groups.get(at); if (group) group.push(index); else groups.set(at, [index]); });
    return groups;
  }, [coordinates]);
  const tablePoints = useMemo(() => [...ordered, ...points.filter(point => !Number.isFinite(Date.parse(point.at)))], [ordered, points]);
  const [view, setView] = useState<'return' | 'equity'>('return');
  const showReturn = performance.hasCapital && view === 'return';
  const invalidTimes = points.length - ordered.length;
  const drawdownAvailable = performance.maxDrawdown !== null && invalidTimes === 0;
  const drawable = coordinates.some(point => point[1] !== null);
  const latest = invalidTimes ? undefined : ordered.at(-1);
  const latestReturn = invalidTimes ? null : performance.returns.at(-1);
  const { token } = theme.useToken();
  const { colorLink: accent, colorTextSecondary: secondary, colorBorderSecondary: line, colorError: loss, colorBgContainer: surface, colorText: text } = token;
  const description = `原生账户权益，${ordered.length} 个有效时间记录点，按时间排序，UTC${ordered.length ? `，${ordered[0]!.at} 至 ${ordered.at(-1)!.at}` : ''}`;
  const intraday = ordered.length > 0 && ordered[0]!.at.slice(0, 10) === ordered.at(-1)!.at.slice(0, 10);
  useEffect(() => {
    if (!element.current) return;
    const instance = echarts.init(element.current, undefined, { renderer: 'svg' }); chart.current = instance;
    const observer = new ResizeObserver(() => instance.resize()); observer.observe(element.current);
    return () => { observer.disconnect(); instance.dispose(); chart.current = undefined; };
  }, [drawable]);
  useEffect(() => {
    const timeAxis = (gridIndex: number, visible: boolean) => ({ type: 'time' as const, gridIndex, boundaryGap: false,
      axisLine: { show: false }, axisTick: { show: false }, splitLine: { show: false }, splitNumber: 4,
      axisLabel: { show: visible, color: secondary, hideOverlap: true, fontSize: 10, formatter: (value: number) => { const at = new Date(value).toISOString(); return intraday ? at.slice(11, 19) : at.slice(0, 10); } } });
    chart.current?.setOption({ animation: false, useUTC: true, textStyle: { fontFamily: '"Geist Variable", system-ui, sans-serif' }, aria: { enabled: true, label: { description } },
      grid: [{ top: 18, right: 16, bottom: drawdownAvailable ? '43%' : 80, left: 64, outerBoundsMode: 'none' }, ...(drawdownAvailable ? [{ top: '70%', right: 16, bottom: 80, left: 64, outerBoundsMode: 'none' }] : [])],
      axisPointer: { link: [{ xAxisIndex: 'all' }], lineStyle: { color: secondary, type: 'dashed' } },
      tooltip: { trigger: 'axis', confine: true, renderMode: 'richText', backgroundColor: surface, borderColor: line, textStyle: { color: text, fontSize: 12 },
        formatter: (params: unknown) => {
          const item = (Array.isArray(params) ? params[0] : params) as { dataIndex: number } | undefined;
          const index = item?.dataIndex; const coordinate = index === undefined ? undefined : coordinates[index];
          return coordinate ? (clockGroups.get(coordinate[0]) ?? []).map(index => {
            const point = ordered[index]!;
            return `${point.at.replace('T', ' ').replace('Z', ' UTC')}\n账户权益  ${point.value ?? '未提供'} ${currency}${performance.hasCapital ? `\n累计收益  ${percent(performance.returns[index])}\n回撤  ${drawdownAvailable ? percent(performance.drawdowns[index]) : '不可用（记录不完整）'}` : ''}`;
          }).join('\n\n') : '';
        } },
      xAxis: [timeAxis(0, !drawdownAvailable), ...(drawdownAvailable ? [timeAxis(1, true)] : [])],
      yAxis: [{ type: 'value', scale: true, splitNumber: 4, axisLabel: { color: secondary, fontSize: 11, width: 54, overflow: 'truncate', formatter: (value: number) => showReturn ? `${axisNumber(value)}%` : axisNumber(value) }, splitLine: { lineStyle: { color: line, type: 'dashed' } } },
        ...(drawdownAvailable ? [{ type: 'value', gridIndex: 1, max: 0, scale: true, splitNumber: 2, axisLabel: { color: secondary, fontSize: 10, formatter: (value: number) => `${axisNumber(value)}%` }, splitLine: { lineStyle: { color: line } } }] : [])],
      dataZoom: [{ id: 'time-slider', type: 'slider', xAxisIndex: drawdownAvailable ? [0, 1] : [0], filterMode: 'none', bottom: 4, height: 24, borderColor: line, backgroundColor: 'transparent', fillerColor: 'rgba(128,150,146,0.12)', textStyle: { color: secondary }, handleStyle: { color: accent, borderColor: accent }, dataBackground: { lineStyle: { color: secondary }, areaStyle: { color: line } }, selectedDataBackground: { lineStyle: { color: accent }, areaStyle: { color: accent, opacity: 0.16 } }, brushSelect: false, showDetail: false },
        { id: 'time-inside', type: 'inside', xAxisIndex: drawdownAvailable ? [0, 1] : [0], filterMode: 'none', zoomOnMouseWheel: 'ctrl' }],
      series: [{ id: 'account', name: showReturn ? '累计收益' : '账户权益', type: 'line', data: coordinates.map(([at, equity], index) => [at, showReturn ? performance.returns[index] : equity]), connectNulls: false, smooth: false, showSymbol: coordinates.length === 1, symbolSize: 5, lineStyle: { width: 2, color: accent }, itemStyle: { color: accent },
        markLine: showReturn ? { silent: true, symbol: 'none', label: { show: false }, lineStyle: { color: secondary, type: 'dashed', opacity: 0.6 }, data: [{ yAxis: 0 }] } : { data: [] } },
        ...(drawdownAvailable ? [{ id: 'drawdown', name: '回撤', type: 'line', xAxisIndex: 1, yAxisIndex: 1, data: coordinates.map(([at], index) => [at, performance.drawdowns[index]]), connectNulls: false, smooth: false, showSymbol: false, lineStyle: { width: 1, color: loss }, areaStyle: { color: loss, opacity: 0.12 }, itemStyle: { color: loss } }] : [])],
    }, { replaceMerge: ['grid', 'xAxis', 'yAxis', 'series'] });
  }, [coordinates, ordered, clockGroups, currency, performance, showReturn, drawdownAvailable, accent, secondary, line, loss, surface, text, description, intraday, drawable]);
  if (!points.length) return <Empty title="尚无权益曲线" />;
  return <section className="equity-section">
    <div className="section-heading equity-heading"><div><h3>{initialCapital === undefined ? '账户权益' : '回测表现'}</h3><span className="quiet">{currency} · UTC · {ordered.length.toLocaleString('zh-CN')} 个记录点</span></div>
      {performance.hasCapital && <Radio.Group size="small" optionType="button" aria-label="曲线显示口径" value={view} onChange={event => setView(event.target.value)} options={[{ label: '累计收益', value: 'return' }, { label: '账户权益', value: 'equity' }]} />}</div>
    {performance.hasCapital && <dl className="equity-summary" aria-label="全区间表现">
      <div><dt>期末权益 <span>{currency}</span></dt><dd className={(latest?.value?.length ?? 0) > 16 ? 'equity-long-value' : undefined}><Exact value={formatEquity(latest?.value)} /></dd></div>
      <div><dt>累计收益</dt><dd className={latestReturn != null && latestReturn < 0 ? 'equity-loss' : 'equity-gain'}>{percent(latestReturn)}</dd></div>
      <div><dt>最大回撤</dt><dd>{drawdownAvailable ? percent(performance.maxDrawdown) : '未提供'}</dd></div>
    </dl>}
    {invalidTimes > 0 && <p className="error-text" role="status">{invalidTimes} 个记录时间无效，未绘制；原始记录见下方数据。</p>}
    {performance.hasCapital && !drawdownAvailable && <p className="quiet">记录不完整，回撤不可用；缺失权益保留断线。</p>}
    {drawable ? <><div className="equity-legend"><span><i />{showReturn ? '累计收益' : `账户权益 / ${currency}`}</span>{drawdownAvailable && <span className="equity-loss"><i />回撤</span>}<Button size="small" type="text" onClick={() => chart.current?.dispatchAction({ type: 'dataZoom', start: 0, end: 100 })}>重置区间</Button></div>
      <div ref={element} className={`equity-chart${drawdownAvailable ? ' with-drawdown' : ''}`} data-testid="equity-chart" role="img" aria-label={description} />
      <p className="equity-caption quiet">{ordered[0]?.at.slice(0, 10)} — {ordered.at(-1)?.at.slice(0, 10)}<span>拖动底部选择区间 · 摘要为全区间{performance.hasCapital && ' · 收益相对初始本金'}</span></p></> : <Empty title={points.every(point => point.value === null) ? '权益数值未提供' : '数值无法绘制'} />}
    <details className="curve-details"><summary>查看数据</summary><Table<EquityPoint> rowKey={(point, index) => `${point.at}/${index}`} size="small" pagination={{ pageSize: 20, hideOnSinglePage: true }} dataSource={tablePoints} scroll={{ x: 450 }} columns={[{ title: '时间', render: (_value, point) => <Timestamp value={point.at} /> }, { title: `权益 / ${currency}`, align: 'right', render: (_value, point) => <Exact value={point.value} /> }]} /></details>
  </section>;
}
