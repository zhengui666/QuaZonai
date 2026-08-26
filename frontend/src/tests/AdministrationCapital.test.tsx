import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { formatCapitalContextValue } from '../pages/AdministrationPage';
import { renderApp } from './testUtils';

function CapitalValue({ value }: { value: number | string }) {
  return <div>{formatCapitalContextValue(value)}</div>;
}

describe('Administration capital context', () => {
  it('preserves a frozen fractional capital value with locale separators', () => {
    renderApp(<CapitalValue value="0.0004" />, { locale: 'es' });

    expect(screen.getByText(new Intl.NumberFormat('es', { maximumSignificantDigits: 21 }).format(0.0004))).toBeInTheDocument();
  });
});
