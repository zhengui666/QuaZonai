#!/usr/bin/env node
// Stopped-writer inspection through official public Storage APIs; no Harness,
// scheduler, migration writes, remote cancellation or replacement task identities.
import { DatabaseSync } from 'node:sqlite';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
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
    const record = await storage.findDocument({ kind: 'qz.application', scope: { kind: 'session' } }, 'current', context);
    const application = record ? (await storage.document(record.id, 'current', context)).value : {};
    const maintenance = application.maintenance;
    const quiesced = maintenance?.schemaVersion === 1 && maintenance.state === 'QUIESCED'
      && typeof maintenance.epoch === 'string' && maintenance.epoch.length > 0
      && Number.isSafeInteger(maintenance.barrierTaskId) && maintenance.barrierTaskId > 0
      && typeof maintenance.quiescedAtNs === 'string' && /^\d+$/.test(maintenance.quiescedAtNs)
      && maintenance.parked && typeof maintenance.parked === 'object' && !Array.isArray(maintenance.parked);
    if (maintenance && maintenance.state !== 'RUNNING' && !quiesced) throw new Error(`Maintenance is not formally QUIESCED in ${path}; resume the original service and finish draining it.`);
    const parkKinds = new Set(['qz.pm.mandate-monitor', 'qz.account-adjustment']);
    const observedParked = new Set(); let observedGate = false;
    for (const status of ['pending', 'running', 'waiting', 'completing']) {
      let cursor;
      do {
        const page = await storage.scanTasks({ status }, 100, cursor, context);
        for (const task of page.items) {
          const checkpoint = task.state.checkpoint;
          const original = quiesced ? maintenance.parked[String(task.id)] : undefined;
          const common = quiesced && task.version === 1 && task.background === true && task.abortRequested === false;
          const gate = common && task.id === maintenance.barrierTaskId && task.kind === 'qz.maintenance.gate'
            && task.input?.epoch === maintenance.epoch && status === 'running' && checkpoint?.phase === 'hold';
          const parked = common && original?.taskId === task.id && original.kind === task.kind && original.epoch === maintenance.epoch
            && parkKinds.has(task.kind) && status === 'waiting' && task.state.policy === 'allSettled'
            && task.state.on.length === 1 && task.state.on[0] === maintenance.barrierTaskId
            && checkpoint?.phase === 'maintenance' && checkpoint.maintenanceEpoch === maintenance.epoch
            && isDeepStrictEqual(checkpoint.resumeCheckpoint, original.resumeCheckpoint);
          if (gate) observedGate = true;
          else if (parked) observedParked.add(String(task.id));
          else throw new Error(`Unresolved durable task ${task.id} (${status}) in ${path}; only original tasks parked for the exact QUIESCED epoch may pass maintenance.`);
        }
        cursor = page.next;
      } while (cursor);
    }
    if (quiesced && (!observedGate || Object.keys(maintenance.parked).some(id => !observedParked.has(id)))) throw new Error(`Incomplete maintenance epoch in ${path}; original gate and parked task identities must still exist.`);
    for (const status of ['queued', 'placed']) {
      const page = await storage.scanSubmissions({ status }, 1, undefined, context);
      if (page.items.length) throw new Error(`Unresolved research submission ${page.items[0].id} in ${path}; no queued work was discarded.`);
    }
    if (record) {
      for (const fact of Object.values(application.observations ?? {})) {
        const status = fact.view?.status;
        if (['PENDING', 'STARTING', 'UNCERTAIN', 'DELIVERING', 'RUNNING'].includes(status) || ['PENDING', 'UNCERTAIN'].includes(fact.view?.receipt?.status)) {
          throw new Error(`Unresolved downstream observation ${fact.view?.id ?? 'unknown'} in ${path}; reconcile its original external receipt first.`);
        }
        if (['PENDING', 'UNCERTAIN'].includes(fact.view?.allocation?.status) || ['PENDING', 'UNCERTAIN'].includes(fact.view?.allocation?.receipt?.status)) {
          throw new Error(`Unresolved downstream allocation ${fact.view?.id ?? 'unknown'} in ${path}; reconcile its original external receipt first.`);
        }
      }
      for (const [id, account] of Object.entries(application.accountControls ?? {})) {
        for (const [requestId, command] of Object.entries(account.commands ?? {})) if (['PENDING', 'UNCERTAIN'].includes(command.state)) {
          throw new Error(`Unresolved capital command ${id}:${requestId} in ${path}; reconcile its original external receipt first.`);
        }
      }
    }
    const pm = await storage.findDocument({ kind: 'qz.pm', scope: { kind: 'session' } }, 'current', context);
    if (pm) {
      const doc = await storage.document(pm.id, 'current', context);
      for (const mandate of Object.values(doc.value.mandates ?? {})) {
        for (const command of Object.values(mandate.commands ?? {})) {
          if (['PENDING', 'UNCERTAIN'].includes(command.state)) {
            throw new Error(`Unresolved PM target command ${command.id ?? 'unknown'} in ${path}; reconcile its original external receipt first.`);
          }
        }
        for (const outcome of Object.values(mandate.outcomes ?? {})) {
          if (['PENDING', 'UNCERTAIN'].includes(outcome.receiptState)) {
            // Outcomes preserve what was known then. A later receipt resolves
            // its original command without rewriting the historical outcome.
            const command = mandate.commands?.[outcome.targetCommandId];
            const resolved = typeof outcome.decisionId === 'string'
              && command?.id === outcome.targetCommandId && command?.decisionId === outcome.decisionId
              && ['ACKNOWLEDGED', 'REJECTED', 'NOT_OFFERED'].includes(command?.state);
            if (!resolved) throw new Error(`Unresolved PM outcome receipt ${outcome.id ?? 'unknown'} in ${path}; local task completion is not external acknowledgement.`);
          }
        }
        for (const retirement of Object.values(mandate.retirements ?? {})) {
          if (retirement.reconciliationRequired === true) {
            throw new Error(`Unresolved PM retirement ${retirement.id ?? 'unknown'} in ${path}; retiring a strategy does not remove its positions or commands.`);
          }
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
