import { ensureCapitalWatch } from './capital-watch.ts';
import { bindScienceData, scienceDataWithoutBinding, scienceTarget } from './config.ts';
import type { ProcessNativeTarget, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { ConversationId, Cursor, TaskId, Tx } from '@earendil-works/pi-durable';
import { randomUUID } from 'node:crypto';
import type { ResearchHost } from '../../research/src/host.ts';
import type { ResearchInput as NativeResearchInput } from '../../research/src/documents.ts';
import { canonical, Requests, Study } from '../../research/src/documents.ts';
import { researchInput } from '../../research/src/input.ts';
import { isRecoveryRequestId } from '../../research/src/recovery.ts';
import { strategyMembers, validateNativeSimulationSettings } from '../../research/src/strategy.ts';
import type { ServiceConfig } from './config.ts';
import { portfolioRuntime } from './runtime-views.ts';
import { readResearchAdmissions } from './admissions.ts';
import { Application, counter, decimal, decimalUnits, json, nowNs, nsTime, object, observation, portfolio, Problem, settings, text } from './facts.ts';
import type { ObservationFact, PortfolioFact } from './facts.ts';
import { factorReference, factorViews } from './factors.ts';
import { publicLiteratureQueries } from '../../../packages/contracts/src/research.ts';
import { nativeResearchInput as parseNativeResearchInput, validateNativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
import { assertNativeRemoteTaskSettled, reconcileNativeRemoteTasks } from '../../research/src/native-remote-receipts.ts';
import { ObserveAccount } from './science.ts';
import { assertCurrentReviewSource, prepareReviewSourceEvidence } from './review-source.ts';
import type { CapturedReviewSource } from '../../research/src/review-source.ts';
import { unsettledAdjustment } from './automatic.ts';
import { PmDocument, assertPmNotDeleted, pmRecord, pmState } from './pm-facts.ts';
import { assertBusinessAdmission, maintenanceState } from './pm-maintenance.ts';
import { accountKey, capitalStatusForBinding, control, readCapitalControl, recordCapitalControl, recordCapitalControlFailure, validateCapitalDocument } from './capital-exit.ts';
import { acceptSnapshot, downstreamRequest } from './downstream.ts';
import type { ResearchInput, ResearchStart, SettingsView, DeletionReceipt } from '../../../packages/contracts/src/index.ts';

const context = BACKGROUND_CONTEXT;
function nativeResearchInput(data: Record<string, unknown>) {
  try { return parseNativeResearchInput(data); }
  catch (error) { throw new Problem(error instanceof Error ? error.message : 'native_request_invalid'); }
}
function portfolioUsesFactor(fact: PortfolioFact, id: string): boolean {
  if (fact.input.factors) return fact.input.factors.some(member => member.factorId === id);
  const reference = factorReference(id);
  if (!fact.view.researchIds.includes(reference.researchId)) return false;
  const evidence = fact.evidence.filter(item => item.researchId === reference.researchId);
  if (!evidence.length) return true; // Unexplained legacy lineage remains protected.
  return evidence.some(item => {
    try {
      const selected = object(item.selected);
      if (!Number.isSafeInteger(selected.taskId)) return true;
      const members = strategyMembers(object(selected.proposal));
      return selected.taskId === reference.trialId && reference.memberIndex < members.length;
    } catch { return true; }
  });
}
export function workbench(host: ResearchHost, config: ServiceConfig, resolveRunner?: (requestId: string, revision: number) => Promise<ProcessNativeTarget>) {
  const snapshot = () => host.harness.snapshot(Application, context);
  const getSettings = async (): Promise<SettingsView> => settings((await snapshot())!.settings);
  const dataset = (id: string) => {
    const found = config.datasets.find((dataset) => dataset.id === id);
    if (!found) throw new Problem('dataset_not_found', 404);
    if (!found.ready) throw new Problem('dataset_not_ready', 409);
    return found;
  };
  async function startResearch(value: unknown, retainedNautilusTarget?: RemoteNativeTarget) {
    const body = object(value); const requestId = text(body.requestId, 'request_id');
    if (Object.keys(body).some(key => !['requestId', 'input', 'goal', 'model', 'datasetId', 'specification', 'publicLiteratureQueries', 'thinkingLevel', 'serviceTier', 'runnerBuildRequestId', 'expectedRunnerRevision'].includes(key))) throw new Problem('research_field_unsupported');
    const original = json(body); const old = (await readResearchAdmissions(host))[`request:${requestId}`];
    const nativeAdmission = (await host.harness.snapshot(Requests, context))?.requests[`request:${requestId}`];
    const priorInput = (old?.input ?? nativeAdmission?.input) as NativeResearchInput | undefined;
    if (priorInput?.campaign) await host.harness.commit(async tx => assertPmNotDeleted(pmRecord(pmState(await tx.doc(PmDocument)), priorInput.campaign!.mandateRevisionId)), context);
    if (nativeAdmission?.recovery || !nativeAdmission && !old && isRecoveryRequestId(requestId)) throw new Problem('request_id_conflict', 409);
    if (old) {
      if (canonical(old.original) !== canonical(original)) throw new Problem('request_id_conflict', 409);
      return host.start(requestId, old.input as NativeResearchInput, undefined, old.sourceEvidence as unknown as CapturedReviewSource | undefined, nativeAdmission?.metadata);
    }
    let selectedTarget: ProcessNativeTarget | undefined;
    if (body.runnerBuildRequestId !== undefined || body.expectedRunnerRevision !== undefined) {
      if (!resolveRunner || typeof body.runnerBuildRequestId !== 'string' || !body.runnerBuildRequestId || !Number.isSafeInteger(body.expectedRunnerRevision) || retainedNautilusTarget) throw new Problem('research_runner_binding_invalid',422);
      selectedTarget = await resolveRunner(body.runnerBuildRequestId, Number(body.expectedRunnerRevision));
    }
    const submission = body as unknown as ResearchStart;
    let input: ResearchInput;
    let sourceDataset: ServiceConfig['datasets'][number] | undefined;
    let capturedDataset: import('../../../packages/contracts/src/index.ts').RecordedDataset | undefined;
    const captureDataset = (configured: ServiceConfig['datasets'][number]) => {
      const { id, name, assetClass, currency, source, start, end, preparation, inputCoverage } = configured;
      const nativeModality = nativeResearchInput(configured.data)?.modality ?? configured.nativeModality;
      return { id, name, assetClass, currency, source, start, end, ...(nativeModality ? { nativeModality } : {}), ...(preparation ? { preparation } : {}), ...(inputCoverage ? { inputCoverage } : {}) };
    };
    if ('input' in submission) {
      const native = (await host.harness.snapshot(Requests, context))?.requests[`request:${requestId}`];
      if (native) {
        if (canonical(native.input) !== canonical(json(submission.input))) throw new Problem('request_id_conflict', 409);
        return host.start(requestId, native.input as NativeResearchInput, undefined, native.sourceEvidence as unknown as CapturedReviewSource | undefined, native.metadata);
      }
      input = structuredClone(submission.input);
      object(input, 'research_input'); text(input.goal, 'goal');
      publicLiteratureQueries(input.publicLiteratureQueries);
      const data = object(input.data, 'dataset');
      const registered = config.datasets.find((dataset) => dataset.ready && canonical((scienceDataWithoutBinding(json(dataset.data)))) === canonical((scienceDataWithoutBinding(data))));
      if (!registered) throw new Problem('dataset_input_unregistered');
      sourceDataset = registered;
      input.data = retainedNautilusTarget ? bindScienceData(registered.data, retainedNautilusTarget) : structuredClone(registered.data);
      if (input.specification) {
        const inherited = { ...registered.policy, ...(registered.strategy ? { strategy: json(registered.strategy) } : {}) };
        if (canonical((json(input.policy))) !== canonical((json(inherited)))) throw new Problem('research_policy_unregistered');
        capturedDataset = captureDataset(registered);
      }
    } else {
      const publicQueries = publicLiteratureQueries(submission.publicLiteratureQueries);
      const configured = dataset(text(submission.datasetId, 'dataset_id'));
      sourceDataset = configured;
      capturedDataset = captureDataset(configured);
      if (submission.serviceTier !== undefined && !['default', 'priority'].includes(submission.serviceTier)) throw new Problem('invalid_service_tier');
      const preferences = await getSettings();
      const preferredModel = preferences.defaultModel?.provider === submission.model?.provider && preferences.defaultModel?.modelId === submission.model?.modelId;
      const thinkingLevel = submission.thinkingLevel ?? (preferredModel ? preferences.thinkingLevel : null);
      const serviceTier = submission.serviceTier ?? (preferredModel ? preferences.serviceTier : null);
      input = { goal: text(submission.goal, 'goal'), ...(submission.specification ? { specification: structuredClone(submission.specification) } : {}), ...(publicQueries ? { publicLiteratureQueries: publicQueries } : {}), model: submission.model, ...(thinkingLevel !== null && thinkingLevel !== undefined ? { thinkingLevel } : {}), ...(serviceTier !== null && serviceTier !== undefined ? { serviceTier } : {}), data: structuredClone(configured.data),
        policy: { ...configured.policy, ...(configured.strategy ? { strategy: json(configured.strategy) } : {}) } };
    }
    if (selectedTarget) input.data = bindScienceData(scienceDataWithoutBinding(input.data), selectedTarget);
    const model = object(input.model, 'model'); text(model.provider, 'provider'); text(model.modelId, 'model_id');
    const available = await host.availableModels(String(model.provider));
    const selectedModel = available.find(item => item.modelId === model.modelId);
    if (!selectedModel) throw new Problem('model_unavailable', 409);
    if (input.thinkingLevel !== undefined && !selectedModel.thinkingLevels.includes(input.thinkingLevel)) throw new Problem('thinking_level_unsupported');
    if (input.serviceTier !== undefined && !['default', 'priority'].includes(input.serviceTier)) throw new Problem('invalid_service_tier');
    if (input.serviceTier !== undefined && !selectedModel.serviceTiers.includes(input.serviceTier)) throw new Problem('service_tier_unsupported');
    const policy = object(input.policy, 'policy');
    if (['reviewedFactorSources', 'reviewPolicy', 'reviewPolicies'].some(key => policy[key] !== undefined)) throw new Problem('research_policy_unsupported', 400);
    const validated = researchInput(input);
    const sourceEvidence = sourceDataset ? await prepareReviewSourceEvidence(sourceDataset) : null;
    if (sourceDataset && sourceEvidence) await assertCurrentReviewSource(sourceDataset, sourceEvidence);
    if (sourceDataset?.reviewSourceEvidence?.heldoutMetadata && sourceEvidence && validated.data.sealed) {
      const independent = await prepareReviewSourceEvidence(sourceDataset, 'HELDOUT');
      if (independent) { await assertCurrentReviewSource(sourceDataset, independent); sourceEvidence.independent = independent; }
    }
    const registry = host.registry.snapshot();
    if (validated.data.native_strategy_backtest_request !== undefined) {
      text(validated.data.catalog, 'catalog');
      if (validated.campaign || ['sealed', 'request', 'catalog_scan_request', 'nativeInput', 'indicator_strategy_request', 'indicator_strategy_batch_request'].some(key => validated.data[key] !== undefined)) throw new Problem('native_strategy_requires_explicit_historical_binding');
      if (!registry.extension('qz.native-strategy-backtest')) throw new Problem('native_strategy_adapter_not_configured');
      object(validated.data.native_strategy_backtest_request, 'native_strategy_backtest_request');
    }
    if (validated.data.indicator_strategy_request !== undefined || validated.data.indicator_strategy_batch_request !== undefined) throw new Problem('custom_indicator_strategy_removed', 410);
    await host.harness.commit(async tx => {
      assertBusinessAdmission(await tx.doc(Application));
      if (validated.campaign) assertPmNotDeleted(pmRecord(pmState(await tx.doc(PmDocument)), validated.campaign.mandateRevisionId));

    }, context);
    if (selectedTarget) {
      const currentTarget = await resolveRunner!(String(body.runnerBuildRequestId), Number(body.expectedRunnerRevision));
      if (canonical(json(currentTarget)) !== canonical(json(selectedTarget))) throw new Problem('research_runner_binding_changed',409);
    }
    return host.start(requestId, validated, undefined, sourceEvidence ?? undefined,
      { original, ...(capturedDataset ? { dataset: json(capturedDataset) } : {}) });
  }

  async function removeResearch(id: number, factor?: string): Promise<DeletionReceipt> {
    const deletedAt = await host.remove(id, async tx => {
      const study = await tx.doc(Study, id as ConversationId);
      if (factor && !factorViews(id, study).some(item => item.id === factor)) throw new Problem('factor_not_found', 404);
      const facts = await tx.doc(Application);
      const referenced = Object.values(facts.portfolios).map(portfolio).some(item => !item.deletedAt && (factor
        ? portfolioUsesFactor(item, factor)
        : item.view.researchIds.includes(id)));
      const kind = factor ? 'factor' : 'research';
      if (referenced) throw new Problem(`${kind}_delete_portfolio_in_use`, 409);
      const mandates = Object.values(pmState(await tx.doc(PmDocument)).mandates);
      if (mandates.some(record => !record.deletedAt && (study.input?.campaign?.mandateRevisionId === record.mandate.id
        || Object.values(record.campaigns).some(campaign => campaign.runtime.conversationId === id)))) throw new Problem(`${kind}_delete_pm_in_use`, 409);
    }, factor);
    return { kind: factor ? 'factor' : 'research', id: factor ?? String(id), deletedAt };
  }
  async function removeFactor(id: string) {
    return removeResearch(factorReference(id).researchId, id);
  }
  async function portfolioDeletionTasks(tx: Tx, id: string) {
    const facts = await tx.doc(Application); const raw = facts.portfolios[id];
    if (!raw) throw new Problem('portfolio_not_found', 404);
    const fact = portfolio(raw); const relatedIds = new Set([id]); const taskIds = new Set<number>();
    if (fact.view.taskId !== null) taskIds.add(fact.view.taskId);
    for (const observed of Object.values(facts.observations).map(observation).filter(item => item.input.portfolioId === id)) {
      relatedIds.add(observed.view.id); taskIds.add(observed.taskId);
      if (observed.automaticTaskId !== undefined) taskIds.add(observed.automaticTaskId);
    }
    const tasks = []; let cursor: Cursor | undefined;
    do {
      const page = await tx.scanTasks({}, 100, cursor);
      for (const task of page.items) {
        const input = task.input && typeof task.input === 'object' && !Array.isArray(task.input) ? task.input : null;
        if (taskIds.has(task.id) || typeof input?.id === 'string' && relatedIds.has(input.id)) tasks.push(task);
      }
      cursor = page.next;
    } while (cursor);
    return tasks;
  }
  async function removePortfolio(id: string): Promise<DeletionReceipt> {
    const tasks = await host.harness.commit(tx => portfolioDeletionTasks(tx, id), context);
    await reconcileNativeRemoteTasks(host.harness, tasks, context);
    return host.harness.commit(async tx => {
      const facts = await tx.doc(Application); const raw = facts.portfolios[id];
      if (!raw) throw new Problem('portfolio_not_found', 404);
      const fact = portfolio(raw);
      if (fact.deletedAt) return { kind: 'portfolio', id, deletedAt: fact.deletedAt };
      for (const task of await portfolioDeletionTasks(tx, id)) {
        if (task.state.status !== 'terminal') throw new Problem('portfolio_delete_tasks_active', 409);
        await assertNativeRemoteTaskSettled(tx, task.conversationId, task.id);
      }
      if (fact.view.taskId !== null && (await tx.task(fact.view.taskId as TaskId))?.state.status !== 'terminal') throw new Problem('portfolio_delete_tasks_active', 409);
      for (const observed of Object.values(facts.observations).map(observation).filter(item => item.input.portfolioId === id)) {
        const receipt = observed.view.receipt;
        if (observed.view.status !== 'STOPPED' || observed.view.automaticPolicy?.enabled || unsettledAdjustment(observed)
          || ['PENDING', 'UNCERTAIN'].includes(receipt?.status ?? '')
          || receipt?.status === 'ACKNOWLEDGED' && Date.parse(receipt.expiresAt) > Date.now()
          || (await tx.task(observed.taskId as TaskId))?.state.status !== 'terminal') throw new Problem('portfolio_delete_observation_in_use', 409);
      }
      fact.deletedAt = new Date().toISOString(); facts.portfolios[id] = json(fact);
      return { kind: 'portfolio', id, deletedAt: fact.deletedAt };
    }, context);
  }
  async function getPortfolio(id: string) {
    const found = (await snapshot())!.portfolios[id]; if (!found || portfolio(found).deletedAt) throw new Problem('portfolio_not_found', 404);
    return portfolioRuntime(host, portfolio(found));
  }
  async function getObservation(id: string) {
    const found = (await snapshot())!.observations[id]; if (!found) throw new Problem('observation_not_found', 404);
    return observation(found);
  }
  async function startAccountObservation(value: unknown) {
    const body = object(value); const requestId = text(body.requestId, 'request_id');
    if (Object.keys(body).some(key => !['requestId', 'purpose', 'mode', 'connectionId'].includes(key))) throw new Problem('account_observation_field_unsupported', 422);
    const old = Object.values((await snapshot())!.observations).map(observation).find(item => item.input.requestId === requestId);
    if (old) { if (canonical(json(old.accountOnlyRequest ?? old.input)) !== canonical(body)) throw new Problem('request_id_conflict', 409); return old.view; }
    const connection = config.connections.find(item => item.id === body.connectionId && item.mode === body.mode);
    if (!connection) throw new Problem('connection_not_configured', 422);
    const native = await downstreamRequest(connection, 'account', 'GET');
    if (native.clientId !== connection.clientId || native.accountId !== connection.accountId || native.mode !== connection.mode) throw new Problem('account_identity_mismatch', 409);
    const currency = text(native.currency, 'account_currency');
    const fresh = await readCapitalControl(connection, currency);
    const id = randomUUID();
    return host.harness.commit(async tx => {
      const facts = await tx.doc(Application); assertBusinessAdmission(facts);
      const prior = Object.values(facts.observations).map(observation).find(item => item.input.requestId === requestId);
      if (prior) { if (canonical(json(prior.accountOnlyRequest ?? prior.input)) !== canonical(body)) throw new Problem('request_id_conflict', 409); return prior.view; }
      facts.accountControls ??= {}; recordCapitalControl(facts.accountControls, connection, fresh);
      const owner = await ensureCapitalWatch(tx, connection, currency);
      const taskId = await tx.createTask(ObserveAccount, { id, connection: structuredClone(connection), target: {}, receiptId: '', refresh: true, accountOnly: true },
        { conversationId: owner.conversationId, ownership: { kind: 'conversation' }, background: true });
      const fact: ObservationFact = { accountOnlyRequest: structuredClone(body) as unknown as import('../../../packages/contracts/src/index.ts').AccountObservationStart,
        input: { requestId, purpose: 'ACCOUNT_ONLY', connectionId: connection.id, mode: connection.mode }, connection: structuredClone(connection), target: null, taskId, snapshot: null,
        view: { id, purpose: 'ACCOUNT_ONLY', name: connection.name, portfolioId: null, mode: connection.mode, status: 'OBSERVING', currency, equity: null, cash: null,
          updatedAt: null, positions: [], curve: [], receipt: null, error: null, connectionId: connection.id, connectionName: connection.name } };
      acceptSnapshot(fact, native); fact.view.capitalExit = capitalStatusForBinding(control(facts.accountControls[owner.key]), connection);
      facts.observations[id] = json(fact); return fact.view;
    }, context);
  }
  async function startObservation(value: unknown) {
    if (object(value).purpose === 'ACCOUNT_ONLY') {
      if (object(value).nativeBrackets !== undefined) throw new Problem('account_only_cannot_deliver_brackets', 422);
      return startAccountObservation(value);
    }
    throw new Problem('native_strategy_only', 410);
  }
  async function refreshObservation(id: string) {
    const original = await getObservation(id);
    // The automatic loop parks during maintenance. Outstanding PM exit duties
    // still need a fresh, GET-only account observation to establish flatness.
    const maintenance = maintenanceState((await snapshot())!);
    if ((!maintenance || maintenance.state === 'RUNNING') && original.view.automaticPolicy?.enabled && original.view.status !== 'STOPPED' && !['PENDING', 'UNCERTAIN'].includes(original.view.receipt?.status ?? '') && !unsettledAdjustment(original)) return original.view;
    const retainReadFailure = async (error: unknown) => {
      const code = error instanceof Problem ? error.message : 'account_control_refresh_failed';
      await host.harness.commit(async tx => {
        const facts = await tx.doc(Application); const value = facts.observations[id]; if (!value) return;
        const fact = observation(value); fact.view.error = code; facts.observations[id] = json(fact);
        facts.accountControls ??= {}; recordCapitalControlFailure(facts.accountControls, fact.connection, code);
      }, context);
    };
    const refreshControl = async () => {
      try {
        const fresh = await readCapitalControl(original.connection, original.view.currency);
        for (const exit of fresh.status.exits) await validateCapitalDocument(config, 'VIEW', exit);
        await host.harness.commit(async tx => {
          const facts = await tx.doc(Application); facts.accountControls ??= {};
          recordCapitalControl(facts.accountControls, original.connection, fresh);
        }, context);
      } catch (error) { await retainReadFailure(error); throw error; }
    };
    // A retained account-only source has no prior target receipt or legacy
    // portfolio to reconcile. Read it within the existing caller/PM monitor;
    // do not invent a target, receipt, portfolio or a second scheduler.
    if (!original.view.receipt && !unsettledAdjustment(original)) {
      if (maintenance?.state === 'QUIESCED') throw new Problem('service_maintenance', 503);
      try {
        const fresh = await downstreamRequest(original.connection, 'account', 'GET');
        await host.harness.commit(async tx => {
          const facts = await tx.doc(Application); const fact = observation(facts.observations[id]);
          const key = accountKey(fact.connection, facts.accountControls);
          const bindings = Object.values(facts.observations).map(observation).filter(item => accountKey(item.connection, facts.accountControls) === key);
          acceptSnapshot(fact, fresh, bindings); fact.view.error = null; facts.observations[id] = json(fact);
        }, context);
      } catch (error) { await retainReadFailure(error); throw error; }
      await refreshControl(); return (await getObservation(id)).view;
    }
    const taskId = await host.harness.commit(async (tx) => {
      const facts = await tx.doc(Application); const value = facts.observations[id]; if (!value) throw new Problem('observation_not_found', 404);
      if (maintenanceState(facts)?.state === 'QUIESCED') throw new Problem('service_maintenance', 503);
      const fact = observation(value);
      if (!fact.input.portfolioId || !fact.target) throw new Problem('account_strategy_binding_required', 409);
      const previous = await tx.task(fact.taskId as TaskId);
      if (!fact.view.automaticPolicy && previous && previous.state.status !== 'terminal') return fact.taskId;
      const originalUnsettled = ['PENDING', 'UNCERTAIN'].includes(fact.view.receipt?.status ?? '');
      const allocated = !originalUnsettled && !!fact.view.automaticPolicy && !!fact.view.allocation?.receipt && fact.view.allocation.receipt.status !== 'NOT_OFFERED' && (fact.view.allocation.receipt.status !== 'PENDING' || fact.view.allocation.status !== 'SIZED' && unsettledAdjustment(fact));
      const refreshId = await tx.createTask(ObserveAccount, { id, connection: fact.connection, target: allocated ? fact.allocationTarget! : fact.target, receiptId: allocated ? fact.view.allocation!.receipt!.id : fact.view.receipt!.id, refresh: true,
        ...(fact.view.automaticPolicy ? { accountOnly: !allocated && !originalUnsettled, ...(allocated ? { allocation: fact.allocationContext } : {}) } : {}) },
        { conversationId: portfolio(facts.portfolios[fact.input.portfolioId]).view.researchIds[0] as ConversationId, ownership: { kind: 'conversation' }, ...(fact.view.automaticPolicy ? { background: true } : {}) });
      if (!previous || previous.state.status === 'terminal') fact.taskId = refreshId;
      facts.observations[id] = json(fact); return refreshId;
    }, context);
    await host.harness.waitForTask(taskId as TaskId, context);
    // Reconcile the original receipt first, including UNCERTAIN responses. A
    // failed capability read must not suppress that existing responsibility.
    await refreshControl(); return (await getObservation(id)).view;
  }
  async function configureAutomatic(_id: string, _value: unknown): Promise<import('../../../packages/contracts/src/index.ts').ObservationView> { throw new Problem('native_strategy_only', 410); }
  function scoped<A extends unknown[], R>(operation: (...args: A) => R): (...args: A) => R {
    return (...args) => host.withNativeHost(() => operation(...args));
  }
  return { snapshot: scoped(snapshot), getSettings: scoped(getSettings), startResearch: scoped(startResearch), removeResearch: scoped(removeResearch),
    removeFactor: scoped(removeFactor), removePortfolio: scoped(removePortfolio), getPortfolio: scoped(getPortfolio),
    startObservation: scoped(startObservation), getObservation: scoped(getObservation), refreshObservation: scoped(refreshObservation), configureAutomatic: scoped(configureAutomatic) };
}
