import assert from 'node:assert/strict';
import { test } from 'node:test';
import type {
  CapacityEstimate, CapitalPlan, ComparisonScope, DecisionPolicyRevision, EnsembleRevision,
  MetricDefinition, MetricObservation, OperatingComparison, PortfolioDecision, ResearchTrial,
} from '../../../packages/contracts/src/pm.ts';
import {
  validateCapitalPlan, validateCapacityEstimate, validateMetricObservation,
  validateOperatingComparison, validatePmEntity,
} from '../../../packages/contracts/src/pm-validation.ts';
import { assessCapacityCurve, compareOperatingCandidates, compareResearchCandidates, projectNativeIndependentMetrics, projectNativeValidationMetrics } from '../../../packages/contracts/src/decision.ts';

// Independent business counterexamples. These small, deliberately synthetic
// observations verify decision semantics, never investment performance.
const fact = (id: string) => ({ schemaVersion: 1 as const, id, revision: '0', supersedesId: null,
  createdAtNs: '200', producerVersion: 'independent-business-acceptance' });
const money = (amount: string) => ({ amount, currency: 'USD' });
const interval = { startNs: '100', endExclusiveNs: '200' };
const metricDefinition = (id: string, key: string, direction: MetricDefinition['direction'],
  annualizationDays: number | null = null): MetricDefinition => ({
  id, key, definitionVersion: '1', label: id, unit: key === 'account.maximum_drawdown' ? 'LOSS_RATIO' : 'RETURN_RATIO',
  valueEncoding: 'FLOAT64', direction, aggregation: 'complete-calendar-days', calendarId: 'calendar:UTC',
  annualizationDays, horizonRef: key === 'account.maximum_drawdown' ? 'horizon:evaluation-window' : 'horizon:day',
  requiredInputs: ['account:net-equity', 'account:cash-flows'],
});
const daily = metricDefinition('metric:daily-net', 'account.daily_geometric_net_return', 'MAXIMIZE');
const drawdown = metricDefinition('metric:drawdown', 'account.maximum_drawdown', 'MINIMIZE');
const sharpe252: MetricDefinition = { ...metricDefinition('metric:sharpe-252', 'account.sharpe', 'MAXIMIZE', 252), unit: 'DIMENSIONLESS' };
const sharpe365: MetricDefinition = { ...metricDefinition('metric:sharpe-365', 'account.sharpe', 'MAXIMIZE', 365), unit: 'DIMENSIONLESS' };
function metric(definition: MetricDefinition, value: number, overrides: Partial<MetricObservation> = {}): MetricObservation {
  return { definition, status: 'AVAILABLE', value, currency: null, reasonCodes: [], role: 'SELECTION', interval,
    asOfNs: '200', availableAtNs: '200', origin: 'DERIVED', methodRef: 'method:native-account',
    evidenceRefs: ['evidence:net-account-path'], sampleCount: '6', expectedCount: '6', uncertainty: null, raw: null, ...overrides };
}
const scope: ComparisonScope = { id: 'scope:shared-account', allowedUniverseRef: 'universe:original',
  datasetRevisionIds: ['dataset:original'], interval, role: 'SELECTION', calendarId: 'calendar:UTC',
  labelDefinitionId: null, accountingPolicyRef: 'accounting:original', costModelRef: 'cost:original',
  startingStateRef: 'account:start', capital: money('3000') };
function trial(id: string, state: ResearchTrial['state'] = 'COMPUTED'): ResearchTrial {
  return { ...fact(id), campaignId: 'campaign:original', hypothesisId: `hypothesis:${id}`, parentTrialIds: [],
    runtime: { conversationId: 1, taskId: id === 'trial:a' ? 11 : 12 }, state, purpose: 'JOINT_REPLAY',
    proposalRef: `proposal:${id}`, datasetRevisionIds: ['dataset:original'], protocolRef: 'protocol:original',
    methodVersion: 'method-v1', countedAttempt: true, resourceUsageRef: `resource:${id}`,
    evidenceRefs: [`evidence:${id}`], errorCode: state === 'FAILED' ? 'native_process_failed' : null };
}
function policy(definitions: MetricDefinition[], mode: 'LEXICOGRAPHIC' | 'FRONTIER_ONLY' = 'LEXICOGRAPHIC'): DecisionPolicyRevision {
  return { ...fact('policy:original'), mandateRevisionId: 'mandate:original', methodFamily: 'registered-comparison',
    nativeImplementationVersion: 'test-observations-only', learnedParametersRef: null, trainingProtocolRef: 'protocol:original',
    validationEvidenceRefs: [], applicableCapabilityIds: ['capability:shared-account'], validDomain: {},
    numericalToleranceRef: 'tolerance:exact-comparison', selectionRule: { mode,
      objectives: definitions.map(definition => ({ metricKey: definition.key, metricDefinitionId: definition.id,
        direction: definition.direction as 'MAXIMIZE' | 'MINIMIZE', horizonRef: definition.horizonRef!, scope: 'ACCOUNT' as const })),
      utilityModelRef: null, establishedBy: 'OWNER_GOAL', rationale: 'Preserve the declared ordering; never infer a new utility from results.' },
    limitations: ['Synthetic observations exercise comparison semantics, not statistical qualification.'] };
}
function compare(definitions: MetricDefinition[], a: MetricObservation[], b: MetricObservation[],
  mode: 'LEXICOGRAPHIC' | 'FRONTIER_ONLY' = 'LEXICOGRAPHIC', states: [ResearchTrial['state'], ResearchTrial['state']] = ['COMPUTED', 'COMPUTED']) {
  return compareResearchCandidates({ policy: policy(definitions, mode), scopes: [scope],
    trials: [trial('trial:a', states[0]), trial('trial:b', states[1])],
    metricsByTrial: { 'trial:a': a, 'trial:b': b }, scopeByTrial: { 'trial:a': scope.id, 'trial:b': scope.id },
    definitions: [daily, drawdown, sharpe252, sharpe365], frontierDefinitionIds: definitions.map(definition => definition.id), asOfNs: '300' });
}

test('PM selection: equal Sharpe does not erase a materially different drawdown path', () => {
  // The same six daily returns can have equal Sharpe while adjacent losing days
  // produce 2.9701% drawdown instead of 1%; six days confer no research validity.
  const result = compare([sharpe252, drawdown],
    [metric(sharpe252, 1.2), metric(drawdown, 0.029701)],
    [metric(sharpe252, 1.2), metric(drawdown, 0.01)]);
  assert.equal(result.recommendationStatus, 'SELECTED');
  assert.equal(result.recommendedTrialId, 'trial:b');
  assert.equal(result.candidates.find(candidate => candidate.trialId === 'trial:a')?.frontierMembership, 'DOMINATED');
});

test('PM selection: unresolved return/downside tradeoffs remain a frontier, without a fabricated winner', () => {
  const result = compare([daily, drawdown], [metric(daily, 0.012), metric(drawdown, 0.25)],
    [metric(daily, 0.006), metric(drawdown, 0.03)], 'FRONTIER_ONLY');
  assert.equal(result.recommendationStatus, 'FRONTIER_ONLY');
  assert.equal(result.recommendedTrialId, null);
  assert.deepEqual(result.candidates.map(candidate => candidate.frontierMembership), ['MEMBER', 'MEMBER']);
});

test('PM selection: a diagnostic frontier cannot silently replace the declared business objective', () => {
  // The owner prioritizes net return. A drawdown-only projection must not discard
  // the higher-return candidate before applying that established preference.
  const result = compareResearchCandidates({ policy: policy([daily]), scopes: [scope],
    trials: [trial('trial:a'), trial('trial:b')],
    metricsByTrial: { 'trial:a': [metric(daily, 0.02), metric(drawdown, 0.10)],
      'trial:b': [metric(daily, 0.01), metric(drawdown, 0.01)] },
    scopeByTrial: { 'trial:a': scope.id, 'trial:b': scope.id }, definitions: [daily, drawdown],
    frontierDefinitionIds: [drawdown.id], asOfNs: '300' });
  assert.notEqual(result.recommendedTrialId, 'trial:b', 'A frontier projection cannot override a frozen return-first policy.');
  if (result.recommendationStatus === 'SELECTED') assert.equal(result.recommendedTrialId, 'trial:a');
});

test('PM selection: all failed or rejected experiments mean missing evidence, not economic falsification', () => {
  const result = compare([daily], [], [], 'LEXICOGRAPHIC', ['FAILED', 'REJECTED']);
  assert.equal(result.recommendationStatus, 'NEEDS_EVIDENCE');
  assert.equal(result.recommendedTrialId, null);
  assert.equal(result.candidates.length, 2, 'Failed attempts must remain visible in the comparison.');
  assert.ok(result.candidates.every(candidate => candidate.frontierMembership === 'UNDETERMINED'));
});

test('PM selection: calibration attempts remain disclosed without being mistaken for competing account strategies', () => {
  const fit = { ...trial('trial:fit'), purpose: 'ENSEMBLE_FIT' as const };
  const result = compareResearchCandidates({ policy: policy([daily]), scopes: [scope],
    trials: [fit, trial('trial:a'), trial('trial:b')], candidateTrialIds: ['trial:a', 'trial:b'],
    metricsByTrial: { 'trial:fit': [], 'trial:a': [metric(daily, 0.02)], 'trial:b': [metric(daily, 0.01)] },
    scopeByTrial: { 'trial:fit': scope.id, 'trial:a': scope.id, 'trial:b': scope.id }, definitions: [daily],
    frontierDefinitionIds: [daily.id], asOfNs: '300' });
  assert.equal(result.recommendationStatus, 'SELECTED');
  assert.equal(result.recommendedTrialId, 'trial:a');
  assert.equal(result.candidates.length, 3, 'Supporting attempts still count and remain in the disclosed ledger.');
  assert.equal(result.candidates.find(candidate => candidate.trialId === fit.id)!.frontierMembership, 'UNDETERMINED');
});

test('PM selection: reusing a scope label cannot make different protocols or revised data comparable', () => {
  for (const changed of [{ ...trial('trial:b'), protocolRef: 'protocol:revised' },
    { ...trial('trial:b'), datasetRevisionIds: ['dataset:corrected-after-selection'] }]) {
    const result = compareResearchCandidates({ policy: policy([daily]), scopes: [scope], trials: [trial('trial:a'), changed],
      metricsByTrial: { 'trial:a': [metric(daily, 0.01)], 'trial:b': [metric(daily, 9)] },
      scopeByTrial: { 'trial:a': scope.id, 'trial:b': scope.id }, definitions: [daily],
      frontierDefinitionIds: [daily.id], asOfNs: '300' });
    assert.notEqual(result.recommendationStatus, 'SELECTED');
    assert.equal(result.recommendedTrialId, null);
    assert.equal(result.candidates.find(candidate => candidate.trialId === 'trial:b')!.status, 'INCOMPARABLE');
  }
});

test('PM selection: a registered 252-day objective cannot select the more flattering 365-day observation', () => {
  const result = compare([sharpe252],
    [metric(sharpe252, 0.8), metric(sharpe365, 0.2)],
    [metric(sharpe365, 5), metric(sharpe252, 0.6)]);
  assert.equal(result.recommendedTrialId, 'trial:a');
  assert.equal(result.objectiveRankings[0]?.metricDefinitionId, sharpe252.id);
  const missing252 = compare([sharpe252], [metric(sharpe365, 50)], [metric(sharpe252, 0.6)]);
  assert.notEqual(missing252.recommendedTrialId, 'trial:a');
  assert.equal(missing252.candidates.find(candidate => candidate.trialId === 'trial:a')?.frontierMembership, 'UNDETERMINED');
});

test('PM selection: contradictory duplicates cannot win by whichever metric appears first', () => {
  for (const values of [[0.8, 80], [80, 0.8]]) {
    const result = compare([daily], values.map(value => metric(daily, value)), [metric(daily, 0.4)]);
    assert.notEqual(result.recommendedTrialId, 'trial:a');
    const ambiguous = result.candidates.find(candidate => candidate.trialId === 'trial:a');
    assert.equal(ambiguous?.status, 'AMBIGUOUS');
    assert.equal(ambiguous?.frontierMembership, 'UNDETERMINED');
  }
});

test('PM selection: a late observation or independent result cannot improve an earlier selection', () => {
  for (const forbidden of [metric(daily, 10, { availableAtNs: '301' }), metric(daily, 10, { role: 'INDEPENDENT' })]) {
    const result = compare([daily], [forbidden], [metric(daily, 0.01)]);
    assert.notEqual(result.recommendedTrialId, 'trial:a');
    assert.equal(result.candidates.find(candidate => candidate.trialId === 'trial:a')?.frontierMembership, 'UNDETERMINED');
  }
});

test('PM metric admission: finite payloads do not rehabilitate unavailable values or a changed registered definition', () => {
  assert.throws(() => validateMetricObservation(metric(daily, 0.4, { status: 'INSUFFICIENT' }), [daily]));
  assert.throws(() => validateMetricObservation(metric({ ...sharpe252, annualizationDays: 365 }, 3), [sharpe252]));
  assert.throws(() => validateMetricObservation(metric(daily, Infinity), [daily]));
  assert.deepEqual(validateMetricObservation(metric(daily, 0.4), [daily]), metric(daily, 0.4));
});

function capacity(): CapacityEstimate {
  return { ...fact('capacity:original'), strategyVersionId: 'strategy:original', ensembleRevisionId: 'ensemble:original',
    basisRevision: 'basis:original', level: 'CONDITIONAL_FEASIBILITY', status: 'AVAILABLE', currency: 'USD', observedInterval: interval,
    asOfNs: '200', validUntilNs: '400', minimumTestedCapital: money('200'), maximumTestedCapital: money('500'),
    supportedCapitalUpperBound: money('500'), feasibleRegions: ['200', '500'].map(amount => ({ lower: money(amount), upper: money(amount),
      kind: 'TESTED_POINT' as const, configurationEvidenceRef: `configuration:${amount}`, evidenceRef: `replay:${amount}` })),
    curve: ['200', '500'].map(amount => ({ capital: money(amount), status: 'AVAILABLE' as const, replayEvidenceRef: `replay:${amount}`,
      strategyVersionId: 'strategy:original', portfolioDecisionRef: null, netPerformance: [metric(daily, 0.01)],
      entryDemand: money(amount), exitDemand: money(amount), estimatedCost: money('1'), fillRatio: 1,
      bindingConstraints: [], reasonCodes: [] })), liquidityModelRef: 'liquidity:observed', impactModelRef: null,
    executionModelRef: 'execution:conditional', exitModelRef: 'exit:conditional', evidenceRefs: ['replay:200', 'replay:500'],
    assumptions: ['Synthetic conditional replay points; no economic capacity claim.'], missingEvidence: ['observed-market-impact'] };
}

test('PM capacity: a scalar upper bound cannot fill an untested hole or prove small capital is feasible', () => {
  const estimate = validateCapacityEstimate(capacity());
  assert.equal(assessCapacityCurve(estimate, money('200')).feasible, true);
  assert.equal(assessCapacityCurve(estimate, money('500')).feasible, true);
  for (const amount of ['50', '199.999999999999999999', '300', '499.999999999999999999']) {
    assert.equal(assessCapacityCurve(estimate, money(amount)).feasible, null,
      `${amount} has no tested point or validated interval; it is unknown, not automatically feasible.`);
  }
});

test('PM capacity: a tested point must not silently become an interpolation interval', () => {
  const estimate = capacity();
  estimate.feasibleRegions[0]!.upper = money('400');
  assert.throws(() => validateCapacityEstimate(estimate));
  const currencyMismatch = capacity();
  currencyMismatch.feasibleRegions[0]!.lower.currency = 'EUR';
  assert.throws(() => validateCapacityEstimate(currencyMismatch));
});

function capital(): CapitalPlan {
  return { equity: money('3000.000000000000000001'), reserved: money('100.000000000000000001'),
    ownerLiquidityReserve: money('50'), deployableBudget: money('2850'), committedCapital: money('2800'),
    uncommittedCash: money('50'), projectedTradingCash: money('10'), capacityEstimateIds: ['capacity:original'], sizingEvidenceRef: 'sizing:original' };
}

test('PM funds: a sub-Number cash discrepancy must not pass conservation or be counted twice', () => {
  const original = capital();
  assert.deepEqual(validateCapitalPlan(original), original);
  assert.throws(() => validateCapitalPlan({ ...original, deployableBudget: money('2850.000000000000000001') }));
  assert.throws(() => validateCapitalPlan({ ...original, uncommittedCash: money('60') }),
    'Trading cash inside committed capital is not additional uncommitted cash.');
  assert.throws(() => validateCapitalPlan({ ...original, reserved: { amount: original.reserved.amount, currency: 'EUR' } }));
});

test('PM factor admission: exact zero weights are legal, duplicate factor identities cannot gain extra representation', () => {
  const ensemble: EnsembleRevision = { ...fact('ensemble:original'), strategyVersionId: 'strategy:original', policyRevisionId: 'policy:original',
    members: [{ factorVersionId: 'factor:a', calibrationModelId: null, weight: '1', economicFamilyId: 'family:a', contributionEvidenceRef: null },
      { factorVersionId: 'factor:b', calibrationModelId: null, weight: '0', economicFamilyId: 'family:b', contributionEvidenceRef: null }],
    predictionUnit: 'RETURN_RATIO', labelDefinitionId: 'label:next-day-gross', fitProtocolRef: 'protocol:original', fitCutoffNs: '200',
    trainingEvidenceRefs: ['evidence:training'], jointValidationEvidenceRefs: ['evidence:selection'], qualification: 'CANDIDATE', rejectionReasonCodes: [] };
  assert.deepEqual(validatePmEntity('EnsembleRevision', ensemble), ensemble);
  assert.throws(() => validatePmEntity('EnsembleRevision', { ...ensemble,
    members: [{ ...ensemble.members[0]!, weight: '0.5' }, { ...ensemble.members[0]!, weight: '0.5' }] }));
});

test('PM allocation: one account cannot fund duplicated strategy budgets or inconsistent capital fractions', () => {
  const funds: CapitalPlan = { equity: money('1000'), reserved: money('0'), ownerLiquidityReserve: money('0'),
    deployableBudget: money('1000'), committedCapital: money('1000'), uncommittedCash: money('0'),
    projectedTradingCash: money('0'), capacityEstimateIds: ['capacity:original'], sizingEvidenceRef: 'sizing:original' };
  const decision: PortfolioDecision = { ...fact('decision:original'), cycleId: 'cycle:original', mandateRevisionId: 'mandate:original',
    strategyVersionId: 'strategy:original', ensembleRevisionId: 'ensemble:original', policyRevisionId: 'policy:original',
    basisRevision: 'basis:original', incumbentDecisionId: null, accountBasis: null, researchStartingStateRef: 'research:starting-state',
    action: 'REBALANCE', status: 'CANDIDATE', capital: funds,
    strategyAllocations: [{ strategyVersionId: 'strategy:original', ensembleRevisionId: 'ensemble:original',
      committedCapital: money('1000'), capitalFraction: '1', allocationEvidenceRef: 'allocation:original',
      riskContributionEvidenceRef: null, nettingGroupId: 'account:one' }],
    assetTargets: [{ instrumentId: 'TEST.SIM', navWeight: '1', targetQuantity: '10', targetNotional: money('1000'),
      currentNotional: money('0'), netTradeNotional: money('1000'), quantityRuleRef: 'quantity:original', riskContribution: null }],
    comparisonRef: 'comparison:operating', solverEvidenceRef: 'solver:original', jointReplayEvidenceRef: 'replay:original',
    reasonCodes: [], explanationFacts: {}, validUntilNs: '400', nextInvalidationDependencies: ['dataset:original'], targetCommandId: null };
  assert.deepEqual(validatePmEntity('PortfolioDecision', decision), decision);
  assert.throws(() => validatePmEntity('PortfolioDecision', { ...decision, strategyAllocations: [] }));
  assert.throws(() => validatePmEntity('PortfolioDecision', { ...decision, strategyAllocations: [
    { ...decision.strategyAllocations[0]!, committedCapital: money('500'), capitalFraction: '0.5' },
    { ...decision.strategyAllocations[0]!, committedCapital: money('500'), capitalFraction: '0.5' },
  ] }), 'Splitting one strategy identity into two entries must not duplicate capital attribution.');
  assert.throws(() => validatePmEntity('PortfolioDecision', { ...decision,
    strategyAllocations: [{ ...decision.strategyAllocations[0]!, capitalFraction: '0.8' }] }));
  const historical = { ...decision, assetTargets: decision.assetTargets.map(target => ({ ...target, currentNotional: null, netTradeNotional: null })) };
  assert.deepEqual(validatePmEntity('PortfolioDecision', historical), historical,
    'Historical normalized allocations must preserve unknown current holdings instead of inventing zero.');
  assert.throws(() => validatePmEntity('PortfolioDecision', { ...historical, targetCommandId: 'command:external' }),
    'A normalized research allocation cannot be delivered as a current-account target.');
  assert.throws(() => validatePmEntity('AssetTarget', { ...historical.assetTargets[0]!, netTradeNotional: money('1000') }));
  const thirds: PortfolioDecision = { ...decision, strategyVersionId: 'strategy:composite',
    capital: { ...funds, equity: money('3000'), deployableBudget: money('3000'), committedCapital: money('3000') },
    assetTargets: [{ ...decision.assetTargets[0]!, targetQuantity: '30', targetNotional: money('3000'), netTradeNotional: money('3000') }],
    strategyAllocations: ['a', 'b', 'c'].map(id => ({ ...decision.strategyAllocations[0]!, strategyVersionId: `strategy:${id}`, capitalFraction: null })) };
  assert.deepEqual(validatePmEntity('PortfolioDecision', thirds), thirds,
    'Three exact 1000 allocations remain legal even though one third has no finite decimal representation.');
  assert.throws(() => validatePmEntity('PortfolioDecision', { ...thirds,
    strategyAllocations: thirds.strategyAllocations.map(item => ({ ...item, capitalFraction: '0.333333333333333333' })) }),
    'An approximate display ratio must not be misrepresented as an exact monetary allocation fact.');
});

test('PM operating comparison: neither a different account basis nor a nonexistent candidate can inherit the selected action', () => {
  const comparison: OperatingComparison = { ...fact('comparison:operating'), cycleId: 'cycle:original', basisRevision: 'basis:original',
    policyRevisionId: 'policy:original', scope, incumbentDecisionId: 'decision:previous', evaluationContext: 'FORWARD_MODEL_ESTIMATE',
    candidates: [{ id: 'candidate:hold', kind: 'HOLD_CURRENT', configurationEvidenceRef: 'configuration:held', status: 'AVAILABLE',
      transitionCost: [], netOutcome: [metric(daily, 0.01)], downside: [metric(drawdown, 0.02)], capacityEstimateIds: ['capacity:original'], reasonCodes: [] }],
    pairedDifferenceEvidenceRefs: ['comparison:paired'], selectedCandidateId: 'candidate:hold', conclusion: 'SELECTED', reasonCodes: [] };
  const context = { cycleId: 'cycle:original', basisRevision: 'basis:original', asOfNs: '300', definitions: [daily, drawdown] };
  assert.deepEqual(validateOperatingComparison(comparison, context), comparison);
  assert.throws(() => validateOperatingComparison(comparison, { ...context, basisRevision: 'basis:new-account' }));
  assert.throws(() => validateOperatingComparison({ ...comparison, selectedCandidateId: 'candidate:missing' }, context));
});

test('PM operating choice: a proved infeasible capital configuration is excluded, while an unexplained compute failure is not', () => {
  const comparison: OperatingComparison = { ...fact('comparison:capital-hole'), cycleId: 'cycle:original', basisRevision: 'basis:original',
    policyRevisionId: 'policy:original', scope, incumbentDecisionId: null, evaluationContext: 'HISTORICAL_JOINT_REPLAY',
    candidates: [
      { id: 'configuration:500', kind: 'REBALANCE', configurationEvidenceRef: 'configuration:500', status: 'AVAILABLE',
        transitionCost: [], netOutcome: [metric(daily, 0.01)], downside: [], capacityEstimateIds: ['capacity:500'], reasonCodes: [] },
      { id: 'configuration:252.5', kind: 'REBALANCE', configurationEvidenceRef: 'configuration:252.5', status: 'UNAVAILABLE',
        transitionCost: [], netOutcome: [], downside: [], capacityEstimateIds: ['capacity:252.5'], reasonCodes: ['lot_constraint_failed'],
        feasibility: { status: 'INFEASIBLE', evidenceRefs: ['native:original-lot-constraint-failure'], reasonCodes: ['SIMULATION_LOT_ROUNDING_EXCEEDS_TOLERANCE'] } },
      { id: 'configuration:cash', kind: 'CASH', configurationEvidenceRef: 'configuration:cash', status: 'AVAILABLE',
        transitionCost: [], netOutcome: [metric(daily, 0)], downside: [], capacityEstimateIds: [], reasonCodes: [] },
    ], pairedDifferenceEvidenceRefs: [], selectedCandidateId: null, conclusion: 'NEEDS_EVIDENCE', reasonCodes: [] };
  const input = { comparison, selection: policy([daily]).selectionRule, definitions: [daily], asOfNs: '300' };
  const proved = compareOperatingCandidates(input).comparison;
  assert.equal(proved.conclusion, 'SELECTED');
  assert.equal(proved.selectedCandidateId, 'configuration:500');
  assert.equal(proved.candidates.length, 3, 'An infeasible tested configuration remains disclosed.');
  const unexplained = structuredClone(comparison);
  delete unexplained.candidates[1]!.feasibility;
  const unknown = compareOperatingCandidates({ ...input, comparison: unexplained }).comparison;
  assert.equal(unknown.conclusion, 'NEEDS_EVIDENCE');
  assert.equal(unknown.selectedCandidateId, null, 'A missing process result cannot be converted into proof of infeasibility.');
});

const projectionContext = { role: 'SELECTION' as const, interval, asOfNs: '200', availableAtNs: '200',
  evidenceRefs: ['native:original-validation'], methodRef: 'projection:registered', currency: 'USD', calendarId: 'calendar:UTC' };
const nativeStatistic = (native_key: string, value: number, rest: Record<string, unknown> = {}) =>
  ({ group: 'RETURNS', native_key, currency: null, value, reason_code: null, ...rest });
function project(definitions: MetricDefinition[], statistics: ReturnType<typeof nativeStatistic>[], rest: Record<string, unknown> = {}) {
  return projectNativeValidationMetrics({ portfolio: { partition: 'VALIDATION', statistics, ...rest } }, definitions, projectionContext);
}

test('PM native projection: unavailable and duplicate values cannot become a more attractive business observation', () => {
  for (const status of ['INSUFFICIENT', 'CONSTANT_SCORE', 'UNSUPPORTED']) {
    const [observation] = project([daily], [nativeStatistic('Calendar Geometric Daily Return', 9, { status })]);
    assert.notEqual(observation!.status, 'AVAILABLE');
    assert.equal(observation!.value, null);
  }
  for (const extras of [{ returns_status: 'UNAVAILABLE' }, {}]) {
    const rows = extras.returns_status ? [nativeStatistic('Calendar Geometric Daily Return', 9)]
      : [nativeStatistic('Calendar Geometric Daily Return', 9, { reason_code: 'NATIVE_STATISTIC_UNAVAILABLE' })];
    const [observation] = project([daily], rows, extras);
    assert.notEqual(observation!.status, 'AVAILABLE');
    assert.equal(observation!.value, null);
  }
  const [duplicate] = project([daily], [nativeStatistic('Calendar Geometric Daily Return', 0.01), nativeStatistic('Calendar Geometric Daily Return', 9)]);
  assert.equal(duplicate!.status, 'AMBIGUOUS');
  assert.equal(duplicate!.value, null);
});

test('PM native projection: annualization identity and drawdown loss direction preserve the original scientific meaning', () => {
  const observations = project([sharpe252, sharpe365, drawdown], [nativeStatistic('Sharpe Ratio (365 days)', 1.8),
    nativeStatistic('Sharpe Ratio (252 days)', 1.2), nativeStatistic('Max Drawdown', -0.18)]);
  assert.deepEqual(observations.map(observation => observation.value), [1.2, 1.8, 0.18]);
  assert.equal(observations[2]!.raw!.value, -0.18);
  assert.ok(observations[2]!.raw!.conversionVersion, 'The sign change must remain auditable.');
  assert.equal(project([sharpe252], [nativeStatistic('Sharpe Ratio (365 days)', 1.8)])[0]!.value, null);
  assert.notEqual(project([drawdown], [nativeStatistic('Max Drawdown', 0.18)])[0]!.status, 'AVAILABLE',
    'A positive value under the signed native key must not silently reverse the loss convention.');
});

test('PM native projection: losing-day frequency includes every complete day, including cash days', () => {
  const fraction: MetricDefinition = { ...daily, id: 'metric:losing-day-fraction', key: 'account.negative_day_fraction', unit: 'RATIO', direction: 'MINIMIZE' };
  const rows = [nativeStatistic('Calendar Negative Days', 1), nativeStatistic('Calendar Zero Days', 2),
    nativeStatistic('Calendar Observed Return Days', 4), nativeStatistic('Calendar Expected Days', 4), nativeStatistic('Calendar Coverage Ratio', 1)];
  const [complete] = project([fraction], rows);
  assert.equal(complete!.status, 'AVAILABLE');
  assert.equal(complete!.value, 0.25, 'Loss frequency is not one losing day divided by two active trading days.');
  assert.equal(complete!.sampleCount, '4');
  const incomplete = rows.map(row => row.native_key === 'Calendar Observed Return Days' ? { ...row, value: 3 }
    : row.native_key === 'Calendar Coverage Ratio' ? { ...row, value: 0.75 } : row);
  assert.notEqual(project([fraction], incomplete)[0]!.status, 'AVAILABLE');
});

test('PM native projection: decimal recorded costs retain their currency and cannot be supplied by held-out results', () => {
  const commission: MetricDefinition = { ...daily, id: 'metric:commission', key: 'cost.recorded_commission', unit: 'CURRENCY',
    valueEncoding: 'DECIMAL', direction: 'MINIMIZE', horizonRef: 'horizon:evaluation-window' };
  const total = '1.000000000000000001';
  const [observation] = project([commission], [], { recorded_fee_totals: { currency: 'USD', commission_total: total } });
  assert.equal(observation!.value, total);
  assert.equal(observation!.currency, 'USD');
  assert.equal(project([commission], [], { recorded_fee_totals: { currency: 'EUR', commission_total: total } })[0]!.value, null);
  assert.equal(project([commission], [], { partition: 'HELDOUT', recorded_fee_totals: { currency: 'USD', commission_total: total } })[0]!.value, null);
});

test('PM native projection: component and independent factor evidence cannot masquerade as the shared account objective', () => {
  const ic: MetricDefinition = { ...sharpe252, id: 'metric:pearson', key: 'factor.pearson_ic', annualizationDays: null };
  const partition = { partition: 'VALIDATION', start_ns: interval.startNs, end_ns: interval.endExclusiveNs,
    metric: { pearson_ic: { status: 'AVAILABLE', value: 0.4 }, counts: { included: '6', observations: '6' } } };
  const positive = projectNativeValidationMetrics({ partitions: [partition] }, [ic], projectionContext)[0]!;
  assert.equal(positive.status, 'AVAILABLE');
  for (const result of [{ partitions: [{ ...partition, partition: 'HELDOUT' }] },
    { portfolio: { partition: 'VALIDATION', statistics: [] }, partitions: [partition] },
    { partitions: [partition], partitions_component: [partition] }]) {
    assert.equal(projectNativeValidationMetrics(result, [ic], projectionContext)[0]!.value, null);
  }
  assert.equal(projectNativeValidationMetrics({ partitions: [partition, partition] }, [ic], projectionContext)[0]!.status, 'AMBIGUOUS');
});

test('PM independent evidence: the owner can read a frozen final result without reopening candidate selection', () => {
  const result = { portfolio: { partition: 'HELDOUT', statistics: [nativeStatistic('Calendar Geometric Daily Return', 0.05),
    nativeStatistic('Sharpe Ratio (252 days)', 0.8), nativeStatistic('Sharpe Ratio (365 days)', 1.0)] } };
  const final = projectNativeIndependentMetrics(result, [daily, sharpe252], { ...projectionContext, role: 'INDEPENDENT' });
  assert.deepEqual(final.map(item => [item.role, item.value]), [['INDEPENDENT', 0.05], ['INDEPENDENT', 0.8]]);
  assert.equal(projectNativeValidationMetrics(result, [daily], projectionContext)[0]!.value, null);
  assert.equal(projectNativeIndependentMetrics({ portfolio: { ...result.portfolio, partition: 'VALIDATION' } }, [daily],
    { ...projectionContext, role: 'INDEPENDENT' })[0]!.value, null);
  const attemptToReselect = compare([daily], [final[0]!], [metric(daily, 0.01)]);
  assert.notEqual(attemptToReselect.recommendedTrialId, 'trial:a');
  assert.equal(attemptToReselect.candidates.find(item => item.trialId === 'trial:a')!.metrics.length, 0,
    'The newly supported final-data reader cannot make final values available to selection feedback.');
});
