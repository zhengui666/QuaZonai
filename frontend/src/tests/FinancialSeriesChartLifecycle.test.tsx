import { render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FinancialSeriesChart } from '../components/charts/FinancialSeriesChart';
import { I18nProvider } from '../i18n';

const chartMocks = vi.hoisted(() => ({ createChart: vi.fn() }));

vi.mock('lightweight-charts', () => ({
  AreaSeries: {},
  ColorType: { Solid: 'solid' },
  LineSeries: {},
  createChart: chartMocks.createChart,
}));

describe('FinancialSeriesChart lifecycle', () => {
  beforeEach(() => {
    chartMocks.createChart.mockImplementation(() => ({
      addSeries: vi.fn(() => ({ setData: vi.fn() })),
      subscribeCrosshairMove: vi.fn(),
      timeScale: () => ({ fitContent: vi.fn() }),
      remove: vi.fn(),
    }));
  });

  afterEach(() => {
    chartMocks.createChart.mockReset();
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
});
