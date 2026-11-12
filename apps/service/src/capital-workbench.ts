import { nativeFixedRiskSizing } from '../../../packages/contracts/src/nautilus.ts';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical } from '../../research/src/documents.ts';
import type { CapitalExitAction } from '../../../packages/contracts/src/capital-exit.ts';
import type { ServiceConfig } from './config.ts';
import { Application, counter, decimal, decimalUnits, json, object, observation, Problem, text } from './facts.ts';
import { accountKey, assertCapitalCommandPermission, assertEnvelope, capitalCommandPermission, CapitalExitCommand, capitalStatusForBinding, control, physicalCapitalState, readCapitalControl, recordCapitalControl, recordCapitalControlFailure, validateCapitalDocument } from './capital-exit.ts';
import { ensureCapitalWatch } from './capital-watch.ts';
import { downstreamRequest } from './downstream.ts';

const context = BACKGROUND_CONTEXT;
export function capitalWorkbench(host: ResearchHost, config: ServiceConfig) {
  async function fact(id: string) {
    const value = (await host.harness.snapshot(Application, context))?.observations[id];
    if (!value) throw new Problem('observation_not_found', 404);
    return observation(value);
  }
  /** Hypothetical native previews only; never reserve capital, submit orders or enable automation. */
  async function previewNative(id: string, value: unknown, kind: 'sizing' | 'order') {
    const original = await fact(id);
    const body = object(value, 'native_preview_request');
    const allowed = kind === 'sizing' ? ['sizing'] : ['instrument_id', 'order'];
    if (Object.keys(body).some(key => !allowed.includes(key))) throw new Problem('native_preview_unknown_field');
    if (kind === 'sizing') {
      try { body.sizing = { ...nativeFixedRiskSizing(body.sizing) }; }
      catch (error) { throw new Problem(error instanceof Error ? error.message : 'native_sizing_invalid'); }
    }
    const instrument = kind === 'sizing' ? text(object(body.sizing).instrument_id, 'instrument_id') : text(body.instrument_id, 'instrument_id');
    const response = await downstreamRequest(original.connection, kind === 'sizing' ? 'sizing/fixed-risk' : 'orders/preview', 'POST', {
      ...body, clientId: original.connection.clientId, accountId: original.connection.accountId,
    });
    assertEnvelope(original.connection, response, original.view.currency);
    if (response.preview_only !== true) throw new Problem('native_preview_response_invalid', 502);
    if (kind === 'sizing') {
      if (response.instrument_id !== instrument || response.model !== 'NAUTILUS_FIXED_RISK'
        || response.input_source !== 'REQUEST_SCENARIO' || response.account_balance_verified !== false
        || typeof response.quantity !== 'string') throw new Problem('native_preview_response_invalid', 502);
    } else {
      if (response.submitted !== false || response.automatic_protection_enabled !== false || !Array.isArray(response.orders)
        || response.orders.length === 0 || response.orders.some(row => !row || typeof row !== 'object' || Array.isArray(row) || row.instrument_id !== instrument)) throw new Problem('native_preview_response_invalid', 502);
    }
    return response;
  }
  async function refreshCapital(id: string) {
    const original = await fact(id); let key = accountKey(original.connection, (await host.harness.snapshot(Application, context))?.accountControls);
    try {
      const fresh = await readCapitalControl(original.connection, original.view.currency);
      await host.harness.commit(async tx => {
        const facts = await tx.doc(Application);
        facts.accountControls ??= {}; key = recordCapitalControl(facts.accountControls, original.connection, fresh);
        if (fresh.status.exits.some(exit => !['COMPLETED', 'CLOSED'].includes(exit.state))) await ensureCapitalWatch(tx, original.connection, original.view.currency);
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
  async function planningContext(id: string) {
    const original = await fact(id);
    const response = await downstreamRequest(original.connection, 'capital-exits/planning-context', 'GET');
    assertEnvelope(original.connection, response, original.view.currency);
    const plan = object(response.context);
    await validateCapitalDocument(config, 'CONTEXT', plan);
    if (plan.currency !== original.view.currency || plan.environment !== (original.connection.mode === 'paper' ? 'PAPER' : 'LIVE')) throw new Problem('capital_exit_context_binding_mismatch', 409);
    await host.harness.commit(async tx => {
      const app = await tx.doc(Application); const binding = observation(app.observations[id]);
      binding.capitalPlanningContexts = Object.fromEntries(Object.entries(binding.capitalPlanningContexts ?? {}).filter(([, value]) => Date.parse(String(value.valid_until)) > Date.now()));
      binding.capitalPlanningContexts[String(plan.id)] = json(plan);
      app.observations[id] = json(binding);
    }, context);
    return plan;
  }
  async function getCapitalExit(id: string, exitId: string) {
    const original = await fact(id);
    const response = await downstreamRequest(original.connection, `capital-exits/${encodeURIComponent(exitId)}`, 'GET');
    assertEnvelope(original.connection, response, original.view.currency);
    const exit = object(response.exit); await validateCapitalDocument(config, 'VIEW', exit);
    const controls = (await host.harness.snapshot(Application, context))?.accountControls;
    const known = control(controls?.[accountKey(original.connection, controls)]).status;
    if (exit.id !== exitId || known?.accountSourceId && exit.account_source_id !== known.accountSourceId
      || exit.environment !== (original.connection.mode === 'paper' ? 'PAPER' : 'LIVE') || object(exit.scope).currency !== original.view.currency) throw new Problem('capital_exit_identity_mismatch', 409);
    return exit;
  }
  async function previewCapital(id: string, value: unknown) {
    const body = object(value); const requestId = text(body.requestId, 'request_id'); const original = await fact(id);
    let captured = original.capitalPreviews?.[requestId];
    if (captured && canonical(captured.original!) !== canonical(body)) throw new Problem('request_id_conflict', 409);
    if (!captured) {
      const status = await refreshCapital(id);
      if (status.capability !== 'SUPPORTED' || !status.accountSourceId || !status.observationId || !status.controlRevision) throw new Problem(status.reason ?? 'capital_exit_unavailable', 409);
      const plan = original.capitalPlanningContexts?.[String(body.contextId)];
      if (!plan || body.contextId !== plan.id || plan.account_source_id !== status.accountSourceId || plan.account_control_revision !== status.controlRevision || Date.parse(String(plan.valid_until)) <= Date.now()) throw new Problem('capital_exit_context_stale', 409);
      const amount = decimal(body.amount, 'amount'); if (decimalUnits(amount) <= 0n || body.currency !== original.view.currency) throw new Problem('capital_exit_amount_or_currency_invalid', 422);
      const policy = body.policy === undefined ? { kind: 'CASH_ONLY' } : object(body.policy);
      if (!status.supportedPolicies?.includes(String(policy.kind) as 'CASH_ONLY' | 'NATURAL_RELEASE' | 'BOUNDED_LIMIT')) throw new Problem('capital_exit_policy_unsupported', 409);
      if (body.resumeExitId !== undefined || body.expectedExitRevision !== undefined) {
        const exit = status.exits.find(item => item.id === body.resumeExitId);
        if (!exit || exit.revision !== body.expectedExitRevision) throw new Problem('capital_exit_revision_conflict', 409);
        if (decimalUnits(amount) !== decimalUnits(exit.funds.reserved_amount.amount)) throw new Problem('capital_exit_resume_amount_mismatch', 422);
      }
      const request = { schema_version: 2, account_source_id: status.accountSourceId, expected_source_observation_id: plan.observation_id,
        expected_account_control_revision: status.controlRevision, planning_context_id: text(body.contextId, 'context_id'),
        scope: { kind: 'AMOUNT', amount, currency: body.currency }, policy,
        ...(body.resumeExitId !== undefined ? { resume_exit_id: text(body.resumeExitId, 'resume_exit_id'), expected_exit_revision: counter(body.expectedExitRevision, 'exit_revision') } : {}) };
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
    const preview = object(response.preview); await validateCapitalDocument(config, 'PREVIEW', preview);
    if (preview.account_source_id !== request.account_source_id || preview.original_observation_id !== request.expected_source_observation_id || preview.expected_account_control_revision !== captured.controlRevision
      || preview.resume_exit_id !== request.resume_exit_id || preview.expected_exit_revision !== request.expected_exit_revision
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
    const priorControls = (await host.harness.snapshot(Application, context))?.accountControls;
    const existing = control(priorControls?.[accountKey(current.connection, priorControls)]).commands[requestId];
    if (existing) { if (canonical(existing.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return { requestId, state: existing.state, result: existing.result }; }
    const status = await refreshCapital(id); const controls = (await host.harness.snapshot(Application, context))?.accountControls; const key = accountKey(current.connection, controls);
    const action = text(original.action, 'action') as CapitalExitAction;
    const permissionBasis = capitalCommandPermission(control(controls?.[key]), current.connection, action);
    if (canonical(physicalCapitalState(permissionBasis.status)) !== canonical(physicalCapitalState(status))) throw new Problem('capital_exit_permission_basis_changed', 409);
    const previews = Object.values(current.capitalPreviews ?? {}).filter(value => value.preview).map(value => object(value.preview));
    let native: import('@earendil-works/pi-durable').JsonObject; let path: string; let reserved = '0';
    if (action === 'START') {
      const preview = previews.find(value => value.id === original.previewId);
      if (!preview || preview.resume_exit_id !== undefined || preview.capability !== 'SUPPORTED' || Date.parse(String(preview.valid_until)) <= Date.now()) throw new Problem('capital_exit_preview_invalid_or_expired', 409);
      if (preview.expected_account_control_revision !== status.controlRevision || preview.account_source_id !== status.accountSourceId) throw new Problem('capital_exit_preview_stale', 409);
      native = { schema_version: 2, preview_id: preview.id!, expected_account_control_revision: counter(preview.expected_account_control_revision, 'control_revision'),
        acknowledged_plan_artifact_id: preview.plan_artifact_id!, expected_source_observation_id: preview.original_observation_id! };
      await validateCapitalDocument(config, 'START', native);
      reserved = decimal(object(object(preview.funds).requested).amount, 'requested_amount'); path = 'capital-exits';
    } else {
      const exitId = text(original.exitId, 'capital_exit_id'); const exit = status.exits.find(value => value.id === exitId);
      if (!exit) throw new Problem('capital_exit_not_found', 404);
      if (original.expectedRevision !== exit.revision) throw new Problem('capital_exit_revision_conflict', 409);
      const availability = exit.action_availability[action]; if (availability?.allowed !== true) throw new Problem(availability?.reason ?? 'capital_exit_action_unavailable', 409);
      native = { schema_version: 2, expected_revision: exit.revision, action };
      if (action === 'RESUME') {
        const preview = previews.find(value => value.id === original.previewId);
        if (!preview || preview.capability !== 'SUPPORTED' || Date.parse(String(preview.valid_until)) <= Date.now()
          || preview.resume_exit_id !== exitId || preview.expected_exit_revision !== exit.revision || preview.expected_account_control_revision !== status.controlRevision
          || preview.account_source_id !== status.accountSourceId) throw new Problem('capital_exit_preview_invalid_or_expired', 409);
        native.preview_id = preview.id!;
      }
      if (action === 'RECONCILE_WITHDRAWAL' || action === 'RECONCILE_EXTERNAL_TRANSFER') {
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
      assertCapitalCommandPermission(state, current.connection, permissionBasis);
      if (Object.values(state.commands).some(value => ['PENDING', 'UNCERTAIN'].includes(value.state))) throw new Problem('capital_exit_command_requires_reconciliation', 409);
      if (state.status?.controlRevision !== status.controlRevision) throw new Problem('capital_exit_revision_conflict', 409);
      const owner = await ensureCapitalWatch(tx, current.connection, current.view.currency);
      const taskId = await tx.createTask(CapitalExitCommand, { key, requestId, connection: current.connection, reconcile: false, binary: config.jobBinary, dataDir: config.dataDir },
        { conversationId: owner.conversationId, ownership: { kind: 'conversation' }, background: true });
      const admitted = control(facts.accountControls![owner.key]);
      admitted.commands[requestId] = { original: json(original), body: native, path, reserved, state: 'PENDING', result: null, error: null, taskId, permissionBasis };
      facts.accountControls![owner.key] = json(admitted); return { requestId, state: 'PENDING', result: null };
    }, context);
  }
  return { refreshCapital, planningContext, getCapitalExit, previewCapital, commandCapital,
    previewFixedRisk: (id: string, value: unknown) => previewNative(id, value, 'sizing'),
    previewOrder: (id: string, value: unknown) => previewNative(id, value, 'order') };
}
