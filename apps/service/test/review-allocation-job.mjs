// TEST ONLY: deterministic wire receipts. This is not a market simulator or numerical validator.
import { appendFileSync, existsSync, readFileSync, unlinkSync, writeFileSync } from 'node:fs';
import { setTimeout } from 'node:timers/promises';
const [log, command] = process.argv.slice(2); const input = JSON.parse(readFileSync(0, 'utf8'));
appendFileSync(log, JSON.stringify({ command, input }) + '\n');
const DAY = 86400000000000n;
let result;
if (command === 'review-source') {
  result = { schema_version:1, method_version:'qz.review-source.v1', validated:true, metadata:input.metadata };
} else if (command === 'forecast') {
  const start = BigInt(input.selection.event_start_ns), end = BigInt(input.selection.event_end_ns);
  result = { points: Array.from({ length: Number((end-start)/DAY) }, (_, i) => ({ instrument_id: 'TEST.SIM', ordinal: i,
    event_ns: String(start+BigInt(i)*DAY), available_ns: String(start+BigInt(i)*DAY), forecast: 1 })) };
} else if (command === 'blend-targets') {
  result = { target_points: input.rows.map(row => row[0]) };
} else if (command === 'simulate') {
  if (existsSync(log+'.pause')) { unlinkSync(log+'.pause'); writeFileSync(log+'.entered', 'interrupted_native_operation'); while (!existsSync(log+'.release')) await setTimeout(10); }
  const start = BigInt(input.selection.event_start_ns), end = BigInt(input.selection.event_end_ns);
  result = { returns_kind: 'PORTFOLIO_DAILY', returns_status: 'OK', returns: Array.from({ length: Number((end-start)/DAY) }, (_, i) => ({ timestamp_ns: String(start+BigInt(i)*DAY), value: existsSync(log+'.negative')&&start>=400n*DAY ? -0.01-(i%3)*0.001 : 0.01+(i%3)*0.001, reason_code: null })), statistics: [] };
} else if (command === 'review-allocate') {
  result = { schema_version: 1, kind: input.kind, method: 'QZ_REVIEW_QUALITY_DIVERSIFICATION_FIXED_DIAGONAL_10_V1', solver_version: 'clarabel/0.11.1',
    members: input.members.map(member => ({ id: member.id, source_report_id: member.source_report_id, quality_coefficient: 1, prior: 0.5, weight: '0.5' })),
    covariance: [[1,0],[0,1]], lambda: 1, objective: 0.5, primal_residual: 0, dual_residual: 0, requires_new_candidate: true, requires_shared_account_study: true };
} else if (command === 'review-science') {
  const metric = (code,value) => ({ code,value,status:'AVAILABLE',reason:null });
  result = { schema_version: 1, source_digest: input.source_digest, method_version: 'qz.review-science.v1', observations: input.analysis.timestamps_ns.length,
    scope: input.analysis.kind === 'SELECTION' ? 'SELECTION_COHORT' : 'SHARED_ACCOUNT', limitations: ['TEST_ONLY_WIRE_NOT_SCIENTIFIC_EVIDENCE'], metrics: input.analysis.kind === 'SELECTION' ? [metric('dsr',0.99),metric('pbo',0.1)] :
    [metric('candidate_net_return_lower',existsSync(log+'.negative')&&BigInt(input.analysis.timestamps_ns[0])>=400n*DAY?-0.05:0.05),metric('candidate_net_return_mean',0.01),metric('return_coverage',1),metric('positive_slice_fraction',1),metric('net_return_delta_lower',0.01)] };
} else throw new Error('TEST_ONLY_UNSUPPORTED_COMMAND:'+command);
console.log(JSON.stringify(result));
