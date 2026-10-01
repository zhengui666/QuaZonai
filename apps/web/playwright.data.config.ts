import { defineConfig } from '@playwright/test';
import native from './playwright.config';

// The common queued admission phase after unchanged auth/restart scenarios.
// Uses the same real packaged API and disposable database; no browser API mocks.
export default defineConfig({ ...native, testMatch: ['**/native-data-inputs.spec.ts'] });
