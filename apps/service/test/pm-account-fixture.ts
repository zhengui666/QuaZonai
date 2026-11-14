import {randomUUID} from 'node:crypto';
import type {FastifyInstance} from 'fastify';
import type {ConnectionConfig} from '../src/config.ts';
import {registerProcessAccount} from '../src/downstream.ts';
import type { JsonObject } from '@earendil-works/pi-durable';
import { captureAccount } from '../src/pm-account.ts';
import { accountKey } from '../src/capital-exit.ts';
import { json } from '../src/facts.ts';
import type { ObservationFact } from '../src/facts.ts';

// Original source-contract fixtures only. They test projection/receipt integrity,
// not a broker account, native simulation or financial performance.
export function accountSourceFixture() {
  const connection = { id: 'binding:original', name: 'Original source protocol', mode: 'paper' as const,
    url: 'http://127.0.0.1:1', clientId: 'transport-client', accountId: 'transport-account' };
  const money = (amount: string, currency: string) => ({ amount, currency });
  const balance = (amount: string, currency: string) => ({ total: money(amount, currency), free: money(amount, currency), locked: money('0', currency) });
  const native: JsonObject = { account_id: 'VENUE-001', account_type: 'CASH', base_currency: null,
    balances: [balance('900.000000000000000001', 'USDC'), balance('0.125000000000000001', 'BTC')], margins: [],
    unrealized_pnls: [], realized_pnls: [], total_equity: [money('900.000000000000000001', 'USDC'), money('0.125000000000000001', 'BTC')],
    base_currency_equity: null, is_stale: false, stale_instruments: [], stale_currencies: [], unpriced_instruments: [],
    event_id: '01900000-0000-7000-8000-000000000001', ts_event: '99', ts_init: '100' };
  const snapshot: JsonObject = { clientId: connection.clientId, accountId: connection.accountId, currency: 'USDC',
    accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: '7', sequence: '9007199254740995', nativeClockNs: '99',
    equity: null, cash: null, positions: [],
    nativeObservation: { schema_version: 1, binding: { schema_version: 1, project_id: '01900000-0000-7000-8000-000000000002', environment: 'PAPER',
      native_trader_id: 'TRADER-001', native_session_id: 'SESSION-001', native_account_id: 'VENUE-001', native_version: '0.69.0' },
      sequence: '9007199254740995', dropped_events: '0', observed_at_ns: '101', connection: 'CONNECTED', snapshot: native },
    openOrders: { schema_version: 1, account_id: 'VENUE-001', asof_ns: '99', available_ns: '101', count: 0 } };
  const observation: ObservationFact = { input: { requestId: 'source-original', portfolioId: 'portfolio-original', mode: 'paper', connectionId: connection.id },
    connection, taskId: 1, target: {}, snapshot, snapshotReceivedAtNs: '102',
    view: { id: connection.id, name: connection.name, portfolioId: 'portfolio-original', mode: 'paper', status: 'OBSERVING', currency: 'USDC',
      equity: null, cash: null, positions: [], curve: [], updatedAt: null, error: null, receipt: null } };
  const application = { observations: { [connection.id]: json(observation) }, accountControls: { [accountKey(connection)]: {
    commands: {}, receivedAtNs: '101', status: { capability: 'SUPPORTED', reason: null, accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: '7', reserved: '0', exits: [] },
  } } };
  return { connection, application, native, snapshot, observation,
    capture: () => captureAccount(application, connection.id, '110') };
}

/** Initialize an explicitly retained control record in the real Durable store.
 * This is test setup, not a replacement admission route or fitted-model result.
 * Existing tests still exercise the real account guard, HTTP commands, Tasks
 * and SQLite recovery; no performance, order fill or qualification is invented.
 */
export async function retainPmControlRecord(state: Awaited<ReturnType<typeof import('./support.ts').setup>>, input: import('../../../packages/contracts/src/pm.ts').PmMandateStart) {
  const { PmDocument, fact, pmState } = await import('../src/pm-facts.ts');
  const { Application, nowNs, observation } = await import('../src/facts.ts');
  const { captureDataset } = await import('../src/pm-datasets.ts');
  const { readSourceRevision } = await import('../src/pm-source.ts');
  const { existingDataset } = await import('../src/config.ts');
  const { assertAccountTargetControl } = await import('../src/account-target-control.ts');
  const { freezeCampaignAccount } = await import('../src/pm-campaign-account.ts');
  const { createPmPlatform } = await import('../src/pm-platform.ts');
  const { BACKGROUND_CONTEXT: context } = await import('@earendil-works/chord/context');
  const configured = existingDataset(state.service.config, input.datasetId)!;
  const source = await readSourceRevision(configured);
  const id = `test:retained:${input.requestId}`;
  await state.service.research.harness.commit(async tx => {
    const stored = pmState(await tx.doc(PmDocument)); const app = await tx.doc(Application);
    const capture = input.funding.kind === 'ACCOUNT' ? captureAccount(app, input.funding.accountBindingId) : null;
    if (input.funding.kind === 'ACCOUNT' && input.operatingAuthority === 'AUTHORIZED_TARGETS') {
      assertAccountTargetControl(app, stored, observation(app.observations[input.funding.accountBindingId]).connection,
        { kind: 'PM', mandateId: id, accountBindingId: input.funding.accountBindingId });
    }
    const dataset = captureDataset(configured, stored, nowNs(), source);
    const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
    const strategyId = `${id}:strategy`, campaignId = `${id}:campaign`, policyId = `${id}:policy`;
    const currency = capture?.basis.currency ?? (input.funding.kind === 'RESEARCH_SCENARIO' ? input.funding.capital.currency : configured.currency);
    const record = {
      input, conversationId: conversation.id,
      mandate: { ...fact(id), ownerGoal: input.goal, interpretation: input.goal, baseCurrency: currency, capitalSource: input.funding,
        ownerBoundaries: [], operatingAuthority: input.operatingAuthority ?? 'RESEARCH_ONLY', unresolvedIntent: [], allowedUniverseRef: `${id}:universe` },
      policy: { ...fact(policyId), mandateRevisionId: id, methodFamily: 'TEST_ONLY_RETAINED_ACCOUNT_CONTROL', validDomain: {}, selectionRule: { mode: 'FRONTIER_ONLY', objectives: [] } },
      definition: { ...fact(`${id}:definition`), name: input.goal },
      budget: { ...fact(`${id}:budget`), maximumTrials: null, reservationForClosureRef: `${id}:reservation` },
      datasets: [dataset.revision.id],
      campaigns: { [campaignId]: { ...fact(campaignId), state: 'PAUSED', datasetRevisionIds: [dataset.revision.id], runtime: { conversationId: conversation.id, taskId: null } } },
      strategies: { [strategyId]: { ...fact(strategyId), definitionId: `${id}:definition`, mandateRevisionId: id, policyRevisionId: policyId,
        fittedEvidenceRefs: [], modelRefs: [], pipelineAssessmentId: `${id}:assessment` } },
      assessments: {}, events: {}, cycles: {}, decisions: {}, comparisons: {}, capacities: {}, ensembles: {}, evidence: {}, artifacts: {},
      outcomes: {}, attributions: {}, retirements: {}, commands: {}, gaps: {}, derivedTrials: {}, budgetMovements: {},
      searchFamily: { ...fact(`${id}:search`), campaignIds: [campaignId], trialIds: [], countedAttempts: '0', failedAttempts: '0' },
      latestStrategyId: strategyId, currentDecisionId: null, latestCycleId: null, basisRevision: 'existing-basis', monitorTaskId: null,
    } as unknown as import('../src/pm-facts.ts').PmRecord;
    if (capture) freezeCampaignAccount(record, record.campaigns[campaignId]!, capture);
    stored.mandates[id] = record;
    const monitorTaskId = await tx.createTask(createPmPlatform(state.service.config).extension.tasks!.find(task => task.definition.name === 'qz.pm.mandate-monitor')! as import('@earendil-works/pi-durable').Task<{ mandateId: string }, { phase: 'start' }, null, object>, { mandateId: id },
      { conversationId: conversation.id, ownership: { kind: 'conversation' }, background: true });
    stored.mandates[id]!.monitorTaskId = monitorTaskId;
    stored.mandates[id]!.campaigns[campaignId]!.runtime.taskId = monitorTaskId;
  }, context);
  return state.service.pm.get(id);
}

/** Controlled direct-handler boundary for service permissions/receipt tests.
 * No HTTP listener, Nautilus engine or real child process is claimed here.
 * Actual process lifecycle is covered by process-account-routes/explicit-start. */
export async function directAccountProtocol(connection:ConnectionConfig,peer:FastifyInstance){
 await peer.ready();
 Object.assign(connection,{url:`rust-process://controlled/${randomUUID()}`,kind:'RUST_PROCESS',processBinding:{directory:'/controlled-no-account-files',runner:{kind:'RUST_PROCESS',id:'controlled',binary:'/controlled-not-executed',capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{testOnly:true},research:{singleStrategyRegistration:false,configuredRunFactory:false},account:{compiled:true,registration:true,command:'account-runtime',requiresExplicitStart:true},running:false}}}});
 return registerProcessAccount(connection,async(path,method,clientId,input)=>{const result=await peer.inject({method,url:path,headers:{'x-qz-client-id':String(clientId)},...(input===undefined?{}:{payload:input as object})});return {status:result.statusCode,value:result.json()};});
}
