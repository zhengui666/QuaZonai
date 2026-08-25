import { execFileSync } from 'node:child_process';
import { expect, test } from '@playwright/test';

const authEnabled = process.env.QUAZONAI_E2E_AUTH_ENABLED === 'true';
const username = process.env.QUAZONAI_E2E_AUTH_USERNAME ?? 'operator';
const password = process.env.QUAZONAI_E2E_AUTH_PASSWORD ?? 'correct horse battery staple';
const totpSecret = process.env.QUAZONAI_E2E_AUTH_TOTP_SECRET ?? '';

function currentTotpCode(): string {
  if (!totpSecret) throw new Error('QUAZONAI_E2E_AUTH_TOTP_SECRET is required');
  return execFileSync(
    'python',
    [
      '-c',
      'import os, pyotp; print(pyotp.TOTP(os.environ["QUAZONAI_E2E_AUTH_TOTP_SECRET"]).now())',
    ],
    {
      encoding: 'utf8',
      env: { ...process.env, QUAZONAI_E2E_AUTH_TOTP_SECRET: totpSecret },
    },
  ).trim();
}

test.describe('single-operator authentication', () => {
  test.skip(!authEnabled, 'Runs only in the dedicated auth-enabled browser workflow.');

  test('password + TOTP login, trusted-browser restore, and logout revocation', async ({
    page,
    context,
  }) => {
    await page.goto('/');

    await page.getByLabel('Username', { exact: true }).fill(username);
    await page.getByLabel('Password', { exact: true }).fill(password);
    await page.getByLabel('Authenticator code', { exact: true }).fill(currentTotpCode());
    await page.getByRole('checkbox', { name: /^Trust this browser/ }).check();
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();

    await expect(page.getByText('Dashboard', { exact: true }).first()).toBeVisible();

    const authenticatedCookies = await context.cookies();
    const session = authenticatedCookies.find((cookie) => cookie.name === 'quazonai_session');
    const trusted = authenticatedCookies.find(
      (cookie) => cookie.name === 'quazonai_trusted_browser',
    );
    expect(session).toBeDefined();
    expect(trusted).toBeDefined();
    expect(session?.httpOnly).toBe(true);
    expect(trusted?.httpOnly).toBe(true);
    expect(session?.sameSite).toBe('Strict');
    expect(trusted?.sameSite).toBe('Strict');

    await context.clearCookies({ name: 'quazonai_session' });
    await page.reload();

    await expect(page.getByText('Dashboard', { exact: true }).first()).toBeVisible();
    expect((await context.cookies()).some((cookie) => cookie.name === 'quazonai_session')).toBe(
      true,
    );

    await page.getByRole('button', { name: /sign out|log out/i }).click();
    await expect(page.getByLabel('Username', { exact: true })).toBeVisible();

    const loggedOutCookies = await context.cookies();
    expect(loggedOutCookies.some((cookie) => cookie.name === 'quazonai_session')).toBe(false);
    expect(
      loggedOutCookies.some((cookie) => cookie.name === 'quazonai_trusted_browser'),
    ).toBe(false);
  });
});
