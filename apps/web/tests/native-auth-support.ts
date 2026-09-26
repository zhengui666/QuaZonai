import { expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import { appendFileSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

export type Fixture = {
  baseUrl: string; redactionsFile: string; password: string;
  phase: 'before-restart' | 'after-restart';
};
export function fixture(): Fixture {
  const path = process.env.QUAZONAI_WEB_E2E_FIXTURE;
  if (!path) throw new Error('Missing private native fixture; use the native-browser harness');
  const value: unknown = JSON.parse(readFileSync(path, 'utf8'));
  if (typeof value !== 'object' || value === null) throw new Error('Invalid private fixture');
  const fields = value as Record<string, unknown>;
  if (typeof fields.baseUrl !== 'string' || fields.baseUrl !== process.env.QUAZONAI_WEB_E2E_ORIGIN
    || typeof fields.password !== 'string' || fields.password.length < 8
    || !['before-restart', 'after-restart'].includes(String(fields.phase))
    || fields.redactionsFile !== resolve(dirname(path), 'redactions.jsonl')) {
    throw new Error('Private fixture fields do not match the test-owned runtime');
  }
  return fields as Fixture;
}
export function rememberPrivateValue(config: Fixture, value: string) {
  appendFileSync(config.redactionsFile, `${JSON.stringify(value)}\n`, { mode: 0o600 });
}
export async function loginNative(page: Page, config: Fixture, remember = true, password = config.password) {
  await page.goto('/');
  await expect(page.getByLabel('登录密码', { exact: true })).toBeVisible();
  await page.getByLabel('登录密码', { exact: true }).fill(password);
  await page.getByRole('checkbox', { name: '记住本设备 30 天' }).setChecked(remember);
  await page.getByRole('button', { name: '登录', exact: true }).click();
  await expect(page.getByRole('button', { name: '新建研究', exact: true })).toBeVisible();
  for (const cookie of await page.context().cookies()) rememberPrivateValue(config, cookie.value);
}
