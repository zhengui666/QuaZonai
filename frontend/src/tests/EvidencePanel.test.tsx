import { Theme } from '@radix-ui/themes';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { EvidencePanel, formatEvidenceValue } from '../components/approval/EvidencePanel';
import { I18nProvider } from '../i18n';
import type { ApprovalSnapshot } from '../lib/api/types';

const approval: ApprovalSnapshot = {
  id: 'approval-1',
  candidate_id: 'candidate-1',
  purpose: 'PAPER',
  state: 'PENDING',
  evidence_summary: { capacity: 0.5 },
};

describe('EvidencePanel', () => {
  it('keeps arbitrary evidence schema keys canonical in localized UI', () => {
    render(
      <I18nProvider initialLocale="zh-CN">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <EvidencePanel approval={approval} />
        </Theme>
      </I18nProvider>,
    );

    expect(screen.getByText(/Capacity$/)).toBeInTheDocument();
    expect(screen.queryByText(/容量$/)).not.toBeInTheDocument();
  });
  it('isolates canonical evidence schema keys in Arabic', () => {
    render(
      <I18nProvider initialLocale="ar">
        <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
          <EvidencePanel approval={{ ...approval, evidence_summary: { search_adjusted_quality: 0.5 } }} />
        </Theme>
      </I18nProvider>,
    );

    expect(screen.getByText('Search Adjusted Quality')).toHaveAttribute('dir', 'ltr');
  });
  it('preserves small nonzero evidence percentages', () => {
    const value = 0.00004;
    expect(formatEvidenceValue('es', value)).toBe(
      new Intl.NumberFormat('es', { style: 'percent', maximumSignificantDigits: 15 }).format(value),
    );
    expect(formatEvidenceValue('es', value)).not.toBe(
      new Intl.NumberFormat('es', { style: 'percent', minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(value),
    );
  });
  it('formats numeric evidence arrays recursively with the active locale', () => {
    const compact = new Intl.NumberFormat('es', { notation: 'compact', maximumFractionDigits: 2 }).format(1234.5);
    const percent = new Intl.NumberFormat('es', { style: 'percent', maximumSignificantDigits: 15 });
    expect(formatEvidenceValue('es', [1234.5, 0.25, [0.5]])).toBe(
      `${compact}, ${percent.format(0.25)}, ${percent.format(0.5)}`,
    );
  });

});
