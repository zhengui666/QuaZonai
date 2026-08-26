import { AreaSeries, ColorType, LineSeries, createChart, type AreaData, type ISeriesApi, type LineData, type Time } from 'lightweight-charts';
import { useEffect, useMemo, useRef } from 'react';
import { useI18n, type Locale } from '../../i18n';
import type { TimeValuePoint } from '../../lib/metrics';

export interface FinancialSeries { name: string; data: TimeValuePoint[]; kind?: 'line' | 'area'; }

function toTime(value: string | number): Time {
  if (typeof value === 'number') return value as Time;
  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) return value as Time;
  return Math.floor(new Date(value).getTime() / 1000) as Time;
}

export function formatFinancialTooltipValue(locale: Locale, value: number): string {
  return new Intl.NumberFormat(locale, { minimumFractionDigits: 4, maximumFractionDigits: 4 }).format(value);
}

export function formatFinancialTooltipTime(locale: Locale, time: Time): string {
  if (typeof time === 'number') {
    return new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'short', day: '2-digit', hour: '2-digit', minute: '2-digit' }).format(new Date(time * 1000));
  }
  if (typeof time === 'string') {
    const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(time);
    if (!match) return time;
    const [, year, month, day] = match;
    return new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'short', day: '2-digit', timeZone: 'UTC' }).format(new Date(Date.UTC(Number(year), Number(month) - 1, Number(day))));
  }
  const item = time as { year: number; month: number; day: number };
  return new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'short', day: '2-digit', timeZone: 'UTC' }).format(new Date(Date.UTC(item.year, item.month - 1, item.day)));
}

export function FinancialSeriesChart({ series, ariaLabel, height = 320 }: { series: FinancialSeries[]; ariaLabel: string; height?: number }) {
  const ref = useRef<HTMLDivElement>(null);
  const { locale, text } = useI18n();
  const localizedSeries = useMemo(() => series.map((item) => ({ ...item, name: text(item.name) })), [series, text]);
  useEffect(() => {
    if (!ref.current || localizedSeries.every((item) => item.data.length === 0)) return;
    const styles = getComputedStyle(document.documentElement);
    const border = styles.getPropertyValue('--qz-border').trim();
    const muted = styles.getPropertyValue('--qz-text-faint').trim();
    const chart = createChart(ref.current, {
      localization: {
        locale,
        priceFormatter: (value: number) => formatFinancialTooltipValue(locale, value),
        timeFormatter: (time: Time) => formatFinancialTooltipTime(locale, time),
      },
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
    localizedSeries.forEach((item, index) => {
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
        return typeof datum?.value === 'number' ? [`${name}: ${formatFinancialTooltipValue(locale, datum.value)}`] : [];
      });
      tooltip.textContent = `${formatFinancialTooltipTime(locale, param.time)}${values.length ? ` · ${values.join(' · ')}` : ''}`;
      tooltip.hidden = false;
      tooltip.style.left = `${Math.min(param.point.x + 12, Math.max(8, ref.current!.clientWidth - 260))}px`;
      tooltip.style.top = `${Math.max(8, param.point.y - 34)}px`;
    });
    chart.timeScale().fitContent();
    return () => { tooltip.remove(); chart.remove(); };
  }, [height, locale, localizedSeries]);
  return <div ref={ref} className="qz-chart-host" style={{ minHeight: height }} role="img" aria-label={text(ariaLabel)} />;
}
