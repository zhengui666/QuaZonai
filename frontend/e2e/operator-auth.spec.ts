import { execFileSync } from 'node:child_process';
import { expect, test } from '@playwright/test';

const authEnabled = process.env.QUAZONAI_E2E_AUTH_ENABLED === 'true';
function currentTotpCode(secret: string): string {
  if (!secret) throw new Error('A setup candidate is required');
  return execFileSync(
    'python',
    [
      '-c',
      'import os, pyotp; print(pyotp.TOTP(os.environ["QUAZONAI_E2E_AUTH_TOTP_SECRET"]).now())',
    ],
    {
      encoding: 'utf8',
      env: { ...process.env, QUAZONAI_E2E_AUTH_TOTP_SECRET: secret },
    },
  ).trim();
}

test.describe('single-operator authentication', () => {
  test.skip(!authEnabled, 'Runs only in the dedicated auth-enabled browser workflow.');
  test.describe.configure({ retries: 0 });

  test('first-visit TOTP setup, trusted-browser restore, and logout revocation', async ({
    page,
    context,
  }) => {
    await page.goto('/');

    await expect(page.getByRole('heading', { name: 'Set up your authenticator' })).toBeVisible();
    await expect(page.getByLabel('Authenticator setup QR code')).toBeVisible();
    const setupSecret = await page.locator('.qz-auth-manual-key code').innerText();
    expect(setupSecret.length).toBeGreaterThanOrEqual(32);
    await page.getByLabel('Authenticator code', { exact: true }).fill(currentTotpCode(setupSecret));
    await page.getByRole('checkbox', { name: /^Trust this browser/ }).check();
    await page.getByRole('button', { name: 'Confirm and continue', exact: true }).click();

    await expect(page.locator('h1.qz-page-title')).toBeVisible();

    const authenticatedCookies = await context.cookies();
    const session = authenticatedCookies.find((cookie) => cookie.name === 'quazonai_session');
    const trusted = authenticatedCookies.find((cookie) => cookie.name === 'quazonai_trusted_browser');
    expect(session).toBeDefined();
    expect(trusted).toBeDefined();
    expect(session?.httpOnly).toBe(true);
    expect(trusted?.httpOnly).toBe(true);
    expect(session?.sameSite).toBe('Strict');
    expect(trusted?.sameSite).toBe('Strict');

    await context.clearCookies({ name: 'quazonai_session' });
    await page.reload();
    await expect(page.locator('h1.qz-page-title')).toBeVisible();
    expect((await context.cookies()).some((cookie) => cookie.name === 'quazonai_session')).toBe(true);

    await page.getByRole('button', { name: /sign out|log out/i }).click();
    await expect(page.getByLabel('Authenticator code', { exact: true })).toBeVisible();
    await expect(page.getByLabel('Username', { exact: true })).toHaveCount(0);
    await expect(page.getByLabel('Password', { exact: true })).toHaveCount(0);

    const loggedOutCookies = await context.cookies();
    expect(loggedOutCookies.some((cookie) => cookie.name === 'quazonai_session')).toBe(false);
    expect(loggedOutCookies.some((cookie) => cookie.name === 'quazonai_trusted_browser')).toBe(false);
  });
});
