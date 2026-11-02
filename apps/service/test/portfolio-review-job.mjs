// TEST ONLY: retain original command arguments around the existing controlled wire fixture.
import { appendFileSync } from 'node:fs';
const [log, command, ...args] = process.argv.slice(2);
appendFileSync(log + '.args', JSON.stringify({ command, args }) + '\n');
const original = console.log;
console.log = encoded => {
  const result = JSON.parse(encoded);
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
