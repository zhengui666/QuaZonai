import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { AccountRuntime } from '../src/account-runtime.ts';

const quote = (value: string) => "'" + value.replace(/'/g, "'\\''") + "'";
test('failed BOOT starts a new generation; an old exit cannot reject its pending request', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-protocol-'));
  const script = join(directory, 'protocol.mjs'), binary = join(directory, 'account-runtime'), counter = join(directory, 'starts');
  await writeFile(script, `import fs from 'node:fs';import readline from 'node:readline';
const counter=${JSON.stringify(counter)};let generation=1;try{generation=Number(fs.readFileSync(counter,'utf8'))+1}catch{}fs.writeFileSync(counter,String(generation));
process.on('SIGTERM',()=>{setTimeout(()=>process.exit(0),500)});
const lines=readline.createInterface({input:process.stdin});lines.on('line',line=>{const v=JSON.parse(line);const send=result=>console.log(JSON.stringify({requestId:v.requestId,result}));
if(v.operation==='BOOT'){if(generation===1)console.log(JSON.stringify({requestId:v.requestId,error:'controlled_boot_rejected'}));else send({ready:true})}
else if(v.operation==='SNAPSHOT')setTimeout(()=>send({generation,proxy:process.env.HTTPS_PROXY}),800);
else if(v.operation==='SHUTDOWN'){send({stopping:true});setTimeout(()=>process.exit(0),20)}});`);
  await writeFile(binary, `#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)}\n`, { mode: 0o700 });
  const previousProxy = process.env.HTTPS_PROXY; process.env.HTTPS_PROXY = 'http://configured-proxy.invalid:8080';
  const runtime = new AccountRuntime(binary, { id: 'paper', clientId: 'client', accountId: 'account', mode: 'paper', instruments: ['HYPE-USDC-SPOT.HYPERLIQUID'], startingBalance: '100', paperFeeRate: '0.001' }, '01900000-0000-7000-8000-000000000001', directory);
  try {
    await assert.rejects(runtime.request('SNAPSHOT'), /controlled_boot_rejected/);
    const snapshot = await runtime.request('SNAPSHOT');
    assert.equal(snapshot.generation, 2); assert.equal(snapshot.proxy, process.env.HTTPS_PROXY);
    assert.equal(await readFile(counter, 'utf8'), '2');
  } finally { if (previousProxy === undefined) delete process.env.HTTPS_PROXY; else process.env.HTTPS_PROXY = previousProxy; await runtime.close(); await rm(directory, { recursive: true, force: true }); }
});

test('SHUTDOWN acknowledgement allows native cache drain before terminating the process', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-shutdown-'));
  const script = join(directory, 'protocol.mjs'), binary = join(directory, 'account-runtime'), drained = join(directory, 'drained');
  await writeFile(script, `import fs from 'node:fs';import readline from 'node:readline';const lines=readline.createInterface({input:process.stdin});
lines.on('line',line=>{const v=JSON.parse(line);console.log(JSON.stringify({requestId:v.requestId,result:{applied:true}}));if(v.operation==='SHUTDOWN')setTimeout(()=>{fs.writeFileSync(${JSON.stringify(drained)},'native-cache-flushed');process.exit(0)},250)});`);
  await writeFile(binary, `#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)}\n`, { mode: 0o700 });
  const runtime = new AccountRuntime(binary, { id: 'paper', clientId: 'client', accountId: 'account', mode: 'paper', instruments: ['HYPE-USDC-SPOT.HYPERLIQUID'] }, '01900000-0000-7000-8000-000000000001', directory);
  try { await runtime.request('SNAPSHOT'); await runtime.close(); assert.equal(await readFile(drained, 'utf8'), 'native-cache-flushed'); }
  finally { await runtime.close(); await rm(directory, { recursive: true, force: true }); }
});
