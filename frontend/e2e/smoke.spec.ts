import { expect, test, type Page } from '@playwright/test';

async function createPendingSource(page: Page) {
  const suffix = `${test.info().project.name}-${test.info().testId}-${Date.now()}`
    .replace(/[^A-Za-z0-9._-]/g, '-')
    .slice(-48);
  const universeName = `US Equities ${suffix}`;
  const sourceName = `Primary PIT Data ${suffix}`;

  await page.goto('/admin');
  const researchReady = page.locator('.qz-kpi').filter({ hasText: 'Research ready' });
  await expect(researchReady).toContainText('NO');

  await page.getByRole('button', { name: 'Create Universe', exact: true }).click();
  let dialog = page.getByRole('dialog');
  await dialog.getByLabel('Universe key').fill(`US_EQUITIES_${suffix}`);
  await dialog.getByLabel('Name').fill(universeName);
  await dialog.getByLabel('Universe specification (JSON)').fill(JSON.stringify({
    instrument_schema: { instrument_id: 'string' },
    membership_rules: { listing: 'NYSE|NASDAQ' },
    calendar_semantics: { timezone: 'America/New_York' },
    currency_semantics: { base_currency: 'USD' },
    data_requirements: { available_at: 'required' },
    risk_model_family: 'EWMA',
    cost_model_family: 'SPREAD',
    capacity_model_family: 'ADV',
  }));
  await dialog.getByRole('button', { name: 'Create Universe', exact: true }).click();
  await expect(page.getByText(universeName, { exact: true })).toBeVisible();

  const registerSource = page.getByRole('button', { name: 'Register data source', exact: true });
  await expect(registerSource).toBeEnabled();
  await registerSource.click();
  dialog = page.getByRole('dialog');
  await dialog.getByLabel('Name').fill(sourceName);
  await dialog.getByLabel('Connector key').fill(`licensed-bars-${suffix}`);
  await dialog.getByLabel('Provider').fill('Approved provider');
  await dialog.getByRole('combobox').click();
  await page.getByRole('option', { name: new RegExp(universeName) }).click();
  await dialog.getByLabel('License classification').fill('LICENSED');
  await dialog.getByLabel('Field schema (JSON)').fill(JSON.stringify({
    event_time: 'timestamp', available_at: 'timestamp', close: 'decimal',
  }));
  await dialog.getByLabel('Availability semantics (JSON)').fill(JSON.stringify({
    available_at_field: 'available_at',
  }));
  await dialog.getByRole('button', { name: 'Register data source', exact: true }).click();

  const sourceTable = page.locator('.qz-table-shell').filter({ hasText: sourceName });
  await expect(sourceTable.getByText('Pending', { exact: true }).first()).toBeVisible();
  await expect(researchReady).toContainText('NO');
  return { sourceName };
}

test('Fresh configuration flow 1: a governed source remains pending and research stays unready', async ({ page }) => {
  await createPendingSource(page);
});

test('Fresh configuration flow 2: a pending source remains unready after reload', async ({ page }) => {
  const { sourceName } = await createPendingSource(page);
  await page.reload();
  const sourceTable = page.locator('.qz-table-shell').filter({ hasText: sourceName });
  await expect(sourceTable.getByText('Pending', { exact: true }).first()).toBeVisible();
  const researchReady = page.locator('.qz-kpi').filter({ hasText: 'Research ready' });
  await expect(researchReady).toContainText('NO');
});
