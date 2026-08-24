import { AreaSeries, ColorType, LineSeries, createChart, type AreaData, type ISeriesApi, type LineData, type Time } from 'lightweight-charts';
import { useEffect, useRef } from 'react';
import type { TimeValuePoint } from '../../lib/metrics';

export interface FinancialSeries { name: string; data: TimeValuePoint[]; kind?: 'line' | 'area'; }

function toTime(value: string | number): Time {
  if (typeof value === 'number') return value as Time;
  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) return value as Time;
  return Math.floor(new Date(value).getTime() / 1000) as Time;
}

export function FinancialSeriesChart({ series, ariaLabel, height = 320 }: { series: FinancialSeries[]; ariaLabel: string; height?: number }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!ref.current || series.every((item) => item.data.length === 0)) return;
    const styles = getComputedStyle(document.documentElement);
    const border = styles.getPropertyValue('--qz-border').trim();
    const muted = styles.getPropertyValue('--qz-text-faint').trim();
    const chart = createChart(ref.current, {
      autoSize: true,
      height,
      layout: { background: { type: ColorType.Solid, color: 'transparent' }, textColor: muted, fontSize: 10 },
      grid: { vertLines: { color: border }, horzLines: { color: border } },
      rightPriceScale: { borderColor: border },
      timeScale: { borderColor: border, timeVisible: true, secondsVisible: false },
      crosshair: { mode: 0 },
    });
    const colors = ['#4f9b82', '#8a9692', '#b08a56', '#7c8fa6'];
    const handles: Array<{ name: string; api: ISeriesApi<'Line'> | ISeriesApi<'Area'> }> = [];
    series.forEach((item, index) => {
      if (!item.data.length) return;
      if (item.kind === 'area') {
        const api = chart.addSeries(AreaSeries, { lineColor: colors[index % colors.length], topColor: 'rgba(79,155,130,.20)', bottomColor: 'rgba(79,155,130,.02)', lineWidth: 2 });
        api.setData(item.data.map((point) => ({ time: toTime(point.time), value: point.value })) as AreaData<Time>[]);
        handles.push({ name: item.name, api });
      } else {
        const api = chart.addSeries(LineSeries, { color: colors[index % colors.length], lineWidth: index === 0 ? 2 : 1 });
        api.setData(item.data.map((point) => ({ time: toTime(point.time), value: point.value })) as LineData<Time>[]);
        handles.push({ name: item.name, api });
      }
    });
    const tooltip = document.createElement('div');
    tooltip.className = 'qz-chart-tooltip';
    tooltip.hidden = true;
    ref.current.appendChild(tooltip);
    chart.subscribeCrosshairMove((param) => {
      if (!param.time || !param.point || param.point.x < 0 || param.point.y < 0) { tooltip.hidden = true; return; }
      const values = handles.flatMap(({ name, api }) => {
        const datum = param.seriesData.get(api) as { value?: number } | undefined;
        return typeof datum?.value === 'number' ? [`${name}: ${datum.value.toFixed(4)}`] : [];
      });
      tooltip.textContent = `${String(param.time)}${values.length ? ` · ${values.join(' · ')}` : ''}`;
      tooltip.hidden = false;
      tooltip.style.left = `${Math.min(param.point.x + 12, Math.max(8, ref.current!.clientWidth - 260))}px`;
      tooltip.style.top = `${Math.max(8, param.point.y - 34)}px`;
    });
    chart.timeScale().fitContent();
    return () => chart.remove();
  }, [ariaLabel, height, series]);
  return <div ref={ref} className="qz-chart-host" style={{ minHeight: height }} role="img" aria-label={ariaLabel} />;
}
