import { Collapse, Table, theme } from 'antd';
import { useMemo, useState } from 'react';
import ReactEChartsCore from 'echarts-for-react/lib/core';
import * as echarts from 'echarts/core';
import { LineChart } from 'echarts/charts';
import type { LineSeriesOption } from 'echarts/charts';
import { AriaComponent, DataZoomComponent, GridComponent, TooltipComponent } from 'echarts/components';
import type { AriaComponentOption, DataZoomComponentOption, GridComponentOption, TooltipComponentOption } from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';
import { equityCoordinates, equityTime, equityTimeText } from './equity-data';
import type { EquityPoint, EquitySeries } from './equity-data';

// There is one renderer and one chart type; no all-in-one ECharts import.
echarts.use([LineChart, GridComponent, TooltipComponent, DataZoomComponent, AriaComponent, CanvasRenderer]);
type Option = echarts.ComposeOption<LineSeriesOption | GridComponentOption | TooltipComponentOption | DataZoomComponentOption | AriaComponentOption>;

function zoomValues(event: unknown): { start: number; end: number } | undefined {
  if (!event || typeof event !== 'object') return;
  if ('batch' in event && Array.isArray(event.batch)) return zoomValues(event.batch[0]);
  if (!('start' in event) || !('end' in event) || typeof event.start !== 'number' || typeof event.end !== 'number') return;
  if (!Number.isFinite(event.start) || !Number.isFinite(event.end) || event.start < 0 || event.end > 100 || event.start > event.end) return;
  return { start: event.start, end: event.end };
}

export default function EquityPlot({ series }: { series: EquitySeries }) {
  const { token } = theme.useToken();
  const [zoom, setZoom] = useState({ start: 0, end: 100 });
  const data = useMemo(() => series.points.map(equityCoordinates), [series]);
  const onEvents = useMemo(() => ({ datazoom: (event: unknown) => {
    const value = zoomValues(event);
    if (value) setZoom(previous => previous.start === value.start && previous.end === value.end ? previous : value);
  } }), []);
  const option = useMemo<Option>(() => ({
    animation: false,
    useUTC: true,
    backgroundColor: token.colorBgContainer,
    textStyle: { color: token.colorText, fontFamily: token.fontFamily },
    aria: { enabled: true, label: { description: `组合价值，${series.base_currency}，UTC。下方明细提供原始时间和金额。` } },
    grid: { left: 20, right: 20, top: 24, bottom: 90, containLabel: true },
    xAxis: { type: 'time', boundaryGap: [0, 0], axisLine: { lineStyle: { color: token.colorBorder } }, axisLabel: { hideOverlap: true, color: token.colorTextSecondary } },
    yAxis: { type: 'value', scale: true, axisLabel: { color: token.colorTextSecondary }, splitLine: { lineStyle: { color: token.colorSplit } } },
    tooltip: {
      trigger: 'axis', confine: true, renderMode: 'richText',
      backgroundColor: token.colorBgElevated, borderColor: token.colorBorder,
      textStyle: { color: token.colorText },
      formatter: params => {
        const item = Array.isArray(params) ? params[0] : params;
        const point = item && series.points[item.dataIndex];
        return point ? `${equityTimeText(point.timestamp_ns)}\n${point.value ?? '缺值'} ${series.base_currency}` : '';
      },
    },
    dataZoom: [
      { type: 'inside', start: zoom.start, end: zoom.end, zoomOnMouseWheel: 'ctrl', moveOnMouseMove: false, moveOnMouseWheel: false, preventDefaultMouseMove: false, filterMode: 'none' },
      { type: 'slider', start: zoom.start, end: zoom.end, bottom: 12, height: 44, showDetail: false, filterMode: 'none', borderColor: token.colorBorder, textStyle: { color: token.colorTextSecondary } },
    ],
    series: [{ id: 'equity', name: '组合价值', type: 'line', data, smooth: false, connectNulls: false,
      showSymbol: series.points.length === 1, symbolSize: 8, lineStyle: { width: 2, color: token.colorPrimary }, itemStyle: { color: token.colorPrimary } }],
  }), [data, series, token, zoom]);
  const visible = useMemo(() => {
    if (!series.points.length) return [];
    const first = equityTime(series.points[0]!.timestamp_ns);
    const last = equityTime(series.points[series.points.length - 1]!.timestamp_ns);
    const start = first + (last - first) * zoom.start / 100;
    const end = first + (last - first) * zoom.end / 100;
    return series.points.filter(point => { const time = equityTime(point.timestamp_ns); return time >= start && time <= end; });
  }, [series, zoom]);
  return <>
    <div data-testid="equity-chart" style={{ width: '100%', minWidth: 0 }}>
      <ReactEChartsCore echarts={echarts} option={option} onEvents={onEvents}
        opts={{ renderer: 'canvas' }} style={{ height: 360, width: '100%' }} />
    </div>
    <Collapse items={[{ key: 'points', label: `明细（${visible.length}）`, children:
      <Table<EquityPoint> rowKey="timestamp_ns" dataSource={visible} size="small" scroll={{ x: 560 }}
        pagination={{ pageSize: 50, showSizeChanger: false }} columns={[
          { title: '时间（UTC）', dataIndex: 'timestamp_ns', render: equityTimeText },
          { title: `价值（${series.base_currency}）`, dataIndex: 'value', render: (value: string | null) => value ?? '缺值' },
        ]} />,
    }]} />
  </>;
}
