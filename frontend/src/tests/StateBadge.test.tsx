import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { StateBadge } from '../components/ui/StateBadge';
import { renderApp } from './testUtils';

describe('StateBadge localization', () => {
  it('uses the semantic QUALIFIED state label in Simplified Chinese', () => {
    renderApp(<StateBadge state="QUALIFIED" />, { locale: 'zh-CN' });

    expect(screen.getByText('合格')).toBeInTheDocument();
    expect(screen.queryByText('合格时间')).not.toBeInTheDocument();
  });

  it('uses the semantic QUALIFIED state label in Arabic', () => {
    renderApp(<StateBadge state="QUALIFIED" />, { locale: 'ar' });

    expect(screen.getByText('مؤهل')).toBeInTheDocument();
    expect(screen.queryByText('تاريخ التأهيل')).not.toBeInTheDocument();
  });
});
