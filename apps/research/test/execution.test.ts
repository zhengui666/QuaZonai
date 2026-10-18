import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { iterationRequest, evaluateWasm } from '../plugins/native-wasm.ts';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { researchSettings } from '../src/settings.ts';

test('research adds no stream deadline, including retired configured deadlines', () => {
  assert.equal(researchSettings().stream?.timeoutMs, undefined);
  assert.equal(researchSettings({ stream: { timeoutMs: 1 } }).stream?.timeoutMs, undefined);
});

test('native preparation preserves absent/null fuel and explicit scientific budgets', () => {
  for (const parameters of [{}, { total_fuel: null }, { total_fuel: '23' }] as JsonObject[]) {
    const data = { request: { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { event_start_ns: '0', event_end_ns: '30' }, parameters } } };
    const original = structuredClone(data);
    const request = iterationRequest(data);
    assert.deepEqual((request.forecast as { parameters: unknown }).parameters, parameters.total_fuel === null ? {} : parameters);
    assert.deepEqual(data, original);
  }
});

test('execution accepts the retained native serializer omitting null fuel without changing admitted input', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-fuel-protocol-')); const binary = join(directory, 'job'); const script = join(directory, 'echo.mjs');
  const previous = process.env.QZ_JOB_BINARY;
  try {
    // Protocol-only child: mirrors NativeForecastParametersV1's skip_serializing_if.
    await writeFile(script, `import { tmpdir } from 'node:os'; let input=''; for await (const chunk of process.stdin) input += chunk; const request=JSON.parse(input); if (request.forecast.parameters.total_fuel == null) delete request.forecast.parameters.total_fuel; console.log(JSON.stringify({request,consumed_fuel:null,temporaryRoot:tmpdir(),cwd:process.cwd()}));`);
    await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${script}' "$@"\n`, { mode: 0o700 });
    process.env.QZ_JOB_BINARY = binary;
    const data = { catalog: directory, request: { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { event_start_ns: '0', event_end_ns: '30' }, parameters: { total_fuel: null } } } };
    const original = structuredClone(data);
    const result = await evaluateWasm('(module)', data, BACKGROUND_CONTEXT);
    assert.equal(result.consumed_fuel, null);
    assert.equal(result.temporaryRoot, result.cwd);
    assert.equal(Object.hasOwn(((result.request as JsonObject).forecast as JsonObject).parameters as JsonObject, 'total_fuel'), false);
    assert.deepEqual(data, original);
  } finally { if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous; await rm(directory, { recursive: true, force: true }); }
});
