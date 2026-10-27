// Package original project license/contract assets; no hand-maintained schemas.
import { copyFile, mkdir } from 'node:fs/promises';
const root = new URL('../../../', import.meta.url);
const local = new URL('../', import.meta.url);
await mkdir(new URL('schemas/', local), { recursive: true });
await copyFile(new URL('LICENSE', root), new URL('LICENSE', local));
await copyFile(new URL('packages/contracts/src/index.ts', root), new URL('schemas/contracts.ts.txt', local));
await copyFile(new URL('packages/contracts/src/pm.ts', root), new URL('schemas/pm.ts.txt', local));
await copyFile(new URL('packages/contracts/src/pm-intent.ts', root), new URL('schemas/pm-intent.ts.txt', local));
await copyFile(new URL('packages/contracts/src/pm-forward.ts', root), new URL('schemas/pm-forward.ts.txt', local));
await copyFile(new URL('packages/contracts/src/maintenance.ts', root), new URL('schemas/maintenance.ts.txt', local));
await copyFile(new URL('packages/contracts/src/research.ts', root), new URL('schemas/research.ts', local));
await copyFile(new URL('packages/contracts/src/tick-input.ts', root), new URL('schemas/tick-input.ts.txt', local));
await copyFile(new URL('contracts/generated/domain-v1.openapi.json', root), new URL('schemas/domain-v1.openapi.json', local));
