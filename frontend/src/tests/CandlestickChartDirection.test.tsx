import { render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CandlestickChart, formatCandlestickTooltipValue } from '../components/charts/CandlestickChart';
import { I18nProvider } from '../i18n';

const chartMocks = vi.hoisted(() => ({
  createChart: vi.fn(),
  subscribeCrosshairMove: vi.fn(),
}));

vi.mock('lightweight-charts', () => ({
  CandlestickSeries: {},
  ColorType: { Solid: 'solid' },
  HistogramSeries: {},
  createChart: chartMocks.createChart,
}));

let candle: { setData: () => void };

describe('CandlestickChart direction', () => {
  beforeEach(() => {
    candle = { setData: vi.fn() };
    chartMocks.createChart.mockImplementation(() => ({
      addSeries: vi.fn(() => candle),
      subscribeCrosshairMove: chartMocks.subscribeCrosshairMove,
      timeScale: () => ({ fitContent: vi.fn() }),
      remove: vi.fn(),
    }));
  });

  afterEach(() => {
    chartMocks.createChart.mockReset();
    chartMocks.subscribeCrosshairMove.mockReset();
    document.querySelectorAll('.qz-chart-tooltip').forEach((tooltip) => tooltip.remove());
  });

  it('keeps each OHLC field isolated and ordered in RTL locales', () => {
    const values = { open: 0.00004, high: 0.00006, low: 0.00002, close: 0.00005 };
    const view = render(
      <I18nProvider initialLocale="ar">
        <CandlestickChart data={[{ time: '2026-08-25', ...values }]} />
      </I18nProvider>,
    );

    const handler = chartMocks.subscribeCrosshairMove.mock.calls[0]?.[0] as ((param: unknown) => void) | undefined;
    expect(handler).toBeTypeOf('function');
    if (!handler) throw new Error('Expected CandlestickChart to register a crosshair handler');

    handler({
      time: '2026-08-25',
      point: { x: 20, y: 20 },
      seriesData: new Map([[candle, values]]),
    });

    const tooltip = view.container.querySelector<HTMLDivElement>('.qz-chart-tooltip');
    expect(tooltip).not.toBeNull();
    expect(tooltip).toHaveAttribute('dir', 'ltr');

    const fields = Array.from(tooltip!.querySelectorAll('bdi'));
    expect(fields).toHaveLength(5);
    expect(fields[0]).toHaveAttribute('dir', 'auto');
    expect(fields.slice(1).map((field) => field.getAttribute('dir'))).toEqual(['ltr', 'ltr', 'ltr', 'ltr']);
    expect(fields.slice(1).map((field) => field.textContent)).toEqual([
      `O ${formatCandlestickTooltipValue('ar', values.open)}`,
      `H ${formatCandlestickTooltipValue('ar', values.high)}`,
      `L ${formatCandlestickTooltipValue('ar', values.low)}`,
      `C ${formatCandlestickTooltipValue('ar', values.close)}`,
    ]);
  });
});
