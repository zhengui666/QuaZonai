'use client';
import { useEffect, useMemo, useRef } from 'react';
import * as echarts from 'echarts/core';
import { LineChart } from 'echarts/charts';
import { AriaComponent, DataZoomComponent, GridComponent, TooltipComponent } from 'echarts/components';
import { SVGRenderer } from 'echarts/renderers';
import { Button, Table, theme } from 'antd';
import type { EquityPoint } from '../../../packages/contracts/src/index.ts';
import { equityCoordinates, formatEquity, orderedEquityPoints } from './equity-values';
import { Empty, Exact, MetricLabel, Timestamp } from './ui';

echarts.use([LineChart, GridComponent, TooltipComponent, DataZoomComponent, AriaComponent, SVGRenderer]);
const axisNumber = (value: number) => value.toLocaleString('zh-CN', { maximumFractionDigits: 2, notation: Math.abs(value) >= 1e6 ? 'compact' : 'standard' });

export default function EquityPlot({ points, currency, initialCapital }: { points: EquityPoint[]; currency: string; initialCapital?: string }) {
  const element = useRef<HTMLDivElement>(null); const chart = useRef<echarts.EChartsType | undefined>(undefined);
  const ordered = useMemo(() => orderedEquityPoints(points), [points]);
  const coordinates = useMemo(() => equityCoordinates(points), [points]);
  const clockGroups = useMemo(() => {
    const groups = new Map<number, number[]>();
    coordinates.forEach(([at], index) => { const group = groups.get(at); if (group) group.push(index); else groups.set(at, [index]); });
    return groups;
  }, [coordinates]);
  const tablePoints = useMemo(() => [...ordered, ...points.filter(point => !Number.isFinite(Date.parse(point.at)))], [ordered, points]);
  const invalidTimes = points.length - ordered.length;
  const drawable = coordinates.some(point => point[1] !== null);
  const latest = invalidTimes ? undefined : ordered.at(-1);
  const { token } = theme.useToken();
  const { colorLink: accent, colorTextSecondary: secondary, colorBorderSecondary: line, colorBgContainer: surface, colorText: text } = token;
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
    chart.current?.setOption({ animation: false, useUTC: true, textStyle: { fontFamily: '-apple-system, BlinkMacSystemFont, "PingFang SC", "Microsoft YaHei", sans-serif' }, aria: { enabled: true, label: { description } },
      grid: [{ top: 18, right: 16, bottom: 80, left: 64, outerBoundsMode: 'none' }],
      axisPointer: { link: [{ xAxisIndex: 'all' }], lineStyle: { color: secondary, type: 'dashed' } },
      tooltip: { trigger: 'axis', confine: true, renderMode: 'richText', backgroundColor: surface, borderColor: line, textStyle: { color: text, fontSize: 12 },
        formatter: (params: unknown) => {
          const item = (Array.isArray(params) ? params[0] : params) as { dataIndex: number } | undefined;
          const index = item?.dataIndex; const coordinate = index === undefined ? undefined : coordinates[index];
          return coordinate ? (clockGroups.get(coordinate[0]) ?? []).map(index => {
            const point = ordered[index]!;
            return `${point.at.replace('T', ' ').replace('Z', ' UTC')}\n账户权益  ${point.value ?? '未提供'} ${currency}`;
          }).join('\n\n') : '';
        } },
      xAxis: [timeAxis(0, true)],
      yAxis: [{ type: 'value', scale: true, splitNumber: 4, axisLabel: { color: secondary, fontSize: 11, width: 54, overflow: 'truncate', formatter: axisNumber }, splitLine: { lineStyle: { color: line, type: 'dashed' } } }],
      dataZoom: [{ id: 'time-slider', type: 'slider', xAxisIndex: [0], filterMode: 'none', bottom: 4, height: 24, borderColor: line, backgroundColor: 'transparent', fillerColor: 'rgba(128,150,146,0.12)', textStyle: { color: secondary }, handleStyle: { color: accent, borderColor: accent }, dataBackground: { lineStyle: { color: secondary }, areaStyle: { color: line } }, selectedDataBackground: { lineStyle: { color: accent }, areaStyle: { color: accent, opacity: 0.16 } }, brushSelect: false, showDetail: false },
        { id: 'time-inside', type: 'inside', xAxisIndex: [0], filterMode: 'none', zoomOnMouseWheel: 'ctrl' }],
      series: [{ id: 'account', name: '账户权益', type: 'line', data: coordinates, connectNulls: false, smooth: false, showSymbol: coordinates.length === 1, symbolSize: 5, lineStyle: { width: 2, color: accent }, itemStyle: { color: accent } }],
    }, { replaceMerge: ['grid', 'xAxis', 'yAxis', 'series'] });
  }, [coordinates, ordered, clockGroups, currency, accent, secondary, line, surface, text, description, intraday, drawable]);
  if (!points.length) return <Empty title="尚无权益曲线" />;
  return <section className="equity-section">
    <div className="section-heading equity-heading"><div><h3>{initialCapital === undefined ? '账户权益' : '回测表现'}</h3><span className="quiet">{currency} · UTC · <MetricLabel metricKey="records" label={`${ordered.length.toLocaleString('zh-CN')} 个记录点`} /></span></div>
    </div>
    <dl className="equity-summary"><div><dt><MetricLabel metricKey="endingEquity" /> <span>{currency}</span></dt><dd><Exact value={formatEquity(latest?.value)} /></dd></div></dl>
    <p className="quiet">仅绘制原始账户权益；未提供上游收益与回撤序列，不在页面自行计算。</p>
    {invalidTimes > 0 && <p className="error-text" role="status">{invalidTimes} 个记录时间无效，未绘制；原始记录见下方数据。</p>}
    {drawable ? <><div className="equity-legend"><span><i /><MetricLabel metricKey="equity" label={`账户权益 / ${currency}`} /></span><Button size="small" type="text" onClick={() => chart.current?.dispatchAction({ type: 'dataZoom', start: 0, end: 100 })}>重置区间</Button></div>
      <div ref={element} className="equity-chart" data-testid="equity-chart" role="img" aria-label={description} />
      <p className="equity-caption quiet">{ordered[0]?.at.slice(0, 10)} — {ordered.at(-1)?.at.slice(0, 10)}<span>拖动底部选择区间</span></p></> : <Empty title={points.every(point => point.value === null) ? '权益数值未提供' : '数值无法绘制'} />}
    <details className="curve-details"><summary>查看数据</summary><Table<EquityPoint> rowKey={(point, index) => `${point.at}/${index}`} size="small" pagination={{ pageSize: 20, hideOnSinglePage: true }} dataSource={tablePoints} scroll={{ x: 450 }} columns={[{ title: '时间', render: (_value, point) => <Timestamp value={point.at} /> }, { title: <MetricLabel metricKey="equity" label={`权益 / ${currency}`} />, align: 'right', render: (_value, point) => <Exact value={point.value} /> }]} /></details>
  </section>;
}
