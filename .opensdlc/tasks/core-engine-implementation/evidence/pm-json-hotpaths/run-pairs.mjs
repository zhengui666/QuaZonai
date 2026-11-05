import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { performance } from 'node:perf_hooks';

const count = process.argv[2] ?? '20000'; const pairs = Number(process.argv[3] ?? '5');
if (!Number.isSafeInteger(pairs) || pairs < 1) throw new Error('positive pair count required');
const probe = fileURLToPath(new URL('./probe.mjs', import.meta.url));
const results = []; const started = performance.now();
for (const workload of ['chunks', 'digest']) for (let pair = 0; pair < pairs; pair++) {
  for (const variant of pair % 2 ? ['optimized', 'original'] : ['original', 'optimized']) {
    const run = spawnSync(process.execPath, ['--experimental-strip-types', '--expose-gc', probe, workload, variant, count],
      { encoding: 'utf8', timeout: 60_000 });
    if (run.error || run.status !== 0) throw run.error ?? new Error(run.stderr);
    results.push({ ...JSON.parse(run.stdout), pair });
  }
}
function median(values) { const sorted = values.toSorted((a, b) => a - b); const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2; }
const summary = {};
for (const workload of ['chunks', 'digest']) {
  const group = results.filter(row => row.workload === workload);
  if (new Set(group.map(row => row.digest)).size !== 1) throw new Error(`${workload}: original bytes changed`);
  summary[workload] = {};
  for (const variant of ['original', 'optimized']) {
    const rows = group.filter(row => row.variant === variant);
    summary[workload][variant] = Object.fromEntries(['wallMs', 'cpuUserMs', 'cpuSystemMs', 'heapDeltaBytes', 'processMaxRssKiB']
      .map(key => [key, median(rows.map(row => row[key]))]));
  }
  summary[workload].pairedWallRatios = Array.from({ length: pairs }, (_, pair) =>
    group.find(row => row.pair === pair && row.variant === 'original').wallMs
      / group.find(row => row.pair === pair && row.variant === 'optimized').wallMs);
}
console.log(JSON.stringify({ completedAtUtc: new Date().toISOString(), elapsedSeconds: (performance.now() - started) / 1000,
  conditions: 'Independent cold Node processes, alternating pairs. Coordinate an idle host before running. Synthetic function workload only; no HTTP/science/whole-system ratio. Heap deltas are GC-sensitive; max RSS includes imports and fixture.',
  results, summary }, null, 2));
