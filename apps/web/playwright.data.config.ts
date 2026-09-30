import { defineConfig } from '@playwright/test';
import native from './playwright.config';

// A distinct admission-only phase after the unchanged auth/restart scenarios.
// Uses the same real packaged API and disposable database; no browser API mocks.
export default defineConfig({ ...native, testMatch: ['**/native-data-inputs.spec.ts'] });
