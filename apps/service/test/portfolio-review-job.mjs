// TEST ONLY: retain original command arguments around the existing controlled wire fixture.
import { appendFileSync, existsSync, readFileSync } from 'node:fs';
const [log, command, ...args] = process.argv.slice(2);
appendFileSync(log + '.args', JSON.stringify({ command, args }) + '\n');
const original = console.log;
console.log = encoded => {
  const result = JSON.parse(encoded);
  if (command === 'review-science' && result.scope === 'SHARED_ACCOUNT' && existsSync(log + '.insufficient')) {
    const calls = readFileSync(log, 'utf8').trim().split('\n').map(line => JSON.parse(line));
    const last = calls.at(-1);
    if (BigInt(last.input.analysis.timestamps_ns[0]) >= 400n*86400000000000n) {
      const lower = result.metrics.find(metric => metric.code === 'candidate_net_return_lower');
      lower.value = null; lower.status = 'INSUFFICIENT_EVIDENCE'; lower.reason = 'TEST_ONLY_INDEPENDENT_SAMPLE_GAP';
    }
  }
  // The portfolio consumer also reads the original account snapshot envelope.
  if (command === 'simulate') {
    let equity = 1000;
    result.canonical_result = { portfolio_snapshots: result.returns.map(row => {
      equity *= 1 + row.value;
      return { ts_event: row.timestamp_ns, total_equity: [`${equity.toFixed(8)} USD`] };
    }) };
  }
  original(JSON.stringify(result));
};
await import('./review-allocation-job.mjs');
