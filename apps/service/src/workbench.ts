import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { ConversationId, TaskId } from '@earendil-works/pi-durable';
import { randomUUID } from 'node:crypto';
import { cp, mkdir, rm } from 'node:fs/promises';
import { join } from 'node:path';
import type { ResearchHost } from '../../research/src/host.ts';
import type { ResearchInput as NativeResearchInput } from '../../research/src/documents.ts';
import { canonical, Requests, Study } from '../../research/src/documents.ts';
import { researchInput } from '../../research/src/input.ts';
import { isRecoveryRequestId } from '../../research/src/recovery.ts';
import { exactWeights, replayCatalog, strategyMembers } from '../../research/src/strategy.ts';
import type { ServiceConfig } from './config.ts';
import { Application, counter, decimal, decimalUnits, json, nowNs, nsTime, object, observation, portfolio, Problem, settings, text } from './facts.ts';
import type { ObservationFact, PortfolioFact } from './facts.ts';
import { factorViews } from './factors.ts';
import { researchCriterion } from '../../../packages/contracts/src/research.ts';
import { metrics } from './projections.ts';
import { BuildPortfolio, ObserveAccount } from './science.ts';
import { AdjustAccount, unsettledAdjustment } from './automatic.ts';
import { accountKey, capitalStatus, control, readCapitalControl, recordCapitalControl, validateCapitalDocument } from './capital-exit.ts';
import type { ObservationStart, PortfolioStart, ResearchInput, ResearchStart, SettingsView, AutomaticPolicy } from '../../../packages/contracts/src/index.ts';

const context = BACKGROUND_CONTEXT;
export function workbench(host: ResearchHost, config: ServiceConfig) {
  const snapshot = () => host.harness.snapshot(Application, context);
  const getSettings = async (): Promise<SettingsView> => settings((await snapshot())!.settings);
  const dataset = (id: string) => {
    const found = config.datasets.find((dataset) => dataset.id === id);
    if (!found) throw new Problem('dataset_not_found', 404);
    if (!found.ready) throw new Problem('dataset_not_ready', 409);
    return found;
  };
  async function recoverResearch() {
    const [facts, native] = await Promise.all([snapshot(), host.harness.snapshot(Requests, context)]);
    for (const [key, request] of Object.entries(facts?.researchRequests ?? {})) {
      // The host repairs its own native admission gap; never resubmit its stopped/final studies.
      if (!native?.requests[key]) await host.start(key.slice('request:'.length), request.input as NativeResearchInput, true);
    }
  }
  async function startResearch(value: unknown) {
    const body = object(value); const requestId = text(body.requestId, 'request_id');
    const original = json(body); const old = (await snapshot())!.researchRequests[`request:${requestId}`];
    const nativeAdmission = (await host.harness.snapshot(Requests, context))?.requests[`request:${requestId}`];
    if (nativeAdmission?.recovery || !nativeAdmission && !old && isRecoveryRequestId(requestId)) throw new Problem('request_id_conflict', 409);
    if (old) {
      if (canonical(old.original) !== canonical(original)) throw new Problem('request_id_conflict', 409);
      return host.start(requestId, old.input as NativeResearchInput, true);
    }
    const submission = body as unknown as ResearchStart;
    let input: ResearchInput;
    let capturedDataset: import('../../../packages/contracts/src/index.ts').RecordedDataset | undefined;
    const captureDataset = (configured: ServiceConfig['datasets'][number]) => {
      const { id, name, assetClass, currency, source, start, end } = configured;
      return { id, name, assetClass, currency, source, start, end };
    };
    if ('input' in submission) {
      const native = (await host.harness.snapshot(Requests, context))?.requests[`request:${requestId}`];
      if (native) {
        if (canonical(native.input) !== canonical(json(submission.input))) throw new Problem('request_id_conflict', 409);
        await host.harness.commit(async (tx) => {
          const facts = await tx.doc(Application); const existing = facts.researchRequests[`request:${requestId}`];
          if (existing && canonical(existing.original) !== canonical(original)) throw new Problem('request_id_conflict', 409);
          facts.researchRequests[`request:${requestId}`] ??= { original, input: json(native.input) };
        }, context);
        return host.start(requestId, native.input as NativeResearchInput);
      }
      input = structuredClone(submission.input);
      object(input, 'research_input'); text(input.goal, 'goal');
      const data = object(input.data, 'dataset');
      const registered = config.datasets.find((dataset) => dataset.ready && canonical(json(dataset.data)) === canonical(data));
      if (!registered) throw new Problem('dataset_input_unregistered');
      if (input.specification) {
        const inherited = { ...registered.policy, max_trials: object(input.policy, 'policy').max_trials, ...(registered.strategy ? { strategy: json(registered.strategy) } : {}) };
        if (canonical(json(input.policy)) !== canonical(json(inherited))) throw new Problem('research_policy_unregistered');
        capturedDataset = captureDataset(registered);
      }
    } else {
      const configured = dataset(text(submission.datasetId, 'dataset_id'));
      capturedDataset = captureDataset(configured);
      if (!Number.isSafeInteger(submission.maxTrials) || submission.maxTrials < 0) throw new Problem('max_trials_invalid');
      if (submission.serviceTier !== undefined && !['default', 'priority'].includes(submission.serviceTier)) throw new Problem('invalid_service_tier');
      const preferences = await getSettings();
      const preferredModel = preferences.defaultModel?.provider === submission.model?.provider && preferences.defaultModel?.modelId === submission.model?.modelId;
      const thinkingLevel = submission.thinkingLevel ?? (preferredModel ? preferences.thinkingLevel : null);
      const serviceTier = submission.serviceTier ?? (preferredModel ? preferences.serviceTier : null);
      input = { goal: text(submission.goal, 'goal'), ...(submission.specification ? { specification: structuredClone(submission.specification) } : {}), model: submission.model, ...(thinkingLevel !== null && thinkingLevel !== undefined ? { thinkingLevel } : {}), ...(serviceTier !== null && serviceTier !== undefined ? { serviceTier } : {}), data: structuredClone(configured.data),
        policy: { ...configured.policy, max_trials: submission.maxTrials, ...(configured.strategy ? { strategy: json(configured.strategy) } : {}) } };
    }
    const model = object(input.model, 'model'); text(model.provider, 'provider'); text(model.modelId, 'model_id');
    const available = await host.availableModels(String(model.provider));
    const selectedModel = available.find(item => item.modelId === model.modelId);
    if (!selectedModel) throw new Problem('model_unavailable', 409);
    if (input.thinkingLevel !== undefined && !selectedModel.thinkingLevels.includes(input.thinkingLevel)) throw new Problem('thinking_level_unsupported');
    if (input.serviceTier !== undefined && !['default', 'priority'].includes(input.serviceTier)) throw new Problem('invalid_service_tier');
    if (input.serviceTier !== undefined && !selectedModel.serviceTiers.includes(input.serviceTier)) throw new Problem('service_tier_unsupported');
    const policy = object(input.policy, 'policy');
    if (policy.max_trials !== undefined && (!Number.isSafeInteger(policy.max_trials) || Number(policy.max_trials) < 0)) throw new Problem('max_trials_invalid');
    const validated = researchInput(input);
    await host.harness.commit(async (tx) => {
      const facts = await tx.doc(Application); const current = facts.researchRequests[`request:${requestId}`];
      if (current && canonical(current.original) !== canonical(original)) throw new Problem('request_id_conflict', 409);
      facts.researchRequests[`request:${requestId}`] ??= { original, input: json(validated), ...(capturedDataset ? { dataset: json(capturedDataset) } : {}) };
    }, context);
    return host.start(requestId, validated);
  }
  async function getPortfolio(id: string) {
    const found = (await snapshot())!.portfolios[id]; if (!found) throw new Problem('portfolio_not_found', 404);
    return portfolio(found);
  }
  async function startPortfolio(value: unknown) {
    const input = object(value) as unknown as PortfolioStart;
    text(input.requestId, 'request_id'); text(input.name, 'name');
    const old = Object.values((await snapshot())!.portfolios).map(portfolio).find((item) => item.input.requestId === input.requestId);
    if (old) { if (canonical(json(old.input)) !== canonical(json(input))) throw new Problem('request_id_conflict', 409); return old.view; }
    const configured = dataset(text(input.datasetId, 'dataset_id'));
    const capital = decimal(input.capital, 'capital'); if (decimalUnits(capital) <= 0n) throw new Problem('capital_invalid');
    let researchIds: number[]; let weights: string[];
    if (input.factors !== undefined) {
      if (input.researchIds !== undefined || input.weights !== undefined) throw new Problem('factor_or_legacy_members_required', 422);
      if (!Array.isArray(input.factors) || !input.factors.length || new Set(input.factors.map(item => item.factorId)).size !== input.factors.length) throw new Problem('distinct_factor_ids_required', 422);
      const { factorReference } = await import('./factors.ts');
      researchIds = input.factors.map(item => factorReference(text(item.factorId, 'factor_id')).researchId);
      weights = input.factors.map(item => decimal(item.weight, 'weight'));
    } else {
      if (!Array.isArray(input.researchIds) || !input.researchIds.length || input.researchIds.some((id) => !Number.isSafeInteger(id) || id < 0)
        || new Set(input.researchIds).size !== input.researchIds.length) throw new Problem('distinct_research_ids_required', 422);
      researchIds = input.researchIds;
      if (input.weights === undefined && researchIds.length === 1) weights = ['1'];
      else {
        if (!Array.isArray(input.weights) || input.weights.length !== researchIds.length) throw new Problem('portfolio_weights_required', 422);
        const byId = new Map(input.weights.map((entry) => { const pair = object(entry, 'portfolio_weight'); return [pair.researchId, decimal(pair.weight, 'weight')] as const; }));
        if (byId.size !== researchIds.length || researchIds.some((id) => !byId.has(id))) throw new Problem('portfolio_weight_ids_mismatch', 422);
        weights = researchIds.map((id) => byId.get(id)!);
      }
    }
    if (input.capacityPolicy) {
      const { researchCapital } = await import('../../../packages/contracts/src/research.ts');
      researchCapital(input.capacityPolicy);
    }
    // Exact admission validation only; all numerical blending belongs to job blend-targets.
    try { exactWeights(weights); } catch { throw new Problem('portfolio_weights_invalid', 422); }
    if (!configured.strategy || !['factor-threshold', 'cross-market-threshold'].includes(configured.strategy.kind)) throw new Problem('strategy_adapter_not_configured', 422);
    const evidence = await host.harness.commit(async (tx) => {
      const records = [];
      let selection: string | undefined;
      for (const [index, id] of researchIds.entries()) {
        const conversationId = id as ConversationId;
        if (!await tx.conversation(conversationId)) throw new Problem('research_not_found', 404);
        const study = await tx.doc(Study, conversationId);
        if (!study.input || canonical(study.input.data) !== canonical(json(configured.data))) throw new Problem('research_dataset_mismatch', 422);
        if (canonical(study.input.policy.strategy ?? null) !== canonical(json(configured.strategy))) throw new Problem('strategy_policy_not_admitted_with_research', 422);
        const final = study.final; const selected = study.trials.find((trial) => trial.taskId === final?.trialId);
        if (final?.status !== 'COMPUTED' || selected?.status !== 'COMPUTED' || !final.result) throw new Problem('independent_evidence_missing', 422);
        const result = object(final.result, 'independent_result');
        const heldout = Array.isArray(result.partitions) ? result.partitions.map((partition) => object(partition)).find((partition) => partition.partition === 'HELDOUT') : null;
        const counts = heldout ? object(object(heldout.metric).counts) : null;
        if (!counts || BigInt(counter(counts.included, 'heldout_included')) === 0n) throw new Problem('heldout_observations_missing', 422);
        const criterion = researchCriterion(study.input.specification, 'COMPUTED', metrics(final.result));
        if (criterion && !['computed', 'met'].includes(criterion.status)) throw new Problem(criterion.status === 'insufficient' ? 'research_criterion_insufficient' : 'research_criterion_not_met', 422);
        strategyMembers(object(selected.proposal));
        if (input.factors) {
          const factor = factorViews(id, study).find(item => item.id === input.factors![index]!.factorId);
          if (!factor?.combinable) throw new Problem(factor?.reason ?? 'factor_not_found', 422);
        }
        if (study.input.data.sealed) replayCatalog(study.input.data, object(result.request, 'independent_request'));
        const current = canonical(object(object(result.request, 'independent_request').forecast, 'forecast_request').selection!);
        if (selection !== undefined && selection !== current) throw new Problem('strategy_selection_mismatch', 422);
        selection = current;
        records.push(json({ researchId: id, ...(input.factors ? { factorId: input.factors[index]!.factorId } : {}), input: study.input, selected, final }));
      }
      return records;
    }, context);
    const id = randomUUID(); const capture = join(config.dataDir, 'catalogs', id);
    await mkdir(join(config.dataDir, 'catalogs'), { recursive: true });
    const originalData = object(object(evidence[0]!.input).data);
    const originalRequest = object(object(object(evidence[0]!.final).result).request);
    const catalog = originalData.sealed ? replayCatalog(originalData, originalRequest) : text(originalData.catalog, 'catalog');
    await cp(catalog, capture, { recursive: true });
    try {
      const admitted = await host.harness.commit(async (tx) => {
        const facts = await tx.doc(Application);
        const existing = Object.values(facts.portfolios).map(portfolio).find((item) => item.input.requestId === input.requestId);
        if (existing) { if (canonical(json(existing.input)) !== canonical(json(input))) throw new Problem('request_id_conflict', 409); return existing.view; }
        const fact: PortfolioFact = { input: structuredClone(input), evidence, weights, policy: json(configured.strategy), native: null, target: null,
          view: { id, name: input.name, researchIds: [...new Set(researchIds)], ...(input.factors ? { factors: input.factors } : {}), datasetId: input.datasetId, status: 'QUEUED', capital, currency: configured.currency,
            createdAt: new Date().toISOString(), taskId: null, metrics: [], curve: [], targets: [], error: null } };
        const taskId = await tx.createTask(BuildPortfolio, { id, evidence, weights, policy: configured.strategy!, capital, currency: configured.currency,
          dataDir: config.dataDir, catalog: capture, binary: config.jobBinary, ...(input.capacityPolicy ? { capacityPolicy: input.capacityPolicy } : {}) },
          { conversationId: researchIds[0] as ConversationId, ownership: { kind: 'conversation' } });
        fact.view.taskId = taskId; facts.portfolios[id] = json(fact); return fact.view;
      }, context);
      if (admitted.id !== id) await rm(capture, { recursive: true, force: true });
      return admitted;
    } catch (error) { await rm(capture, { recursive: true, force: true }); throw error; }
  }
  async function getObservation(id: string) {
    const found = (await snapshot())!.observations[id]; if (!found) throw new Problem('observation_not_found', 404);
    return observation(found);
  }
  async function startObservation(value: unknown) {
    const input = object(value) as unknown as ObservationStart; text(input.requestId, 'request_id');
    const old = Object.values((await snapshot())!.observations).map(observation).find((item) => item.input.requestId === input.requestId);
    if (old) { if (canonical(json(old.input)) !== canonical(json(input))) throw new Problem('request_id_conflict', 409); return old.view; }
    const selected = await getPortfolio(text(input.portfolioId, 'portfolio_id'));
    if (selected.view.status !== 'COMPUTED' || !selected.target) throw new Problem('portfolio_target_missing', 422);
    if (!input.automatic?.enabled && BigInt(counter(selected.target.valid_until_ns, 'expiry')) <= BigInt(nowNs())) throw new Problem('target_expired', 422);
    if (input.automatic) validateAutomatic(input.automatic);
    const connection = config.connections.find((connection) => connection.id === input.connectionId && connection.mode === input.mode);
    if (!connection) throw new Problem('connection_not_configured', 422);
    if (!input.automatic?.enabled) {
      const fresh = await readCapitalControl(connection, selected.view.currency);
      for (const exit of fresh.status.exits) await validateCapitalDocument(config, 'VIEW', exit);
      await host.harness.commit(async tx => { const facts = await tx.doc(Application); facts.accountControls ??= {}; recordCapitalControl(facts.accountControls, connection, fresh); }, context);
    }
    const id = randomUUID(); const receiptId = randomUUID();
    return host.harness.commit(async (tx) => {
      const facts = await tx.doc(Application);
      const existing = Object.values(facts.observations).map(observation).find((item) => item.input.requestId === input.requestId);
      if (existing) { if (canonical(json(existing.input)) !== canonical(json(input))) throw new Problem('request_id_conflict', 409); return existing.view; }
      const accountControl = facts.accountControls?.[accountKey(connection, facts.accountControls)];
      if (!input.automatic?.enabled) {
        const state = capitalStatus(control(accountControl));
        if (state.reserved === null) throw new Problem('account_reservation_unknown', 409);
        if (decimalUnits(state.reserved) > 0n || state.exits.some(exit => exit.remaining_trading !== 'BUDGET_AWARE')) throw new Problem('account_capital_exit_requires_budget_aware_target', 409);
      }
      const unsettled = Object.values(facts.observations).map(observation).some((fact) => accountKey(fact.connection, facts.accountControls) === accountKey(connection, facts.accountControls)
        && (['PENDING', 'UNCERTAIN'].includes(fact.view.receipt?.status ?? '') || unsettledAdjustment(fact)));
      if (unsettled) throw new Problem('target_delivery_requires_reconciliation', 409);
      if (input.automatic?.enabled && Object.values(facts.observations).map(observation).some(fact => accountKey(fact.connection, facts.accountControls) === accountKey(connection, facts.accountControls) && fact.view.automaticPolicy?.enabled && fact.view.status !== 'STOPPED')) throw new Problem('account_automatic_binding_already_enabled', 409);
      const taskOptions = { conversationId: selected.view.researchIds[0] as ConversationId, ownership: { kind: 'conversation' as const } };
      const taskId = input.automatic?.enabled
        ? await tx.createTask(AdjustAccount, { id, binary: config.jobBinary, dataDir: config.dataDir }, { ...taskOptions, background: true })
        : await tx.createTask(ObserveAccount, { id, connection: structuredClone(connection), target: selected.target!, receiptId, refresh: false }, taskOptions);
      const fact: ObservationFact = { input: structuredClone(input), connection: structuredClone(connection), target: selected.target!, taskId, snapshot: null,
        view: { id, name: selected.view.name, portfolioId: selected.view.id, mode: input.mode, status: 'QUEUED', currency: selected.view.currency,
          equity: null, cash: null, updatedAt: null, positions: [], curve: [], error: null,
          ...(input.automatic ? { automaticPolicy: input.automatic } : {}),
          receipt: { id: receiptId, status: input.automatic?.enabled ? 'NOT_OFFERED' : 'PENDING', externalId: null, expiresAt: nsTime(String(selected.target!.valid_until_ns)), acknowledgedAt: null } } };
      facts.observations[id] = json(fact); return fact.view;
    }, context);
  }
  async function refreshObservation(id: string) {
    const original = await getObservation(id);
    if (original.view.automaticPolicy?.enabled && original.view.status !== 'STOPPED' && !['PENDING', 'UNCERTAIN'].includes(original.view.receipt?.status ?? '') && !unsettledAdjustment(original)) return original.view;
    const taskId = await host.harness.commit(async (tx) => {
      const facts = await tx.doc(Application); const value = facts.observations[id]; if (!value) throw new Problem('observation_not_found', 404);
      const fact = observation(value);
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
    await host.harness.waitForTask(taskId as TaskId, context); return (await getObservation(id)).view;
  }
  function validateAutomatic(value: AutomaticPolicy) {
    if (typeof value.enabled !== 'boolean' || decimalUnits(decimal(value.cashBuffer, 'cash_buffer')) < 0n || !Number.isSafeInteger(value.maximumSnapshotAgeSeconds) || value.maximumSnapshotAgeSeconds < 1) throw new Problem('automatic_policy_invalid', 422);
  }
  async function configureAutomatic(id: string, value: unknown) {
    const input = object(value) as unknown as AutomaticPolicy; validateAutomatic(input);
    let abortId: number | undefined;
    await host.harness.commit(async tx => {
      const facts = await tx.doc(Application); const found = facts.observations[id]; if (!found) throw new Problem('observation_not_found', 404);
      const fact = observation(found);
      if (input.enabled && fact.view.status === 'STOPPED') throw new Problem('observation_stopped', 409);
      if (input.enabled && Object.values(facts.observations).map(observation).some(other => other.view.id !== id && accountKey(other.connection, facts.accountControls) === accountKey(fact.connection, facts.accountControls) && other.view.automaticPolicy?.enabled && other.view.status !== 'STOPPED')) throw new Problem('account_automatic_binding_already_enabled', 409);
      if (input.enabled && Object.values(facts.observations).map(observation).some(other => accountKey(other.connection, facts.accountControls) === accountKey(fact.connection, facts.accountControls)
        && (['PENDING', 'UNCERTAIN'].includes(other.view.receipt?.status ?? '') || other.view.id !== id && unsettledAdjustment(other)))) throw new Problem('target_delivery_requires_reconciliation', 409);
      const prior = await tx.task(fact.taskId as TaskId);
      if (input.enabled && !fact.view.automaticPolicy?.enabled && prior && prior.state.status !== 'terminal') throw new Problem('original_observation_requires_reconciliation', 409);
      fact.view.automaticPolicy = structuredClone(input);
      if (!input.enabled && prior && prior.state.status !== 'terminal') abortId = fact.taskId;
      if (input.enabled && (!prior || prior.state.status === 'terminal')) fact.taskId = await tx.createTask(AdjustAccount, { id, binary: config.jobBinary, dataDir: config.dataDir },
        { conversationId: portfolio(facts.portfolios[fact.input.portfolioId]).view.researchIds[0] as ConversationId, ownership: { kind: 'conversation' }, background: true });
      facts.observations[id] = json(fact);
    }, context);
    if (abortId !== undefined) { await host.harness.abortTask(abortId as TaskId, context); await host.harness.waitForTask(abortId as TaskId, context); }
    return (await getObservation(id)).view;
  }
  return { snapshot, getSettings, recoverResearch, startResearch, startPortfolio, getPortfolio, startObservation, getObservation, refreshObservation, configureAutomatic };
}
