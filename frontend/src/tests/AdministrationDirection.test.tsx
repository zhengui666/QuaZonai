import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { CanonicalFieldList } from '../pages/AdministrationPage';
import { renderApp } from './testUtils';

describe('Administration canonical field direction', () => {
  it('keeps a canonical field list left-to-right in Arabic', () => {
    const fields = 'event_time, available_time, close, volume';
    renderApp(<CanonicalFieldList fields={fields.split(', ')} />, { locale: 'ar' });

    expect(screen.getByText(fields, { selector: 'bdi' })).toHaveAttribute('dir', 'ltr');
  });
});
