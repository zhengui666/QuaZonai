import { afterEach, describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes } from 'react-router-dom';
import { AppShell } from './AppShell';
import { localeLabels, localeOrder } from '../../i18n';
import { renderApp } from '../../tests/testUtils';

const operatorAuth = vi.hoisted(() => ({
  authEnabled: false,
  logout: vi.fn(),
}));

vi.mock('../../auth/AuthGate', () => ({
  isLogoutError: (error: unknown) => typeof error === 'object' && error !== null && 'failure' in error,
  useOperatorAuth: () => operatorAuth,
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

afterEach(() => {
  operatorAuth.authEnabled = false;
  operatorAuth.logout.mockReset();
});

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

  it('re-renders fallback sign-out errors after a locale change while preserving API messages', async () => {
    operatorAuth.authEnabled = true;
    operatorAuth.logout.mockRejectedValue({ failure: { kind: 'http', status: 503 } });
    const user = userEvent.setup();
    renderShell();

    await user.click(screen.getByRole('button', { name: 'Sign out and forget this browser' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Sign out failed with HTTP 503.');

    await user.click(screen.getByRole('button', { name: 'Change language: English' }));
    await user.click(screen.getByRole('menuitemradio', { name: /العربية/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent('فشل تسجيل الخروج مع HTTP 503.');

    operatorAuth.logout.mockRejectedValue({ failure: { kind: 'api', message: 'Sign-out policy denied.' } });
    await user.click(screen.getByRole('button', { name: 'تسجيل الخروج ونسيان هذا المتصفح' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Sign-out policy denied.');
  });
});
