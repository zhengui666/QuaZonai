import type { EChartsCoreOption } from 'echarts/core';
import { describe, expect, it } from 'vitest';
import { compactEChartOption, formatEChartNumber, localizeEChartOption } from '../components/charts/EChart';

type UnknownRecord = Record<string, unknown>;

function record(value: unknown): UnknownRecord {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new Error('Expected an object');
  return value as UnknownRecord;
}

describe('EChart locale formatting', () => {
  it('caps desktop chart margins in compact mode', () => {
    const option = {
      grid: { left: 130, right: '96px', top: 10, bottom: 24 },
      series: [],
    } as EChartsCoreOption;

    const compact = record(compactEChartOption(option, true));
    expect(record(compact.grid)).toMatchObject({ left: 72, right: '72px', containLabel: true });
    expect(record(option).grid).toEqual({ left: 130, right: '96px', top: 10, bottom: 24 });
  });

  it('formats numeric values with the requested locale and preserves tuple values', () => {
    const numberFormat = new Intl.NumberFormat('es', { maximumSignificantDigits: 15 });
    expect(formatEChartNumber('es', 1234.5)).toBe(numberFormat.format(1234.5));
    expect(formatEChartNumber('es', [0.25, 1234.5])).toBe(`${numberFormat.format(0.25)} · ${numberFormat.format(1234.5)}`);
    const preciseValue = 0.123456789012345;
    expect(formatEChartNumber('es', preciseValue)).toBe(numberFormat.format(preciseValue));
    expect(formatEChartNumber('es', preciseValue)).not.toBe(
      new Intl.NumberFormat('es', { maximumSignificantDigits: 12 }).format(preciseValue),
    );
  });

  it('adds locale-aware value-axis, tooltip, and visual-map formatters without mutating the caller option', () => {
    const option = {
      xAxis: { type: 'value' },
      yAxis: [{ type: 'category', data: ['A'] }, { type: 'value' }],
      tooltip: { trigger: 'axis' },
      visualMap: { min: 0, max: 1 },
      series: [],
    } as EChartsCoreOption;

    const localized = record(localizeEChartOption(option, 'es'));
    const preciseValue = 0.123456789012345;
    const expected = new Intl.NumberFormat('es', { maximumSignificantDigits: 15 }).format(preciseValue);
    const xAxis = record(localized.xAxis);
    const xAxisLabel = record(xAxis.axisLabel);
    expect((xAxisLabel.formatter as (value: unknown) => string)(preciseValue)).toBe(expected);

    const yAxes = localized.yAxis as unknown[];
    expect(record(yAxes[0]).axisLabel).toBeUndefined();
    const valueAxisLabel = record(record(yAxes[1]).axisLabel);
    expect((valueAxisLabel.formatter as (value: unknown) => string)(preciseValue)).toBe(expected);

    const tooltip = record(localized.tooltip);
    expect((tooltip.valueFormatter as (value: unknown) => string)(preciseValue)).toBe(expected);

    const visualMap = record(localized.visualMap);
    expect((visualMap.formatter as (value: unknown, value2?: unknown) => string)(preciseValue, 0.5)).toBe(
      `${expected} – ${new Intl.NumberFormat('es', { maximumSignificantDigits: 15 }).format(0.5)}`,
    );

    expect(record(record(option).xAxis).axisLabel).toBeUndefined();
    expect(record(option).tooltip).toEqual({ trigger: 'axis' });
    expect(record(option).visualMap).toEqual({ min: 0, max: 1 });
  });

  it('preserves explicit custom formatters and produces different output after a locale change', () => {
    const axisFormatter = (value: unknown) => `axis:${String(value)}`;
    const tooltipFormatter = (value: unknown) => `tooltip:${String(value)}`;
    const visualMapFormatter = (value: unknown) => `map:${String(value)}`;
    const option = {
      xAxis: { type: 'value', axisLabel: { formatter: axisFormatter } },
      tooltip: { formatter: tooltipFormatter },
      visualMap: { formatter: visualMapFormatter },
      series: [],
    } as EChartsCoreOption;

    const localized = record(localizeEChartOption(option, 'ar'));
    expect(record(record(localized.xAxis).axisLabel).formatter).toBe(axisFormatter);
    expect(record(localized.tooltip).formatter).toBe(tooltipFormatter);
    expect(record(localized.visualMap).formatter).toBe(visualMapFormatter);

    expect(formatEChartNumber('es', 1234.5)).not.toBe(formatEChartNumber('ar', 1234.5));
  });
});
