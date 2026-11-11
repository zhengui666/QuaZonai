import { useEffect, useRef } from 'react';
import * as echarts from 'echarts/core';
import { HeatmapChart, LineChart } from 'echarts/charts';
import { AriaComponent, DataZoomComponent, GridComponent, LegendComponent, TooltipComponent, VisualMapComponent } from 'echarts/components';
import { SVGRenderer } from 'echarts/renderers';
import { theme } from 'antd';
import type { EquityPoint } from '../../../packages/contracts/src/index.ts';
import { equityCoordinates, orderedEquityPoints } from './equity-values';
import { Empty } from './ui';

echarts.use([HeatmapChart, LineChart, AriaComponent, DataZoomComponent, GridComponent, LegendComponent, TooltipComponent, VisualMapComponent, SVGRenderer]);

function ScanChart({ option, description, onPoint }: { option: echarts.EChartsCoreOption; description: string; onPoint?: (series: number, index: number) => void }) {
  const element = useRef<HTMLDivElement>(null); const chart = useRef<echarts.EChartsType | undefined>(undefined);
  useEffect(() => {
    if (!element.current) return;
    const instance = echarts.init(element.current, undefined, { renderer: 'svg' }); chart.current = instance;
    const observer = new ResizeObserver(() => instance.resize()); observer.observe(element.current);
    return () => { observer.disconnect(); instance.dispose(); chart.current = undefined; };
  }, []);
  useEffect(() => { chart.current?.setOption(option, { notMerge: true }); }, [option]);
  useEffect(() => {
    const instance = chart.current; if (!instance || !onPoint) return;
    const clicked = (event: unknown) => { const value = event as { seriesIndex?: number; dataIndex?: number }; if (value.seriesIndex !== undefined && value.dataIndex !== undefined) onPoint(value.seriesIndex, value.dataIndex); };
    instance.on('click', clicked); return () => { instance.off('click', clicked); };
  }, [onPoint]);
  return <div ref={element} className="scan-chart" role="img" aria-label={description} />;
}

export function ParameterHeatmap({ x, y, cells, metric, onSelect }: {
  x: { label: string; values: string[] }; y: { label: string; values: string[] };
  cells: { x: string; y: string; value: number | null; ordinal: string; status: string; reason: string | null }[];
  metric: string;
  onSelect?: (ordinal: string) => void;
}) {
  const { token } = theme.useToken();
  const available = cells.filter(cell => cell.value !== null && Number.isFinite(cell.value));
  const missing = cells.filter(cell => cell.value === null || !Number.isFinite(cell.value));
  if (!x.values.length || !y.values.length) return <Empty title="暂无二维参数切片" />;
  const values = available.map(cell => cell.value!);
  const description = `${x.label} 与 ${y.label} 的精确二维切片；${metric}；${available.length} 个有效值，${missing.length} 个未提供值。不插值，不跨其他参数聚合。`;
  const option: echarts.EChartsCoreOption = {
    animation: false, aria: { enabled: true, label: { description } }, textStyle: { color: token.colorText },
    grid: { top: 30, bottom: 100, left: 100, right: 30 },
    tooltip: { confine: true, renderMode: 'richText', backgroundColor: token.colorBgContainer, borderColor: token.colorBorderSecondary, textStyle: { color: token.colorText },
      formatter: (value: unknown) => { const item = value as { seriesIndex: number; dataIndex: number }; const cell = (item.seriesIndex === 0 ? available : missing)[item.dataIndex];
        return cell ? `${x.label}  ${cell.x}\n${y.label}  ${cell.y}\n试验 #${cell.ordinal}\n${metric}  ${cell.value ?? '不可用'}\n${cell.status}${cell.reason ? `\n${cell.reason}` : ''}` : ''; } },
    xAxis: { type: 'category', name: x.label, nameLocation: 'middle', nameGap: 50, data: x.values, splitArea: { show: true }, axisLabel: { color: token.colorTextSecondary, rotate: x.values.length > 12 ? 40 : 0 } },
    yAxis: { type: 'category', name: y.label, data: y.values, splitArea: { show: true }, axisLabel: { color: token.colorTextSecondary } },
    visualMap: values.length ? { type: 'continuous', seriesIndex: 0, min: Math.min(...values), max: Math.max(...values), calculable: false, orient: 'horizontal', left: 'center', bottom: 0, inRange: { color: [token.colorFillSecondary, token.colorPrimary] }, textStyle: { color: token.colorTextSecondary } } : undefined,
    series: [
      { type: 'heatmap', name: metric, data: available.map(cell => [x.values.indexOf(cell.x), y.values.indexOf(cell.y), cell.value]), itemStyle: { borderWidth: 1, borderColor: token.colorBgContainer } },
      // The separate status layer has no metric value and is excluded from visualMap.
      { type: 'heatmap', name: '无有效指标', data: missing.map(cell => ({ value: [x.values.indexOf(cell.x), y.values.indexOf(cell.y), 1], itemStyle: { color: token.colorFillAlter, borderWidth: 1, borderColor: cell.status === 'FAILED' ? token.colorError : cell.status === 'RECONCILING' ? token.colorWarning : token.colorBorderSecondary } })), label: { show: true, formatter: '—', color: token.colorTextSecondary } },
    ],
  };
  return <><ScanChart option={option} description={description} onPoint={(series, index) => { const cell = (series === 0 ? available : missing)[index]; if (cell) onSelect?.(cell.ordinal); }} /><p className="quiet">灰色单元格表示待执行、失败或指标不可用，悬停查看原因，点击查看原始参数点。其他参数固定为上方所选值。</p></>;
}

export function ParameterCurveComparison({ curves, currency }: { curves: { ordinal: string; points: EquityPoint[] }[]; currency: string }) {
  const { token } = theme.useToken();
  if (!curves.some(curve => curve.points.length)) return <Empty title="所选试验没有已记录的原生账户曲线" />;
  const series = curves.map(curve => ({ ...curve, ordered: orderedEquityPoints(curve.points), coordinates: equityCoordinates(curve.points) }));
  const description = `${curves.length} 个试验的原生账户权益；${currency}；UTC；缺失记录保留断线。`;
  return <><ScanChart description={description} option={{
    animation: false, useUTC: true, aria: { enabled: true, label: { description } },
    textStyle: { color: token.colorText }, grid: { top: 50, bottom: 75, left: 90, right: 25 },
    legend: { top: 0, textStyle: { color: token.colorTextSecondary } },
    tooltip: { trigger: 'axis', confine: true, renderMode: 'richText', backgroundColor: token.colorBgContainer, borderColor: token.colorBorderSecondary, textStyle: { color: token.colorText }, formatter: (values: unknown) => (Array.isArray(values) ? values : [values]).map(value => { const item = value as { seriesIndex: number; dataIndex: number }; const curve = series[item.seriesIndex]; const point = curve?.ordered[item.dataIndex]; return point ? `试验 #${curve!.ordinal}\n${point.at}\n${point.value ?? '未提供'} ${currency}` : ''; }).join('\n\n') },
    xAxis: { type: 'time', axisLabel: { color: token.colorTextSecondary } },
    yAxis: { type: 'value', scale: true, name: currency, axisLabel: { color: token.colorTextSecondary }, splitLine: { lineStyle: { color: token.colorBorderSecondary } } },
    dataZoom: [{ type: 'slider', bottom: 5, height: 25, filterMode: 'none' }, { type: 'inside', filterMode: 'none', zoomOnMouseWheel: 'ctrl' }],
    series: series.map(curve => ({ name: `试验 #${curve.ordinal}`, type: 'line', data: curve.coordinates, connectNulls: false, smooth: false, showSymbol: false, lineStyle: { width: 2 } })),
  }} /><p className="quiet">仅展示原执行已产生的账户权益，不触发新计算。金额读数保留原始精度，曲线使用绘图坐标。</p></>;
}

export function ParameterTrend({ parameter, metric, points }: { parameter: string; metric: string; points: { ordinal: string; x: string; value: number | null; reason: string | null }[] }) {
  const { token } = theme.useToken();
  const description = `${parameter} 的一维参数趋势；${metric}；仅当前结果页，未执行及无值不补零。`;
  return <section className="result-section"><h3>当前结果页的一维趋势</h3>
    {!points.some(point => point.value !== null) ? <Empty title="当前结果页尚无有效目标指标" /> : <ScanChart description={description} option={{
      animation: false, aria: { enabled: true, label: { description } }, textStyle: { color: token.colorText },
      grid: { top: 30, left: 90, right: 25, bottom: 85 },
      tooltip: { confine: true, renderMode: 'richText', backgroundColor: token.colorBgContainer, borderColor: token.colorBorderSecondary, textStyle: { color: token.colorText }, formatter: (value: unknown) => { const item = value as { dataIndex: number }; const point = points[item.dataIndex]; return point ? `试验 #${point.ordinal}\n${parameter}  ${point.x}\n${metric}  ${point.value ?? '不可用'}${point.reason ? `\n${point.reason}` : ''}` : ''; } },
      xAxis: { type: 'category', name: parameter, nameLocation: 'middle', nameGap: 50, data: points.map(point => point.x), axisLabel: { color: token.colorTextSecondary, rotate: points.length > 12 ? 40 : 0 } },
      yAxis: { type: 'value', scale: true, axisLabel: { color: token.colorTextSecondary }, splitLine: { lineStyle: { color: token.colorBorderSecondary } } },
      series: [{ type: 'line', name: metric, data: points.map(point => point.value), connectNulls: false, smooth: false, showSymbol: true, lineStyle: { color: token.colorPrimary }, itemStyle: { color: token.colorPrimary } }],
    }} />}
    <p className="quiet">当前结果页，随分页查看；未执行及无值不补零。横轴保留原参数文本，并按原序号排列；当前页不代表完整参数域。</p>
  </section>;
}
