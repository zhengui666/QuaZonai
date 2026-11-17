#!/usr/bin/env node
// Stopped-writer inspection through official public Storage APIs; no Harness,
// scheduler, migration writes, remote cancellation or replacement task identities.
import { DatabaseSync } from 'node:sqlite';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { isDeepStrictEqual, stripVTControlCharacters } from 'node:util';
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
// Called before stopping the live writer, after the owner enters existing
// maintenance. Admission is closed; no unrelated task or external activity may
// be interrupted just to discover it during the stopped inspection.
export function requireExitedPreflight(selection, status, detail, owner) {
  const fail = () => { throw new Error('Enter maintenance and wait until only the selected exited native job remains; app was not stopped'); };
  const ownerKind = owner?.kind;
  const supportedOwner = ['qz.native-strategy.backtest', 'qz.native-market-import'].includes(ownerKind);
  const live = detail?.tasks?.filter(task => task.status !== 'terminal');
  const expected = new Map((live ?? []).map(task => [task.id, task.name]));
  const native = detail?.tasks?.find(task => task.id === selection.nativeTaskId);
  if (status?.state !== 'DRAINING' || typeof status.epoch !== 'string' || !status.epoch
    || status.activeBusinessRequests !== 0 || status.blockingExternal?.length !== 0 || status.parkedTaskIds?.length !== 0
    || owner?.taskId !== selection.ownerTaskId || !supportedOwner
    || owner.state?.status !== 'sleeping' || owner.state.checkpoint?.phase !== 'reconcile'
    || owner.state.checkpoint.nativeTaskId !== selection.nativeTaskId
    || detail?.id !== selection.conversationId || detail.submission?.id !== selection.submissionId
    || native?.name !== 'qz.native.execution' || native.status !== 'terminal' || native.error !== 'nautilus_execution_unknown'
    || live?.length !== 3 || expected.get(selection.ownerTaskId) !== ownerKind
    || live.find(task => task.id === selection.ownerTaskId)?.status !== 'sleeping'
    || [...expected.values()].sort().join(',') !== ['pi.generation', 'pi.tool', ownerKind].sort().join(',')
    || status.blockingTasks?.length !== 3 || status.blockingTasks.some(task => expected.get(task.taskId) !== task.kind)
    || status.blockingSubmissions?.length !== 1 || status.blockingSubmissions[0].submissionId !== selection.submissionId
    || status.blockingSubmissions[0].status !== 'placed') fail();
  return {nativeFailure:selection, maintenanceEpoch:status.epoch};
}
// Explicit upgrade of one already-exited configured backtest or market import. This only reads
// original Durable records; normal startup resumes them and the API reviews the
// failure afterwards. No task, receipt, submission or diagnostic is rewritten.
async function exitedBacktest(storage, selection) {
  const fail = () => { throw new Error('Original exited native job upgrade evidence conflict'); };
  if (!selection || Object.keys(selection).sort().join(',') !== 'conversationId,nativeTaskId,ownerTaskId,submissionId'
    || Object.values(selection).some(id => !Number.isSafeInteger(id) || id <= 0)) fail();
  const {conversationId, nativeTaskId, ownerTaskId, submissionId} = selection;
  const native = await storage.task(nativeTaskId, context), owner = await storage.task(ownerTaskId, context);
  const input = native?.input;
  const marketImport = owner?.kind === 'qz.native-market-import' && input?.command === 'native-market-import';
  const backtest = owner?.kind === 'qz.native-strategy.backtest' && input?.command === 'native-configured-backtest';
  if (!native || native.kind !== 'qz.native.execution' || native.conversationId !== conversationId
    || native.owner !== ownerTaskId || native.abortRequested || input?.ownerTaskId !== ownerTaskId
    || (!marketImport && !backtest) || input.target?.kind !== 'RUST_PROCESS'
    || typeof input.requestId !== 'string' || !/^[a-f0-9]{64}$/.test(input.requestId)
    || typeof input.basis !== 'string' || native.state.status !== 'terminal'
    || native.state.outcome.status !== 'failed' || native.state.outcome.error.message !== 'nautilus_execution_unknown') fail();
  if (!owner || (!marketImport && !backtest) || owner.conversationId !== conversationId
    || owner.abortRequested || !['running', 'sleeping'].includes(owner.state.status)
    || owner.state.checkpoint?.phase !== 'reconcile' || owner.state.checkpoint.nativeTaskId !== nativeTaskId) fail();
  const tool = await storage.task(owner.owner, context), generation = tool && await storage.task(tool.owner, context);
  const waits = (task, kind, child) => task?.kind === kind && task.conversationId === conversationId
    && !task.abortRequested && task.state.status === 'waiting' && task.state.on.length === 1 && task.state.on[0] === child;
  const suspendedTool = tool?.kind === 'pi.tool' && tool.conversationId === conversationId && !tool.abortRequested
    && tool.state.status === 'pending' && tool.state.checkpoint?.phase === 'execute' && tool.state.checkpoint.replay === 'safe' && tool.input?.kind === 'model'
    && tool.input.assistant === generation?.state.checkpoint?.assistant;
  if (!(waits(tool, 'pi.tool', ownerTaskId) || suspendedTool) || !waits(generation, 'pi.generation', tool.id)) fail();
  const submission = await storage.submission(submissionId, context);
  if (!submission || submission.conversationId !== conversationId || submission.status !== 'placed') fail();
  async function doc(kind, scope, key) {
    const record = await storage.findDocument({kind, scope, key}, 'current', context);
    return record && (await storage.document(record.id, 'current', context)).value;
  }
  const scope = {kind:'conversation', conversationId};
  const facts = await doc('qz.native-remote-uncertainty', scope, String(ownerTaskId));
  const references = facts?.requests;
  if (!Array.isArray(references) || references.length !== 1) fail();
  const ref = references[0], process = ref.process;
  if (ref.taskId !== ownerTaskId || ref.requestId !== input.requestId || ref.status !== 'UNKNOWN'
    || !isDeepStrictEqual(ref.target, input.target) || ref.configurationFailureReview
    || !process || !isDeepStrictEqual(process, native.state.outcome.error.detail?.process)
    || process.errorCategory !== 'native_process_failed' || process.exitObserved !== true || process.exitCode !== 1
    || process.signal !== null || process.terminationRequested !== false) fail();
  const call = await doc('qz.native-call', {kind:'task',taskId:ownerTaskId}, input.basis);
  const stderr = await doc('qz.native-evidence-part', scope, `${nativeTaskId}:private-native-stderr`);
  const knownRejection = typeof stderr?.data === 'string' && (marketImport ? stderr.data === 'QZ_NATIVE_JOB_FAILED\nSOURCE_PATH_PREFIX_INVALID\n' : (stderr.data === 'QZ_NATIVE_JOB_FAILED\nvenues must not be empty\n'
    || /(?:^|\n)[^\n]*\[ERROR\] [^\n]*nautilus_backtest::node: Error building backtest '[^'\n]+': BacktestVenueConfig for '[^'\n]+' requires an explicit fee_model, including an explicit zero-fee model\nQZ_NATIVE_JOB_FAILED\nNATIVE_CONFIGURED_ENGINE_UNAVAILABLE\n$/.test(stripVTControlCharacters(stderr.data))));
  if (call?.taskId !== nativeTaskId || call.adoptedReceipt || !knownRejection) fail();
  return {tasks:new Set([ownerTaskId, tool.id, generation.id]), submissionId};
}
export async function requireIdle(configPath, options = {}) {
  const path = requireOriginal(configPath);
  const storage = await openNodeSqliteStorage(path);
  try {
    const exited = options.nativeFailure ? await exitedBacktest(storage, options.nativeFailure) : null;
    const record = await storage.findDocument({ kind: 'qz.application', scope: { kind: 'session' } }, 'current', context);
    const application = record ? (await storage.document(record.id, 'current', context)).value : {};
    const maintenance = application.maintenance;
    const quiesced = maintenance?.schemaVersion === 1 && maintenance.state === 'QUIESCED'
      && typeof maintenance.epoch === 'string' && maintenance.epoch.length > 0
      && Number.isSafeInteger(maintenance.barrierTaskId) && maintenance.barrierTaskId > 0
      && typeof maintenance.quiescedAtNs === 'string' && /^\d+$/.test(maintenance.quiescedAtNs)
      && maintenance.parked && typeof maintenance.parked === 'object' && !Array.isArray(maintenance.parked);
    const recovering = exited && maintenance?.schemaVersion === 1 && maintenance.state === 'DRAINING'
      && maintenance.epoch === options.maintenanceEpoch && typeof options.maintenanceEpoch === 'string'
      && options.maintenanceEpoch.length > 0 && Object.keys(maintenance.parked ?? {}).length === 0;
    if (exited && !recovering) throw new Error('Original recovery maintenance epoch is not DRAINING');
    if (maintenance && maintenance.state !== 'RUNNING' && !quiesced && !recovering) throw new Error(`Maintenance is not formally QUIESCED in ${path}; resume the original service and finish draining it.`);
    const parkKinds = new Set(['qz.pm.mandate-monitor', 'qz.account-adjustment']);
    const observedParked = new Set(); let observedGate = false;
    for (const status of ['pending', 'running', 'waiting', 'sleeping', 'completing']) {
      let cursor;
      do {
        const page = await storage.scanTasks({ status }, 100, cursor, context);
        for (const task of page.items) {
          const checkpoint = task.state.checkpoint;
          const original = quiesced ? maintenance.parked[String(task.id)] : undefined;
          const common = (quiesced || recovering) && task.version === 1 && task.background === true && task.abortRequested === false;
          const gate = common && task.id === maintenance.barrierTaskId && task.kind === 'qz.maintenance.gate'
            && task.input?.epoch === maintenance.epoch && ['pending', 'running'].includes(status) && checkpoint?.phase === 'hold';
          const parked = common && original?.taskId === task.id && original.kind === task.kind && original.epoch === maintenance.epoch
            && parkKinds.has(task.kind) && status === 'waiting' && task.state.policy === 'allSettled'
            && task.state.on.length === 1 && task.state.on[0] === maintenance.barrierTaskId
            && checkpoint?.phase === 'maintenance' && checkpoint.maintenanceEpoch === maintenance.epoch
            && isDeepStrictEqual(checkpoint.resumeCheckpoint, original.resumeCheckpoint);
          if (exited?.tasks.has(task.id)) continue;
          if (gate) observedGate = true;
          else if (parked) observedParked.add(String(task.id));
          else throw new Error(`Unresolved durable task ${task.id} (${status}) in ${path}; only original tasks parked for the exact QUIESCED epoch may pass maintenance.`);
        }
        cursor = page.next;
      } while (cursor);
    }
    if ((quiesced || recovering) && (!observedGate || Object.keys(maintenance.parked).some(id => !observedParked.has(id)))) throw new Error(`Incomplete maintenance epoch in ${path}; original gate and parked task identities must still exist.`);
    for (const status of ['queued', 'placed']) {
      let cursor;
      do {
        const page = await storage.scanSubmissions({status}, 100, cursor, context);
        const blocker = page.items.find(item => item.id !== exited?.submissionId);
        if (blocker) throw new Error(`Unresolved research submission ${blocker.id} in ${path}; no queued work was discarded.`);
        cursor = page.next;
      } while (cursor);
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
      for (const [id, command] of Object.entries(application.strategyCommands ?? {})) {
        if (['PENDING', 'UNCERTAIN'].includes(command.status) || command.receipt?.persisted === false || ['PENDING', 'UNCERTAIN'].includes(command.receipt?.status)) {
          throw new Error(`Unresolved native strategy command ${id} in ${path}; reconcile its original command receipt, not the current strategy status.`);
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
  else if (process.argv[2] === 'exited-preflight') {
    let input = ''; for await (const part of process.stdin) input += part;
    const {selection, status, detail, owner} = JSON.parse(input);
    console.log(JSON.stringify(requireExitedPreflight(selection, status, detail, owner)));
  } else {
    const options = process.argv[2] === 'exited-backtest' ? JSON.parse(process.argv[3]) : {};
    await requireIdle(configPath, options);
    console.log(options.nativeFailure ? 'Original exited native job wait chain retained for upgrade; no reconciliation, cancellation or retry performed.' : 'Original durable database is idle; no cancellation was inferred.');
  }
}
