import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { ConversationId, TaskId } from '@earendil-works/pi-durable';
import { randomUUID } from 'node:crypto';
import { cp, mkdir, rm } from 'node:fs/promises';
import { join } from 'node:path';
import type { ResearchHost } from '../../research/src/host.ts';
import type { ResearchInput as NativeResearchInput } from '../../research/src/documents.ts';
import { canonical, Requests, Study } from '../../research/src/documents.ts';
import { researchInput } from '../../research/src/input.ts';
import { exactWeights, replayCatalog, strategyMembers } from '../../research/src/strategy.ts';
import type { ServiceConfig } from './config.ts';
import { Application, counter, decimal, decimalUnits, json, nowNs, nsTime, object, observation, portfolio, Problem, settings, text } from './facts.ts';
import type { ObservationFact, PortfolioFact } from './facts.ts';
import { connectionBase } from './downstream.ts';
import { BuildPortfolio, ObserveAccount } from './science.ts';
import type { ObservationStart, PortfolioStart, ResearchInput, ResearchStart, SettingsView } from '../../../packages/contracts/src/index.ts';

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
      if (!native?.requests[key]) await host.start(key.slice('request:'.length), request.input as NativeResearchInput);
    }
  }
  async function startResearch(value: unknown) {
    const body = object(value); const requestId = text(body.requestId, 'request_id');
    const original = json(body); const old = (await snapshot())!.researchRequests[`request:${requestId}`];
    if (old) {
      if (canonical(old.original) !== canonical(original)) throw new Problem('request_id_conflict', 409);
      return host.start(requestId, old.input as NativeResearchInput);
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
      capturedDataset = captureDataset(registered);
    } else {
      const configured = dataset(text(submission.datasetId, 'dataset_id'));
      capturedDataset = captureDataset(configured);
      if (!Number.isSafeInteger(submission.maxTrials) || submission.maxTrials < 0) throw new Problem('max_trials_invalid');
      const preferences = await getSettings();
      const thinkingLevel = submission.thinkingLevel ?? (preferences.defaultModel?.provider === submission.model?.provider && preferences.defaultModel?.modelId === submission.model?.modelId ? preferences.thinkingLevel : null);
      input = { goal: text(submission.goal, 'goal'), model: submission.model, ...(thinkingLevel !== null && thinkingLevel !== undefined ? { thinkingLevel } : {}), data: structuredClone(configured.data),
        policy: { ...configured.policy, max_trials: submission.maxTrials, ...(configured.strategy ? { strategy: json(configured.strategy) } : {}) } };
    }
    const model = object(input.model, 'model'); text(model.provider, 'provider'); text(model.modelId, 'model_id');
    const available = await host.availableModels(String(model.provider));
    const selectedModel = available.find(item => item.modelId === model.modelId);
    if (!selectedModel) throw new Problem('model_unavailable', 409);
    if (input.thinkingLevel !== undefined && !selectedModel.thinkingLevels.includes(input.thinkingLevel)) throw new Problem('thinking_level_unsupported');
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
    if (!Array.isArray(input.researchIds) || !input.researchIds.length || input.researchIds.some((id) => !Number.isSafeInteger(id) || id < 0)
      || new Set(input.researchIds).size !== input.researchIds.length) throw new Problem('distinct_research_ids_required', 422);
    let weights: string[];
    if (input.weights === undefined && input.researchIds.length === 1) weights = ['1'];
    else {
      if (!Array.isArray(input.weights) || input.weights.length !== input.researchIds.length) throw new Problem('portfolio_weights_required', 422);
      const byId = new Map(input.weights.map((entry) => { const pair = object(entry, 'portfolio_weight'); return [pair.researchId, decimal(pair.weight, 'weight')] as const; }));
      if (byId.size !== input.researchIds.length || input.researchIds.some((id) => !byId.has(id))) throw new Problem('portfolio_weight_ids_mismatch', 422);
      weights = input.researchIds.map((id) => byId.get(id)!);
    }
    // Exact admission validation only; all numerical blending belongs to job blend-targets.
    try { exactWeights(weights); } catch { throw new Problem('portfolio_weights_invalid', 422); }
    if (!configured.strategy || configured.strategy.kind !== 'factor-threshold') throw new Problem('strategy_adapter_not_configured', 422);
    const evidence = await host.harness.commit(async (tx) => {
      const records = [];
      let selection: string | undefined;
      for (const id of input.researchIds) {
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
        strategyMembers(object(selected.proposal));
        if (study.input.data.sealed) replayCatalog(study.input.data, object(result.request, 'independent_request'));
        const current = canonical(object(object(result.request, 'independent_request').forecast, 'forecast_request').selection!);
        if (selection !== undefined && selection !== current) throw new Problem('strategy_selection_mismatch', 422);
        selection = current;
        records.push(json({ researchId: id, input: study.input, selected, final }));
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
          view: { id, name: input.name, researchIds: input.researchIds, datasetId: input.datasetId, status: 'QUEUED', capital, currency: configured.currency,
            createdAt: new Date().toISOString(), taskId: null, metrics: [], curve: [], targets: [], error: null } };
        const taskId = await tx.createTask(BuildPortfolio, { id, evidence, weights, policy: configured.strategy!, capital, currency: configured.currency,
          dataDir: config.dataDir, catalog: capture, binary: config.jobBinary },
          { conversationId: input.researchIds[0] as ConversationId, ownership: { kind: 'conversation' } });
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
    if (BigInt(counter(selected.target.valid_until_ns, 'expiry')) <= BigInt(nowNs())) throw new Problem('target_expired', 422);
    const connection = config.connections.find((connection) => connection.id === input.connectionId && connection.mode === input.mode);
    if (!connection) throw new Problem('connection_not_configured', 422);
    const id = randomUUID(); const receiptId = randomUUID();
    return host.harness.commit(async (tx) => {
      const facts = await tx.doc(Application);
      const existing = Object.values(facts.observations).map(observation).find((item) => item.input.requestId === input.requestId);
      if (existing) { if (canonical(json(existing.input)) !== canonical(json(input))) throw new Problem('request_id_conflict', 409); return existing.view; }
      const unsettled = Object.values(facts.observations).map(observation).some((fact) => fact.connection.clientId === connection.clientId
        && fact.connection.accountId === connection.accountId && connectionBase(fact.connection).href === connectionBase(connection).href
        && ['PENDING', 'UNCERTAIN'].includes(fact.view.receipt?.status ?? ''));
      if (unsettled) throw new Problem('target_delivery_requires_reconciliation', 409);
      const taskId = await tx.createTask(ObserveAccount, { id, connection: structuredClone(connection), target: selected.target!, receiptId, refresh: false },
        { conversationId: selected.view.researchIds[0] as ConversationId, ownership: { kind: 'conversation' } });
      const fact: ObservationFact = { input: structuredClone(input), connection: structuredClone(connection), target: selected.target!, taskId, snapshot: null,
        view: { id, name: selected.view.name, portfolioId: selected.view.id, mode: input.mode, status: 'QUEUED', currency: selected.view.currency,
          equity: null, cash: null, updatedAt: null, positions: [], curve: [], error: null,
          receipt: { id: receiptId, status: 'PENDING', externalId: null, expiresAt: nsTime(String(selected.target!.valid_until_ns)), acknowledgedAt: null } } };
      facts.observations[id] = json(fact); return fact.view;
    }, context);
  }
  async function refreshObservation(id: string) {
    const taskId = await host.harness.commit(async (tx) => {
      const facts = await tx.doc(Application); const value = facts.observations[id]; if (!value) throw new Problem('observation_not_found', 404);
      const fact = observation(value);
      const previous = await tx.task(fact.taskId as TaskId);
      if (previous && previous.state.status !== 'terminal') return fact.taskId;
      fact.taskId = await tx.createTask(ObserveAccount, { id, connection: fact.connection, target: fact.target, receiptId: fact.view.receipt!.id, refresh: true },
        { conversationId: portfolio(facts.portfolios[fact.input.portfolioId]).view.researchIds[0] as ConversationId, ownership: { kind: 'conversation' } });
      facts.observations[id] = json(fact); return fact.taskId;
    }, context);
    await host.harness.waitForTask(taskId as TaskId, context); return (await getObservation(id)).view;
  }
  return { snapshot, getSettings, recoverResearch, startResearch, startPortfolio, getPortfolio, startObservation, getObservation, refreshObservation };
}
