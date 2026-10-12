import { Collapse, Table } from 'antd';
import ReactEChartsCore from 'echarts-for-react/lib/core';
import * as echarts from 'echarts/core';
import { LineChart } from 'echarts/charts';
import { AriaComponent, GridComponent, TooltipComponent } from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';
import type { EquityPoint } from '../../../packages/contracts/src/index.ts';
import { display, NoData, number } from './ui';

echarts.use([LineChart, GridComponent, TooltipComponent, AriaComponent, CanvasRenderer]);
export function equityValues(points: EquityPoint[]) {
  return points.map(point => point.value === null || point.value.trim() === '' || !Number.isFinite(Number(point.value)) ? null : Number(point.value));
}
export default function EquityPlot({ points, currency, title }: { points: EquityPoint[]; currency: string; title: string }) {
  const values = equityValues(points);
  const option = {
    animation: false, useUTC: true, backgroundColor: 'transparent', textStyle: { color: '#756152', fontFamily: 'sans-serif' },
    aria: { enabled: true, label: { description: `${title} · ${currency}` } },
    grid: { left: 8, right: 16, top: 32, bottom: 12, containLabel: true },
    xAxis: { type: 'time', axisLine: { lineStyle: { color: '#dfd1c2' } }, axisTick: { show: false }, axisLabel: { color: '#756152', hideOverlap: true } },
    yAxis: { type: 'value', scale: true, splitNumber: 4, axisLabel: { color: '#756152', formatter: (value: number) => number(value) }, splitLine: { lineStyle: { color: '#eee5d9', type: 'dashed' } } },
    tooltip: { trigger: 'axis', confine: true, renderMode: 'richText', backgroundColor: '#fffcf7', borderColor: '#dfd1c2', textStyle: { color: '#49352a' },
      formatter: (params: { dataIndex: number }[]) => { const point = points[params[0]?.dataIndex ?? -1]; return point ? `${point.at}\n${display(point.value)} ${currency}` : ''; } },
    series: [{ name: title, type: 'line', data: points.map((point, index) => [Date.parse(point.at), values[index]]), connectNulls: false, smooth: false, showSymbol: points.length <= 200, symbolSize: 4,
      lineStyle: { color: '#bf674b', width: 2.5 }, itemStyle: { color: '#bf674b' }, areaStyle: { color: '#bf674b', opacity: 0.055 } }],
  };
  return <div className="chart-panel"><div className="chart-title"><h2>{title}</h2><span className="muted">{display(currency)}</span></div>
    {values.some(value => value !== null) ? <div className="equity-chart" data-testid="equity-chart"><ReactEChartsCore echarts={echarts} option={option} notMerge style={{ height: '100%', width: '100%' }} opts={{ renderer: 'canvas' }} /></div> : <NoData text="暂无曲线" />}
    {points.length > 0 && <Collapse ghost className="chart-table" items={[{ key: 'values', label: '曲线明细', children: <Table<EquityPoint> rowKey={(_, index) => String(index)} dataSource={points} size="small" pagination={{ pageSize: 8, size: 'small', showSizeChanger: false }} columns={[
      { title: '时间', dataIndex: 'at', render: display }, { title: '账户净值', dataIndex: 'value', align: 'right', render: display },
    ]} /> }]} />}
  </div>;
}
