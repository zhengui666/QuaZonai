'use client';
import { useEffect, useRef } from 'react';
import * as echarts from 'echarts/core';
import { LineChart } from 'echarts/charts';
import { AriaComponent, GridComponent, TooltipComponent } from 'echarts/components';
import { SVGRenderer } from 'echarts/renderers';
import { Table } from 'antd';
import type { EquityPoint } from '../../../packages/contracts/src/index.ts';
import { useAppearance } from './App';
import { equityCoordinates } from './equity-values';
import { Empty, Exact, Timestamp } from './ui';
echarts.use([LineChart, GridComponent, TooltipComponent, AriaComponent, SVGRenderer]);
export default function EquityPlot({ points, currency }: { points: EquityPoint[]; currency: string }) {
  const element = useRef<HTMLDivElement>(null); const chart = useRef<echarts.EChartsType | undefined>(undefined);
  const drawable = points.some(point => point.value !== null && Number.isFinite(Number(point.value)));
  const description = `原生账户权益，${points.length} 个记录点${points.length ? `，${points[0]!.at} 至 ${points.at(-1)!.at}` : ''}`;
  const intraday = points.length > 0 && points.every(point => point.at.includes('T') && point.at.slice(0, 10) === points[0]!.at.slice(0, 10));
  const { mode } = useAppearance(); const dark = mode === 'dark' || mode === 'system' && matchMedia('(prefers-color-scheme: dark)').matches;
  useEffect(() => {
    if (!element.current) return;
    const instance = echarts.init(element.current, undefined, { renderer: 'svg' }); chart.current = instance;
    const observer = new ResizeObserver(() => instance.resize()); observer.observe(element.current);
    return () => { observer.disconnect(); instance.dispose(); chart.current = undefined; };
  }, [drawable]);
  useEffect(() => {
    const values = equityCoordinates(points);
    chart.current?.setOption({ animation: false, useUTC: true, textStyle: { fontFamily: '"Geist Variable", system-ui, sans-serif' }, aria: { enabled: true, label: { description } }, grid: { top: 20, right: 24, bottom: 35, left: 70 },
      tooltip: { trigger: 'axis', renderMode: 'richText', formatter: (params: unknown) => { const item = (Array.isArray(params) ? params[0] : params) as { dataIndex: number } | undefined; const point = item && points[item.dataIndex]; return point ? `${point.at}\n${point.value ?? '未提供'} ${currency}` : ''; } },
      xAxis: { type: 'time', boundaryGap: false, minInterval: intraday ? 0 : 86_400_000, axisLine: { lineStyle: { color: dark ? '#38414A' : '#E0E5EA' } }, axisTick: { show: false }, axisLabel: { color: dark ? '#A7B1BC' : '#626F7B', hideOverlap: true, formatter: (value: number) => { const at = new Date(value).toISOString(); return intraday ? at.slice(11, 19) : at.slice(0, 10); } } },
      yAxis: { type: 'value', scale: true, axisLabel: { color: dark ? '#A7B1BC' : '#626F7B' }, splitLine: { lineStyle: { color: dark ? '#38414A' : '#E0E5EA' } } },
      series: [{ type: 'line', data: values, connectNulls: false, smooth: false, showSymbol: true, symbolSize: 5, lineStyle: { width: 2, color: dark ? '#94C7BC' : '#2A6B62' }, itemStyle: { color: dark ? '#94C7BC' : '#2A6B62' } }],
    });
  }, [points, currency, dark, description, intraday]);
  if (!points.length) return <Empty title="尚无权益曲线" />;
  return <section className="equity-section"><div className="section-heading"><h3>账户权益</h3><code>{currency}</code></div>{drawable ? <div ref={element} className="equity-chart" data-testid="equity-chart" role="img" aria-label={description} /> : <Empty title={points.every(point => point.value === null) ? '权益数值未提供' : '数值无法绘制'} />}<details className="curve-details"><summary>查看数据</summary><Table<EquityPoint> rowKey={(point, index) => `${point.at}/${index}`} size="small" pagination={{ pageSize: 20, hideOnSinglePage: true }} dataSource={points} scroll={{ x: 450 }} columns={[{ title: '时间', render: (_value, point) => <Timestamp value={point.at} /> }, { title: `权益 / ${currency}`, align: 'right', render: (_value, point) => <Exact value={point.value} /> }]} /></details></section>;
}
