import { render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FinancialSeriesChart, formatFinancialTooltipValue } from '../components/charts/FinancialSeriesChart';
import { I18nProvider } from '../i18n';

const chartMocks = vi.hoisted(() => ({ createChart: vi.fn(), subscribeCrosshairMove: vi.fn() }));

let seriesApis: Array<{ setData: () => void }>;

vi.mock('lightweight-charts', () => ({
  AreaSeries: {},
  ColorType: { Solid: 'solid' },
  LineSeries: {},
  createChart: chartMocks.createChart,
}));

describe('FinancialSeriesChart lifecycle', () => {
  beforeEach(() => {
    seriesApis = [];
    chartMocks.createChart.mockImplementation(() => ({
      addSeries: vi.fn(() => {
        const api = { setData: vi.fn() };
        seriesApis.push(api);
        return api;
      }),
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

  it('removes its custom tooltip before recreating a chart for refreshed series', () => {
    const firstSeries = [{ name: 'Portfolio', data: [{ time: '2026-08-25', value: 1 }] }];
    const secondSeries = [{ name: 'Portfolio', data: [{ time: '2026-08-25', value: 2 }] }];
    const view = render(
      <I18nProvider initialLocale="en">
        <FinancialSeriesChart ariaLabel="Portfolio chart" series={firstSeries} />
      </I18nProvider>,
    );

    expect(view.container.querySelectorAll('.qz-chart-tooltip')).toHaveLength(1);

    view.rerender(
      <I18nProvider initialLocale="en">
        <FinancialSeriesChart ariaLabel="Portfolio chart" series={secondSeries} />
      </I18nProvider>,
    );

    expect(view.container.querySelectorAll('.qz-chart-tooltip')).toHaveLength(1);
  });
  it('isolates timestamp and mixed-direction series fields in RTL tooltips', () => {
    const values = [0.00004, 1.25];
    const view = render(
      <I18nProvider initialLocale="ar">
        <FinancialSeriesChart
          ariaLabel="Financial chart"
          series={[
            { name: 'EUR/USD', data: [{ time: '2026-08-25', value: values[0] }] },
            { name: 'مؤشر', data: [{ time: '2026-08-25', value: values[1] }] },
          ]}
        />
      </I18nProvider>,
    );

    const handler = chartMocks.subscribeCrosshairMove.mock.calls[0]?.[0] as ((param: unknown) => void) | undefined;
    expect(handler).toBeTypeOf('function');
    if (!handler) throw new Error('Expected FinancialSeriesChart to register a crosshair handler');

    handler({
      time: '2026-08-25',
      point: { x: 20, y: 20 },
      seriesData: new Map([
        [seriesApis[0], { value: values[0] }],
        [seriesApis[1], { value: values[1] }],
      ]),
    });

    const tooltip = view.container.querySelector<HTMLDivElement>('.qz-chart-tooltip');
    expect(tooltip).not.toBeNull();
    expect(tooltip).toHaveAttribute('dir', 'ltr');

    const fields = Array.from(tooltip!.querySelectorAll('bdi'));
    expect(fields).toHaveLength(3);
    expect(fields.map((field) => field.getAttribute('dir'))).toEqual(['auto', 'auto', 'auto']);
    expect(fields[1]).toHaveTextContent(`EUR/USD: ${formatFinancialTooltipValue('ar', values[0])}`);
    expect(fields[2]).toHaveTextContent(`مؤشر: ${formatFinancialTooltipValue('ar', values[1])}`);
  });
});
