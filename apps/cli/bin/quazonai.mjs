#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { registerHooks, stripTypeScriptTypes } from 'node:module';

// Node does not automatically strip .ts under node_modules. Use its public
// strip API only for this package's own source when installed by npm.
const sources = new URL('../src/', import.meta.url).href;
const research = new URL('../schemas/research.ts', import.meta.url).href;
if (sources.includes('/node_modules/')) registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === '../../../packages/contracts/src/research.ts' && context.parentURL?.startsWith(sources)) return { url: research, shortCircuit: true };
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if ((url.startsWith(sources) || url === research) && url.endsWith('.ts')) return { format: 'module', source: stripTypeScriptTypes(readFileSync(new URL(url), 'utf8')), shortCircuit: true };
    return nextLoad(url, context);
  },
});
await import('../src/main.ts');
