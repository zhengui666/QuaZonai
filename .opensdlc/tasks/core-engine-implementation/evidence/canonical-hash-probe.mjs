import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { executionId } from '../../../../apps/nautilus/src/server.ts';
function original(value) {
  if(value === null || typeof value !== 'object') return JSON.stringify(value);
  if(Array.isArray(value)) return `[${value.map(original).join(',')}]`;
  return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${original(value[key])}`).join(',')}}`;
}
if (!['original', 'buffered'].includes(process.argv[2])) throw new Error('usage: node --experimental-strip-types --expose-gc canonical-hash-probe.mjs original|buffered');
if (typeof global.gc !== 'function') throw new Error('run with --expose-gc');
const request={schemaVersion:1,invocationId:'11111111-1111-4111-8111-111111111111',implementationDigest:'sha256:'+'a'.repeat(64),command:'allocate',input:{rows:Array.from({length:100000},(_,index)=>({timestamp:'1790000000000000001',decimal:'1.000000000000000001',index}))},args:[],files:{model:Buffer.alloc(1024*1024,7).toString('base64')}};
global.gc(); const before=process.memoryUsage().heapUsed; const start=performance.now();
const digest=process.argv[2] === 'original' ? createHash('sha256').update(original(request)).digest('hex') : executionId(request);
console.log(JSON.stringify({mode:process.argv[2],rows:100000,wallMs:performance.now()-start,heapDeltaBytes:process.memoryUsage().heapUsed-before,maxRssKiB:process.resourceUsage().maxRSS,digest}));
