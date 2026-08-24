import { useEffect, useRef } from 'react';
import { CandlestickSeries, ColorType, HistogramSeries, createChart, type CandlestickData, type HistogramData, type Time } from 'lightweight-charts';
import { useI18n } from '../../i18n';
import type { OhlcPoint } from '../../lib/api/types';

function toTime(value: string | number): Time {
  if (typeof value === 'number') return value as Time;
  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) return value as Time;
  return Math.floor(new Date(value).getTime() / 1000) as Time;
}

export function CandlestickChart({ data }: { data: OhlcPoint[] }) {
  const { t } = useI18n();
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!ref.current || data.length === 0) return;
    const styles = getComputedStyle(document.documentElement);
    const chart = createChart(ref.current, {
      autoSize: true,
      height: 360,
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
    tooltip.hidden = true;
    ref.current.appendChild(tooltip);
    chart.subscribeCrosshairMove((param) => {
      if (!param.time || !param.point || param.point.x < 0 || param.point.y < 0) { tooltip.hidden = true; return; }
      const value = param.seriesData.get(candle) as { open?: number; high?: number; low?: number; close?: number } | undefined;
      if (typeof value?.close !== 'number') { tooltip.hidden = true; return; }
      tooltip.textContent = `${String(param.time)} · O ${value.open?.toFixed(4)} · H ${value.high?.toFixed(4)} · L ${value.low?.toFixed(4)} · C ${value.close.toFixed(4)}`;
      tooltip.hidden = false;
      tooltip.style.left = `${Math.min(param.point.x + 12, Math.max(8, ref.current!.clientWidth - 310))}px`;
      tooltip.style.top = `${Math.max(8, param.point.y - 34)}px`;
    });
    chart.timeScale().fitContent();
    return () => { tooltip.remove(); chart.remove(); };
  }, [data]);

  return <div ref={ref} className="qz-chart-host qz-chart-tall" role="img" aria-label={`${t('research.marketContext')} · OHLC`} />;
}
