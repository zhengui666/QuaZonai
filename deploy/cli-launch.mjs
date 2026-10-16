// Release distribution adapter, executed by the bundled official Node runtime.
import { readFile } from 'node:fs/promises';
if (process.argv.slice(2).includes('--version')) {
  const { version } = JSON.parse(await readFile(new URL('../distribution.json', import.meta.url), 'utf8'));
  process.stdout.write(`quazonai ${version}\n`);
} else {
  await import('./apps/cli/src/main.ts');
}
