import { defineConfig } from '@playwright/test';
import native from './playwright.config';

// Explicit real production Runtime/Worker/OCI phase, never a skipped extension
// of the ordinary synthetic TLS admission test.
export default defineConfig({ ...native, timeout: 240_000, testMatch: ['**/native-data-completion.spec.ts'] });
