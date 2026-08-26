import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { Route, Routes } from 'react-router-dom';
import { AppShell } from './AppShell';
import { localeLabels, localeOrder } from '../../i18n';
import { renderApp } from '../../tests/testUtils';

vi.mock('../../auth/AuthGate', () => ({
  useOperatorAuth: () => ({ authEnabled: false, logout: vi.fn() }),
}));

function renderShell() {
  return renderApp(
    <Routes>
      <Route element={<AppShell />}>
        <Route index element={<div>Workbench</div>} />
      </Route>
    </Routes>,
    { route: '/', locale: 'en' },
  );
}

describe('AppShell locale picker', () => {
  it('marks every language option with its own language and direction', async () => {
    const user = userEvent.setup();
    renderShell();

    await user.click(screen.getByRole('button', { name: 'Change language: English' }));
    for (const code of localeOrder) {
      expect(screen.getByText(localeLabels[code].native, { selector: `span[lang="${code}"]:not(.qz-section-meta)` })).toHaveAttribute('dir', localeLabels[code].dir);
      const englishLabel = screen.getByText(localeLabels[code].english, { selector: 'span.qz-section-meta' });
      expect(englishLabel).toHaveAttribute('lang', 'en');
      expect(englishLabel).toHaveAttribute('dir', 'ltr');
    }
  });
});
