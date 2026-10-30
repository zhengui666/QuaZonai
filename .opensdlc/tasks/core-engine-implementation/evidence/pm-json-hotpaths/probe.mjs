import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { jsonChunks } from '../../../../../apps/research/src/native-json.ts';
import { pmArtifactDigest } from '../../../../../apps/service/src/pm-evidence.ts';

// Exact original encoders. The optimized variants import the real application.
function* fragments(value) {
  if (value === null || typeof value !== 'object') { yield JSON.stringify(value) ?? 'null'; return; }
  if (Array.isArray(value)) {
    yield '[';
    for (let index = 0; index < value.length; index++) { if (index) yield ','; yield* fragments(value[index]); }
    yield ']';
  } else {
    yield '{'; let first = true;
    for (const key of Object.keys(value)) {
      if (value[key] === undefined) continue;
      if (!first) yield ','; first = false;
      yield JSON.stringify(key); yield ':'; yield* fragments(value[key]);
    }
    yield '}';
  }
}
function* originalChunks(value, chunkSize) {
  let pending = '';
  for (const fragment of fragments(value)) {
    if (pending && pending.length + fragment.length > chunkSize) { yield pending; pending = ''; }
    pending += fragment;
  }
  if (pending) yield pending;
}
function originalDigest(value) {
  const hash = createHash('sha256');
  function visit(item) {
    if (item === null || typeof item !== 'object') { hash.update(JSON.stringify(item) ?? 'null'); return; }
    if (Array.isArray(item)) {
      hash.update('['); item.forEach((entry, index) => { if (index) hash.update(','); visit(entry); }); hash.update(']'); return;
    }
    hash.update('{'); let first = true;
    for (const key of Object.keys(item).filter(key => item[key] !== undefined).sort()) {
      if (!first) hash.update(','); first = false; hash.update(JSON.stringify(key)); hash.update(':'); visit(item[key]);
    }
    hash.update('}');
  }
  visit(value); return hash.digest('hex');
}
const [workload, variant, count = '20000'] = process.argv.slice(2);
if (!['chunks', 'digest'].includes(workload) || !['original', 'optimized'].includes(variant) || !global.gc) {
  throw new Error('usage: node --experimental-strip-types --expose-gc probe.mjs chunks|digest original|optimized [rows]');
}
const rows = Number(count);
if (!Number.isSafeInteger(rows) || rows < 1) throw new Error('positive row count required');
const value = { scope: 'SYNTHETIC_STORAGE_SHAPE_NOT_MARKET_EVIDENCE', schema_version: 1,
  rows: Array.from({ length: rows }, (_, index) => ({ index, original: { snapshot: {
    balances: [{ total: { amount: '3000.000000000000000001', currency: 'USDC' }, free: { amount: '0.000000000000000001', currency: 'USDC' } }],
    events: [{ source: { native: { clock: '1791849600000000001', label: '原生🐋', sides: ['BUY', 'SELL'], value: index } } }],
  } } })), scalar: 'x'.repeat(65_535) + '🐋' };
global.gc();
const heapBefore = process.memoryUsage().heapUsed; const cpuBefore = process.cpuUsage(); const started = performance.now();
let digest; let chunks = null; let utf8Bytes = null;
if (workload === 'chunks') {
  const hash = createHash('sha256'); chunks = 0; utf8Bytes = 0;
  for (const chunk of (variant === 'original' ? originalChunks : jsonChunks)(value, 1024 * 1024)) {
    hash.update(chunk); chunks++; utf8Bytes += Buffer.byteLength(chunk);
  }
  digest = hash.digest('hex');
} else digest = (variant === 'original' ? originalDigest : pmArtifactDigest)(value);
const wallMs = performance.now() - started; const cpu = process.cpuUsage(cpuBefore);
console.log(JSON.stringify({ node: process.version, workload, variant, rows, chunks, utf8Bytes, digest, wallMs,
  cpuUserMs: cpu.user / 1000, cpuSystemMs: cpu.system / 1000,
  heapDeltaBytes: process.memoryUsage().heapUsed - heapBefore, processMaxRssKiB: process.resourceUsage().maxRSS }));
