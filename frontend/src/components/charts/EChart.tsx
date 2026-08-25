import { BarChart, HeatmapChart, LineChart } from 'echarts/charts';
import { GridComponent, LegendComponent, TooltipComponent, VisualMapComponent } from 'echarts/components';
import * as echarts from 'echarts/core';
import type { EChartsCoreOption } from 'echarts/core';
import { CanvasRenderer } from 'echarts/renderers';
import { useEffect, useRef } from 'react';
import { useI18n, type Locale } from '../../i18n';

echarts.use([BarChart, HeatmapChart, LineChart, GridComponent, LegendComponent, TooltipComponent, VisualMapComponent, CanvasRenderer]);

type OptionRecord = Record<string, unknown>;

function isRecord(value: unknown): value is OptionRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function formatEChartNumber(locale: Locale, value: unknown): string {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return new Intl.NumberFormat(locale, { maximumSignificantDigits: 12 }).format(value);
  }
  if (Array.isArray(value)) return value.map((item) => formatEChartNumber(locale, item)).join(' · ');
  if (value === null || value === undefined) return '—';
  return String(value);
}

function localizeValueAxis(axis: unknown, locale: Locale): unknown {
  if (Array.isArray(axis)) return axis.map((item) => localizeValueAxis(item, locale));
  if (!isRecord(axis) || axis.type !== 'value') return axis;
  const axisLabel = isRecord(axis.axisLabel) ? axis.axisLabel : {};
  if ('formatter' in axisLabel) return axis;
  return {
    ...axis,
    axisLabel: {
      ...axisLabel,
      formatter: (value: unknown) => formatEChartNumber(locale, value),
    },
  };
}

function localizeTooltip(tooltip: unknown, locale: Locale): unknown {
  if (Array.isArray(tooltip)) return tooltip.map((item) => localizeTooltip(item, locale));
  if (!isRecord(tooltip) || 'formatter' in tooltip || 'valueFormatter' in tooltip) return tooltip;
  return {
    ...tooltip,
    valueFormatter: (value: unknown) => formatEChartNumber(locale, value),
  };
}

function localizeVisualMap(visualMap: unknown, locale: Locale): unknown {
  if (Array.isArray(visualMap)) return visualMap.map((item) => localizeVisualMap(item, locale));
  if (!isRecord(visualMap) || 'formatter' in visualMap) return visualMap;
  return {
    ...visualMap,
    formatter: (value: unknown, value2?: unknown) => value2 === undefined
      ? formatEChartNumber(locale, value)
      : `${formatEChartNumber(locale, value)} – ${formatEChartNumber(locale, value2)}`,
  };
}

export function localizeEChartOption(option: EChartsCoreOption, locale: Locale): EChartsCoreOption {
  const source = option as OptionRecord;
  const localized: OptionRecord = { ...source };
  if ('xAxis' in source) localized.xAxis = localizeValueAxis(source.xAxis, locale);
  if ('yAxis' in source) localized.yAxis = localizeValueAxis(source.yAxis, locale);
  if ('tooltip' in source) localized.tooltip = localizeTooltip(source.tooltip, locale);
  if ('visualMap' in source) localized.visualMap = localizeVisualMap(source.visualMap, locale);
  return localized as EChartsCoreOption;
}

export function EChart({ option, ariaLabel, height = 300 }: { option: EChartsCoreOption; ariaLabel: string; height?: number }) {
  const ref = useRef<HTMLDivElement>(null);
  const { locale, text } = useI18n();
  useEffect(() => {
    if (!ref.current) return;
    const chart = echarts.init(ref.current, undefined, { renderer: 'canvas' });
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    chart.setOption({ animationDuration: reduced ? 0 : 180, ...localizeEChartOption(option, locale) });
    const observer = new ResizeObserver(() => chart.resize());
    observer.observe(ref.current);
    return () => { observer.disconnect(); chart.dispose(); };
  }, [locale, option]);
  return <div ref={ref} className="qz-chart" style={{ height }} role="img" aria-label={text(ariaLabel)} />;
}
