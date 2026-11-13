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
  async function commandCapital(id: string, value: unknown) {
    const original = object(value); const requestId = text(original.requestId, 'request_id'); const current = await fact(id);
    const priorControls = (await host.harness.snapshot(Application, context))?.accountControls;
    const existing = control(priorControls?.[accountKey(current.connection, priorControls)]).commands[requestId];
    if (existing) { if (canonical(existing.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return { requestId, state: existing.state, result: existing.result }; }
    if(!['PAUSE','CANCEL'].includes(String(original.action)))throw new Problem('capital_exit_planning_retired',410);
    const status = await refreshCapital(id); const controls = (await host.harness.snapshot(Application, context))?.accountControls; const key = accountKey(current.connection, controls);
    const action = text(original.action, 'action') as CapitalExitAction;
    const permissionBasis = capitalCommandPermission(control(controls?.[key]), current.connection, action);
    if (canonical(physicalCapitalState(permissionBasis.status)) !== canonical(physicalCapitalState(status))) throw new Problem('capital_exit_permission_basis_changed', 409);
    const exitId=text(original.exitId,'capital_exit_id');const exit=status.exits.find(value=>value.id===exitId);
    if(!exit)throw new Problem('capital_exit_not_found',404);
    if(original.expectedRevision!==exit.revision)throw new Problem('capital_exit_revision_conflict',409);
    const availability=exit.action_availability[action];if(availability?.allowed!==true)throw new Problem(availability?.reason??'capital_exit_action_unavailable',409);
    const native={schema_version:2,expected_revision:exit.revision,action};
    await validateCapitalDocument(config,'ACTION',native);
    const path=`capital-exits/${encodeURIComponent(exitId)}/actions`,reserved='0';
    return host.harness.commit(async tx => {
      const facts = await tx.doc(Application); const state = control(facts.accountControls?.[key]);
      const prior = state.commands[requestId];
      if (prior) { if (canonical(prior.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return json({ requestId, state: prior.state, result: prior.result }); }
      assertCapitalCommandPermission(state, current.connection, permissionBasis);
      // Stop-only admission may proceed while older intents remain uncertain; their reservations are retained.
      if (state.status?.controlRevision !== status.controlRevision) throw new Problem('capital_exit_revision_conflict', 409);
      const owner = await ensureCapitalWatch(tx, current.connection, current.view.currency);
      const taskId = await tx.createTask(CapitalExitCommand, { key, requestId, connection: current.connection, reconcile: false, binary: config.jobBinary, dataDir: config.dataDir },
        { conversationId: owner.conversationId, ownership: { kind: 'conversation' }, background: true });
      const admitted = control(facts.accountControls![owner.key]);
      admitted.commands[requestId] = { original: json(original), body: native, path, reserved, state: 'PENDING', result: null, error: null, taskId, permissionBasis };
      facts.accountControls![owner.key] = json(admitted); return { requestId, state: 'PENDING', result: null };
    }, context);
  }
  return { refreshCapital, getCapitalExit, commandCapital,
    previewFixedRisk: (id: string, value: unknown) => previewNative(id, value, 'sizing'),
    previewOrder: (id: string, value: unknown) => previewNative(id, value, 'order') };
}
