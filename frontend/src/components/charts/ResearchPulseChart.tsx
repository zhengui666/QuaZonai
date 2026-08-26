import { useEffect, useRef } from 'react';
import * as echarts from 'echarts/core';
import { BarChart, LineChart } from 'echarts/charts';
import { GridComponent, TooltipComponent } from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';
import { useI18n } from '../../i18n';
import { humanize } from '../../lib/format';
import { localizeEChartOption } from './EChart';

echarts.use([BarChart, LineChart, GridComponent, TooltipComponent, CanvasRenderer]);

export interface PulsePoint {
  label: string;
  active: number;
  cooling: number;
  evidence: number;
}

export function ResearchPulseChart({ data }: { data: PulsePoint[] }) {
  const { t, locale } = useI18n();
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!ref.current) return;
    const chart = echarts.init(ref.current, undefined, { renderer: 'canvas' });
    const styles = getComputedStyle(document.documentElement);
    const muted = styles.getPropertyValue('--qz-text-faint').trim();
    const border = styles.getPropertyValue('--qz-border').trim();
    const accent = styles.getPropertyValue('--qz-accent').trim();
    const warning = styles.getPropertyValue('--qz-warning').trim();
    const info = styles.getPropertyValue('--qz-info').trim();

    chart.setOption(localizeEChartOption({
      animationDuration: matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 250,
      grid: { top: 18, left: 36, right: 16, bottom: 28 },
      tooltip: {
        trigger: 'axis',
        backgroundColor: '#111916',
        borderColor: border,
        textStyle: { color: '#dfeae6', fontSize: 11 },
      },
      xAxis: {
        type: 'category',
        data: data.map((point) => point.label),
        axisLine: { lineStyle: { color: border } },
        axisTick: { show: false },
        axisLabel: { color: muted, fontSize: 10 },
      },
      yAxis: {
        type: 'value',
        axisLine: { show: false },
        splitLine: { lineStyle: { color: border } },
        axisLabel: { color: muted, fontSize: 10 },
      },
      series: [
        {
          name: humanize('ACTIVE'),
          type: 'bar',
          stack: 'programs',
          data: data.map((point) => point.active),
          itemStyle: { color: accent },
          barMaxWidth: 22,
        },
        {
          name: humanize('COOLING'),
          type: 'bar',
          stack: 'programs',
          data: data.map((point) => point.cooling),
          itemStyle: { color: warning },
        },
        {
          name: t('alpha.evidence'),
          type: 'line',
          yAxisIndex: 0,
          data: data.map((point) => point.evidence),
          symbol: 'none',
          lineStyle: { color: info, width: 1.5 },
        },
      ],
    }, locale));

    const observer = new ResizeObserver(() => chart.resize());
    observer.observe(ref.current);
    return () => {
      observer.disconnect();
      chart.dispose();
    };
  }, [data, locale, t]);

  return <div ref={ref} className="qz-chart" role="img" aria-label={t('home.researchPulse')} />;
}
