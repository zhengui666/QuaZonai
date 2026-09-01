import { useEffect, useRef } from 'react';
import { CandlestickSeries, ColorType, HistogramSeries, createChart, type CandlestickData, type HistogramData, type Time } from 'lightweight-charts';
import { useI18n, type Locale } from '../../i18n';
import type { OhlcPoint } from '../../lib/api/types';
import { useResponsiveViewport } from '../../lib/useMediaQuery';

function toTime(value: string | number): Time {
  if (typeof value === 'number') return value as Time;
  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) return value as Time;
  return Math.floor(new Date(value).getTime() / 1000) as Time;
}

export function formatCandlestickTooltipValue(locale: Locale, value: number): string {
  return new Intl.NumberFormat(locale, { maximumSignificantDigits: 15 }).format(value);
}

export function formatCandlestickTooltipTime(locale: Locale, time: Time): string {
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

export function CandlestickChart({ data }: { data: OhlcPoint[] }) {
  const { locale, t } = useI18n();
  const { isPhone } = useResponsiveViewport();
  const chartHeight = isPhone ? 260 : 360;
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!ref.current || data.length === 0) return;
    const styles = getComputedStyle(document.documentElement);
    const chart = createChart(ref.current, {
      localization: {
        locale,
        priceFormatter: (value: number) => formatCandlestickTooltipValue(locale, value),
        timeFormatter: (time: Time) => formatCandlestickTooltipTime(locale, time),
      },
      autoSize: true,
      height: chartHeight,
      layout: { background: { type: ColorType.Solid, color: 'transparent' }, textColor: styles.getPropertyValue('--qz-text-faint').trim(), fontSize: 10 },
      grid: { vertLines: { color: styles.getPropertyValue('--qz-border').trim() }, horzLines: { color: styles.getPropertyValue('--qz-border').trim() } },
      rightPriceScale: { borderColor: styles.getPropertyValue('--qz-border').trim() },
      timeScale: { borderColor: styles.getPropertyValue('--qz-border').trim(), timeVisible: true, secondsVisible: false },
      crosshair: { mode: 0 },
    });
    const candle = chart.addSeries(CandlestickSeries, {
      upColor: '#32b79a',
      downColor: '#ef6c71',
      borderUpColor: '#32b79a',
      borderDownColor: '#ef6c71',
      wickUpColor: '#54d1b3',
      wickDownColor: '#ef6c71',
    });
    candle.setData(data.map((point) => ({ time: toTime(point.time), open: point.open, high: point.high, low: point.low, close: point.close })) as CandlestickData<Time>[]);
    if (data.some((point) => point.volume !== undefined)) {
      const volume = chart.addSeries(HistogramSeries, { priceFormat: { type: 'volume' }, priceScaleId: '' });
      volume.priceScale().applyOptions({ scaleMargins: { top: .82, bottom: 0 } });
      volume.setData(data.filter((point) => point.volume !== undefined).map((point) => ({ time: toTime(point.time), value: point.volume!, color: point.close >= point.open ? 'rgba(50,183,154,.35)' : 'rgba(239,108,113,.35)' })) as HistogramData<Time>[]);
    }
    const tooltip = document.createElement('div');
    tooltip.className = 'qz-chart-tooltip';
    tooltip.dir = 'ltr';
    tooltip.hidden = true;
    ref.current.appendChild(tooltip);
    chart.subscribeCrosshairMove((param) => {
      if (!param.time || !param.point || param.point.x < 0 || param.point.y < 0) { tooltip.hidden = true; return; }
      const value = param.seriesData.get(candle) as { open?: number; high?: number; low?: number; close?: number } | undefined;
      if (typeof value?.close !== 'number') { tooltip.hidden = true; return; }
      const formatValue = (item: number | undefined) => typeof item === 'number' ? formatCandlestickTooltipValue(locale, item) : '—';
      const timestamp = document.createElement('bdi');
      timestamp.dir = 'auto';
      timestamp.textContent = formatCandlestickTooltipTime(locale, param.time);
      tooltip.replaceChildren(timestamp);
      const fields: Array<[string, number | undefined]> = [
        ['O', value.open],
        ['H', value.high],
        ['L', value.low],
        ['C', value.close],
      ];
      for (const [label, item] of fields) {
        const field = document.createElement('bdi');
        field.dir = 'ltr';
        field.textContent = `${label} ${formatValue(item)}`;
        tooltip.append(' · ', field);
      }
      tooltip.hidden = false;
      tooltip.style.left = `${Math.min(param.point.x + 12, Math.max(8, ref.current!.clientWidth - 310))}px`;
      tooltip.style.top = `${Math.max(8, param.point.y - 34)}px`;
    });
    chart.timeScale().fitContent();
    return () => { tooltip.remove(); chart.remove(); };
  }, [chartHeight, data, locale]);

  return <div ref={ref} className="qz-chart-host qz-chart-tall" style={{ height: chartHeight, touchAction: 'pan-x pan-y' }} role="img" aria-label={`${t('research.marketContext')} · OHLC`} />;
}
