import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AlphaLibraryPage } from '../pages/AlphaLibraryPage';
import { renderApp } from './testUtils';

vi.mock('../lib/api/hooks', () => ({
  useAlphaLibrary: () => ({
    isLoading: false,
    error: null,
    data: [{
      id: 'alpha-precision',
      name: 'Precision Alpha',
      role: 'PRIMARY_ALPHA',
      state: 'QUALIFIED',
      metrics: { ic: 0.0004 },
    }],
  }),
}));

describe('Alpha library evidence metrics', () => {
  it('preserves small nonzero evidence metrics with locale-aware precision', () => {
    renderApp(<AlphaLibraryPage />, { locale: 'es' });
    const precise = new Intl.NumberFormat('es', { maximumSignificantDigits: 15 }).format(0.0004);
    expect(screen.getByText(precise)).toBeInTheDocument();
    expect(precise).not.toBe(new Intl.NumberFormat('es').format(0.0004));
  });
  it('isolates alpha identifiers after Arabic API names', () => {
    renderApp(<AlphaLibraryPage />, { locale: 'ar' });

    expect(screen.getByText('alpha-precision')).toHaveAttribute('dir', 'ltr');
  });
  it('filters the visible localized evidence value', () => {
    renderApp(<AlphaLibraryPage />, { locale: 'es' });
    const precise = new Intl.NumberFormat('es', { maximumSignificantDigits: 15 }).format(0.0004);
    fireEvent.change(screen.getByRole('textbox'), { target: { value: precise } });
    expect(screen.getByText('Precision Alpha')).toBeInTheDocument();
  });
});
