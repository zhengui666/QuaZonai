'use client';
import { useEffect, useRef } from 'react';
import * as echarts from 'echarts/core';
import { LineChart } from 'echarts/charts';
import { AriaComponent, GridComponent, TooltipComponent } from 'echarts/components';
import { SVGRenderer } from 'echarts/renderers';
import { Table } from 'antd';
import type { EquityPoint } from '../../../packages/contracts/src/index.ts';
import { useAppearance } from './App';
import { Empty, Exact, Timestamp } from './ui';
echarts.use([LineChart, GridComponent, TooltipComponent, AriaComponent, SVGRenderer]);
export default function EquityPlot({ points, currency }: { points: EquityPoint[]; currency: string }) {
  const element = useRef<HTMLDivElement>(null); const chart = useRef<echarts.EChartsType | undefined>(undefined);
  const { mode } = useAppearance(); const dark = mode === 'dark' || mode === 'system' && matchMedia('(prefers-color-scheme: dark)').matches;
  useEffect(() => {
    if (!element.current) return;
    const instance = echarts.init(element.current, undefined, { renderer: 'svg' }); chart.current = instance;
    const observer = new ResizeObserver(() => instance.resize()); observer.observe(element.current);
    return () => { observer.disconnect(); instance.dispose(); chart.current = undefined; };
  }, [points.length > 0]);
  useEffect(() => {
    const values = points.map(point => point.value === null || !Number.isFinite(Number(point.value)) ? null : Number(point.value));
    chart.current?.setOption({ animation: false, aria: { enabled: true }, grid: { top: 20, right: 24, bottom: 35, left: 70 },
      tooltip: { trigger: 'axis', renderMode: 'richText', formatter: (params: unknown) => { const item = (Array.isArray(params) ? params[0] : params) as { dataIndex: number }; const point = points[item.dataIndex]; return point ? `${point.at}\n${point.value ?? '未提供'} ${currency}` : ''; } },
      xAxis: { type: 'category', boundaryGap: false, data: points.map(point => point.at), axisLine: { lineStyle: { color: dark ? '#38414A' : '#E0E5EA' } }, axisTick: { show: false }, axisLabel: { color: dark ? '#A7B1BC' : '#626F7B', hideOverlap: true, formatter: (value: string) => value.slice(0, 10) } },
      yAxis: { type: 'value', scale: true, axisLabel: { color: dark ? '#A7B1BC' : '#626F7B' }, splitLine: { lineStyle: { color: dark ? '#38414A' : '#E0E5EA' } } },
      series: [{ type: 'line', data: values, connectNulls: false, smooth: false, showSymbol: true, symbolSize: 5, lineStyle: { width: 2, color: dark ? '#94C7BC' : '#2A6B62' }, itemStyle: { color: dark ? '#94C7BC' : '#2A6B62' } }],
    });
  }, [points, currency, dark]);
  if (!points.length) return <Empty title="尚无权益曲线" />;
  return <section className="equity-section"><div className="section-heading"><h3>账户权益</h3><code>{currency}</code></div><div ref={element} className="equity-chart" data-testid="equity-chart" role="img" aria-label={`原生账户权益，${points.length} 个记录点`} /><details className="curve-details"><summary>查看数据</summary><Table<EquityPoint> rowKey={(point, index) => `${point.at}/${index}`} size="small" pagination={{ pageSize: 20, hideOnSinglePage: true }} dataSource={points} scroll={{ x: 450 }} columns={[{ title: '时间', render: (_value, point) => <Timestamp value={point.at} /> }, { title: `权益 / ${currency}`, align: 'right', render: (_value, point) => <Exact value={point.value} /> }]} /></details></section>;
}
