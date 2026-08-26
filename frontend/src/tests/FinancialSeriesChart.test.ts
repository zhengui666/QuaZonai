import { describe, expect, it } from 'vitest';
import { formatCandlestickTooltipTime, formatCandlestickTooltipValue } from '../components/charts/CandlestickChart';
import { formatFinancialTooltipTime, formatFinancialTooltipValue } from '../components/charts/FinancialSeriesChart';

describe('FinancialSeriesChart tooltip formatting', () => {
  it('formats numeric values with the active locale', () => {
    expect(formatFinancialTooltipValue('es', 1234.5)).toBe(
      new Intl.NumberFormat('es', { maximumSignificantDigits: 15 }).format(1234.5),
    );
  });

  it('preserves small values instead of rounding them to zero', () => {
    const value = 0.00004;
    expect(formatFinancialTooltipValue('es', value)).toBe(
      new Intl.NumberFormat('es', { maximumSignificantDigits: 15 }).format(value),
    );
    expect(formatFinancialTooltipValue('es', value)).not.toBe(
      new Intl.NumberFormat('es', { minimumFractionDigits: 4, maximumFractionDigits: 4 }).format(value),
    );
  });
  it('formats business-day timestamps with the active locale', () => {
    const expected = new Intl.DateTimeFormat('es', { year: 'numeric', month: 'short', day: '2-digit', timeZone: 'UTC' })
      .format(new Date(Date.UTC(2026, 7, 25)));
    expect(formatFinancialTooltipTime('es', '2026-08-25')).toBe(expected);
  });
});

describe('CandlestickChart tooltip formatting', () => {
  it('formats OHLC values with the active locale', () => {
    expect(formatCandlestickTooltipValue('ar', 1234.5)).toBe(
      new Intl.NumberFormat('ar', { maximumSignificantDigits: 15 }).format(1234.5),
    );
  });

  it('preserves small OHLC values instead of rounding them to zero', () => {
    const value = 0.00004;
    expect(formatCandlestickTooltipValue('ar', value)).toBe(
      new Intl.NumberFormat('ar', { maximumSignificantDigits: 15 }).format(value),
    );
    expect(formatCandlestickTooltipValue('ar', value)).not.toBe(
      new Intl.NumberFormat('ar', { minimumFractionDigits: 4, maximumFractionDigits: 4 }).format(value),
    );
  });
  it('formats business-day timestamps with the active locale', () => {
    const expected = new Intl.DateTimeFormat('es', { year: 'numeric', month: 'short', day: '2-digit', timeZone: 'UTC' })
      .format(new Date(Date.UTC(2026, 7, 25)));
    expect(formatCandlestickTooltipTime('es', '2026-08-25')).toBe(expected);
  });
});
