import { expect, test } from '@playwright/test';

test('Flow 1: create idea -> research program -> mission appears', async ({ page }) => {
  await page.goto('/ideas');
  await page.getByLabel('What should the research system investigate?').fill(
    'Test post-earnings drift in liquid US equities after realistic costs.',
  );
  await page.getByRole('button', { name: 'Preview research charter' }).click();
  await expect(page.getByText(/post-earnings drift in liquid US equities/i)).toBeVisible();
  await page.getByRole('button', { name: 'Start Research' }).click();
  await expect(page).toHaveURL(/\/research\/[0-9a-f-]+$/i);
  await expect(page.getByText(/Alpha Discovery · Ready/i)).toBeVisible();
});

test('Flow 2: candidate ready -> approve -> handoff available', async ({ page }) => {
  await page.goto('/approval');
  await expect(page.getByText(/materially improves the current frontier/i)).toBeVisible();
  await expect(page.getByText(/Paper Lab · PAPER/i).first()).toBeVisible();
  await page.getByRole('button', { name: 'Approve' }).first().click();
  await expect(page.getByText('Approved', { exact: true })).toBeVisible();
  await page.getByRole('link', { name: 'Handoff Center' }).click();
  await expect(page.getByText('Available', { exact: true }).first()).toBeVisible();
  await expect(page.getByRole('button', { name: /stop|undeploy|close position|buy|sell/i })).toHaveCount(0);
});

test('Flow 3: create datasource -> readiness update', async ({ page }) => {
  await page.goto('/admin');
  const researchReady = page.locator('.qz-kpi').filter({ hasText: 'Research ready' });
  await expect(researchReady).toContainText('NO');
  await page.getByRole('button', { name: /Register data source/ }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Name').fill('Primary PIT Data');
  await dialog.getByLabel('Provider').fill('Approved provider');
  await dialog.getByLabel('Canonical fields').fill('event_time, available_time, close, volume');
  await dialog.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('Primary PIT Data')).toBeVisible();
  await expect(researchReady).toContainText('YES');
});
