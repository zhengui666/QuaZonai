import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { formatCapitalContextValue } from '../pages/AdministrationPage';
import { formatPlainDecimalString } from '../lib/format';
import { renderApp } from './testUtils';

function CapitalValue({ value }: { value: number | string }) {
  return <div>{formatCapitalContextValue(value)}</div>;
}

describe('Administration capital context', () => {
  it('preserves a frozen fractional capital value with locale separators', () => {
    renderApp(<CapitalValue value="0.0004" />, { locale: 'es' });

    expect(screen.getByText(new Intl.NumberFormat('es', { maximumSignificantDigits: 21 }).format(0.0004))).toBeInTheDocument();
  });

  it('preserves exact integer strings beyond the safe-integer range', () => {
    renderApp(<CapitalValue value="9007199254740993" />, { locale: 'en' });

    expect(screen.getByText('9,007,199,254,740,993')).toBeInTheDocument();
  });

  it('expands high-precision scientific capital strings without rounding', () => {
    renderApp(<CapitalValue value="9.007199254740993e+15" />, { locale: 'en' });

    expect(screen.getByText('9,007,199,254,740,993')).toBeInTheDocument();
  });

  it('localizes negative scientific capital strings while preserving trailing precision', () => {
    renderApp(<CapitalValue value="-1.2300E-4" />, { locale: 'ar' });

    expect(screen.getByText(formatPlainDecimalString('-0.00012300', 'ar')!)).toBeInTheDocument();
  });
});
