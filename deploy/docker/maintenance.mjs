#!/usr/bin/env node
// Stopped-writer inspection through official public Storage APIs; no Harness,
// scheduler, migration writes, remote cancellation or replacement task identities.
import { DatabaseSync } from 'node:sqlite';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const require = createRequire(new URL('../../apps/service/package.json', import.meta.url));
const { openNodeSqliteStorage } = await import(require.resolve('@earendil-works/pi-durable/storage/sqlite/node'));
// Chord exports are import-only. Resolve its declared public import target.
const chordPackage = require.resolve('@earendil-works/chord/package.json');
const chord = JSON.parse(readFileSync(chordPackage, 'utf8'));
export const { BACKGROUND_CONTEXT: context } = await import(new URL(chord.exports['./context'].import, pathToFileURL(chordPackage)).href);

export function requireOriginal(configPath) {
  const config = JSON.parse(readFileSync(configPath, 'utf8'));
  if (!config.research?.database) throw new Error(`Database path missing in ${configPath}`);
  const path = resolve(dirname(configPath), config.research.database);
  // readOnly refuses absent/unknown databases rather than creating empty state.
  const data = resolve(dirname(configPath), config.dataDir);
  if (!path.startsWith(data + '/')) throw new Error(`External research database ${path} needs an explicit stopped recovery plan; this data backup cannot silently omit it.`);
  // Upstream open performs idempotent ledger bookkeeping, so validate original
  // metadata on a read-only connection first. Never initialize over an absent,
  // empty or unrelated file. No scheduler is opened by this maintenance owner.
  const original = new DatabaseSync(path, { readOnly: true });
  try {
    if (!original.prepare('SELECT next_id FROM durable_metadata WHERE singleton = 1').get()) throw new Error(`Original durable metadata unavailable: ${path}`);
  } finally { original.close(); }
  return path;
}
export async function requireIdle(configPath) {
  const path = requireOriginal(configPath);
  const storage = await openNodeSqliteStorage(path);
  try {
    for (const status of ['pending', 'running', 'waiting', 'completing']) {
      const page = await storage.scanTasks({ status }, 1, undefined, context);
      if (page.items.length) throw new Error(`Unresolved durable task ${page.items[0].id} (${status}) in ${path}; resume the original release and reconcile it before updating.`);
    }
    for (const status of ['queued', 'placed']) {
      const page = await storage.scanSubmissions({ status }, 1, undefined, context);
      if (page.items.length) throw new Error(`Unresolved research submission ${page.items[0].id} in ${path}; no queued work was discarded.`);
    }
    const record = await storage.findDocument({ kind: 'qz.application', scope: { kind: 'session' } }, 'current', context);
    if (record) {
      const doc = await storage.document(record.id, 'current', context);
      for (const fact of Object.values(doc.value.observations ?? {})) {
        const status = fact.view?.status;
        if (['PENDING', 'STARTING', 'UNCERTAIN', 'DELIVERING', 'RUNNING'].includes(status) || ['PENDING', 'UNCERTAIN'].includes(fact.view?.receipt?.status)) {
          throw new Error(`Unresolved downstream observation ${fact.view?.id ?? 'unknown'} in ${path}; reconcile its original external receipt first.`);
        }
      }
    }
  } finally { await storage.close(context); }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const configPath = process.env.QZ_CONFIG ?? process.argv[2];
  if (process.argv[2] === 'exists') requireOriginal(configPath);
  else { await requireIdle(configPath); console.log('Original durable database is idle; no cancellation was inferred.'); }
}
