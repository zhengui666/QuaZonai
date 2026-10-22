import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { ConversationId } from '@earendil-works/pi-durable';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical } from '../../research/src/documents.ts';
import type { ServiceConfig } from './config.ts';
import { Application, counter, decimal, decimalUnits, json, object, observation, portfolio, Problem, text } from './facts.ts';
import { accountKey, CapitalExitCommand, capitalStatusForBinding, control, readCapitalControl, recordCapitalControl, recordCapitalControlFailure, validateCapitalDocument } from './capital-exit.ts';
import { downstreamRequest } from './downstream.ts';

const context = BACKGROUND_CONTEXT;
export function capitalWorkbench(host: ResearchHost, config: ServiceConfig) {
  async function fact(id: string) {
    const value = (await host.harness.snapshot(Application, context))?.observations[id];
    if (!value) throw new Problem('observation_not_found', 404);
    return observation(value);
  }
  async function refreshCapital(id: string) {
    const original = await fact(id); let key = accountKey(original.connection, (await host.harness.snapshot(Application, context))?.accountControls);
    try {
      const fresh = await readCapitalControl(original.connection, original.view.currency);
      for (const exit of fresh.status.exits) await validateCapitalDocument(config, 'VIEW', exit);
      await host.harness.commit(async tx => {
        const facts = await tx.doc(Application);
        facts.accountControls ??= {}; key = recordCapitalControl(facts.accountControls!, original.connection, fresh);
      }, context);
    } catch (error) {
      await host.harness.commit(async tx => {
        const facts = await tx.doc(Application); key = accountKey(original.connection, facts.accountControls);
        const unsupported = error instanceof Problem && error.message === 'downstream_control_404';
        facts.accountControls ??= {}; recordCapitalControlFailure(facts.accountControls, original.connection, unsupported ? 'downstream_capital_exit_unsupported' : 'capital_exit_status_unavailable');
      }, context);
    }
    const state = control((await host.harness.snapshot(Application, context))?.accountControls?.[key]);
    return { ...capitalStatusForBinding(state, original.connection), commands: Object.entries(state.commands).map(([requestId, value]) => ({ requestId, state: value.state, error: value.error, result: value.result })) };
  }
  async function previewCapital(id: string, value: unknown) {
    const body = object(value); const requestId = text(body.requestId, 'request_id'); const original = await fact(id);
    let captured = original.capitalPreviews?.[requestId];
    if (captured && canonical(captured.original!) !== canonical(body)) throw new Problem('request_id_conflict', 409);
    if (!captured) {
      const status = await refreshCapital(id);
      if (status.capability !== 'SUPPORTED' || !status.accountSourceId || !status.observationId || !status.controlRevision) throw new Problem(status.reason ?? 'capital_exit_unavailable', 409);
      const amount = decimal(body.amount, 'amount'); if (decimalUnits(amount) <= 0n || body.currency !== original.view.currency) throw new Problem('capital_exit_amount_or_currency_invalid', 422);
      const request = { schema_version: 1, account_source_id: status.accountSourceId, expected_source_observation_id: status.observationId,
        scope: { kind: 'AMOUNT', amount, currency: body.currency }, policy: body.policy === undefined ? { kind: 'CASH_ONLY' } : object(body.policy) };
      await validateCapitalDocument(config, 'REQUEST', request);
      captured = await host.harness.commit(async tx => {
        const facts = await tx.doc(Application); const current = observation(facts.observations[id]);
        const prior = current.capitalPreviews?.[requestId]; if (prior && canonical(prior.original!) !== canonical(body)) throw new Problem('request_id_conflict', 409);
        current.capitalPreviews ??= {}; current.capitalPreviews[requestId] ??= json({ original: body, controlRevision: status.controlRevision, envelope: { requestId, clientId: original.connection.clientId, accountId: original.connection.accountId, request } });
        facts.observations[id] = json(current); return json(current.capitalPreviews[requestId]);
      }, context);
    }
    if (captured.preview) return captured.preview;
    const envelope = object(captured.envelope); const request = object(envelope.request);
    const response = await downstreamRequest(original.connection, 'capital-exits/preview', 'POST', envelope);
    if (response.requestId !== requestId || response.clientId !== original.connection.clientId || response.accountId !== original.connection.accountId) throw new Problem('capital_exit_preview_identity_mismatch', 409);
    const preview = object(response.preview);
    await validateCapitalDocument(config, 'PREVIEW', preview);
    if (preview.account_source_id !== request.account_source_id || preview.original_observation_id !== request.expected_source_observation_id || preview.expected_account_control_revision !== captured.controlRevision
      || canonical(preview.scope!) !== canonical(json(request.scope)) || canonical(preview.policy!) !== canonical(json(request.policy)) || preview.environment !== (original.connection.mode === 'paper' ? 'PAPER' : 'LIVE')) throw new Problem('capital_exit_preview_binding_mismatch', 409);
    await host.harness.commit(async tx => {
      const facts = await tx.doc(Application); const current = observation(facts.observations[id]);
      const prior = current.capitalPreviews?.[requestId];
      if (prior && canonical(prior.original!) !== canonical(body)) throw new Problem('request_id_conflict', 409);
      current.capitalPreviews![requestId] = json({ ...prior, preview }); facts.observations[id] = json(current);
    }, context);
    return preview;
  }
  async function commandCapital(id: string, value: unknown) {
    const original = object(value); const requestId = text(original.requestId, 'request_id'); const current = await fact(id);
    const status = await refreshCapital(id); const controls = (await host.harness.snapshot(Application, context))?.accountControls; const key = accountKey(current.connection, controls);
    const old = control(controls?.[key]).commands[requestId];
    if (old) { if (canonical(old.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return { requestId, state: old.state, result: old.result }; }
    if (status.capability !== 'SUPPORTED') throw new Problem(status.reason ?? 'capital_exit_unavailable', 409);
    const previews = Object.values(current.capitalPreviews ?? {}).filter(value => value.preview).map(value => object(value.preview));
    let native: import('@earendil-works/pi-durable').JsonObject; let path: string; let reserved = '0';
    if (original.action === 'START') {
      const preview = previews.find(value => value.id === original.previewId);
      if (!preview || preview.capability !== 'SUPPORTED' || Date.parse(String(preview.valid_until)) <= Date.now()) throw new Problem('capital_exit_preview_invalid_or_expired', 409);
      if (preview.expected_account_control_revision !== status.controlRevision || preview.original_observation_id !== status.observationId) throw new Problem('capital_exit_preview_stale', 409);
      native = { schema_version: 1, preview_id: preview.id!, expected_account_control_revision: counter(preview.expected_account_control_revision, 'control_revision'),
        acknowledged_plan_artifact_id: preview.plan_artifact_id!, expected_source_observation_id: preview.original_observation_id! };
      await validateCapitalDocument(config, 'START', native);
      reserved = decimal(object(object(preview.funds).requested).amount, 'requested_amount'); path = 'capital-exits';
    } else {
      const exitId = text(original.exitId, 'capital_exit_id'); const exit = status.exits.find(value => value.id === exitId);
      if (!exit) throw new Problem('capital_exit_not_found', 404);
      if (original.expectedRevision !== exit.revision) throw new Problem('capital_exit_revision_conflict', 409);
      native = { schema_version: 1, expected_revision: exit.revision, action: text(original.action, 'action') };
      if (original.action === 'RESUME') {
        const preview = previews.find(value => value.id === original.previewId);
        if (!preview || preview.capability !== 'SUPPORTED' || Date.parse(String(preview.valid_until)) <= Date.now()) throw new Problem('capital_exit_preview_invalid_or_expired', 409);
        native.preview_id = preview.id!;
      }
      if (original.action === 'RECONCILE_WITHDRAWAL') {
        const amount = decimal(original.amount, 'withdrawal_amount'); if (decimalUnits(amount) <= 0n || original.currency !== current.view.currency) throw new Problem('capital_exit_amount_or_currency_invalid', 422);
        native.user_reported_amount = amount; native.currency = current.view.currency;
        native.external_transfer_ref = text(original.externalTransferRef, 'external_transfer_ref');
      }
      await validateCapitalDocument(config, 'ACTION', native); path = `capital-exits/${encodeURIComponent(exitId)}/actions`;
    }
    return host.harness.commit(async tx => {
      const facts = await tx.doc(Application); const state = control(facts.accountControls?.[key]);
      const prior = state.commands[requestId];
      if (prior) { if (canonical(prior.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return json({ requestId, state: prior.state, result: prior.result }); }
      if (Object.values(state.commands).some(value => ['PENDING', 'UNCERTAIN'].includes(value.state))) throw new Problem('capital_exit_command_requires_reconciliation', 409);
      if (state.status?.controlRevision !== status.controlRevision) throw new Problem('capital_exit_revision_conflict', 409);
      // Local reservation and durable command admission are one transaction, before any unsafe POST.
      const taskId = await tx.createTask(CapitalExitCommand, { key, requestId, connection: current.connection, reconcile: false, binary: config.jobBinary, dataDir: config.dataDir },
        { conversationId: portfolio(facts.portfolios[current.input.portfolioId]).view.researchIds[0] as ConversationId, ownership: { kind: 'conversation' }, background: true });
      state.commands[requestId] = { original: json(original), body: native, path, reserved, state: 'PENDING', result: null, error: null, taskId };
      facts.accountControls ??= {}; facts.accountControls[key] = json(state); return { requestId, state: 'PENDING', result: null };
    }, context);
  }
  return { refreshCapital, previewCapital, commandCapital };
}
