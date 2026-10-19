import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { iterationRequest, evaluateWasm } from '../plugins/native-wasm.ts';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT, withAbortSignal } from '@earendil-works/chord/context';
import { FactorTask } from '../plugins/native-factor-task.ts';
import { executeNative } from '../plugins/native-portfolio.ts';
import { classifyExecutionError, ExecutionFailure } from '../src/execution-errors.ts';
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

test('factor cancellation propagates without attempting to commit a scientific failure', async () => {
  const controller = new AbortController(); controller.abort();
  const compute = FactorTask.definition.phases.compute;
  let commits = 0;
  const task = { input: { model: '(module)', data: { catalog: '/unused', request: { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { event_start_ns: '0', event_end_ns: '30' }, parameters: {} } } } } } as unknown as Parameters<typeof compute>[0];
  const runtime = { signal: controller.signal, async commit() { commits++; } } as unknown as Parameters<typeof compute>[1];
  await assert.rejects(compute(task, runtime, withAbortSignal(controller.signal, BACKGROUND_CONTEXT)));
  assert.equal(commits, 0, 'official suspend/abort protocol, not a FAILED scientific outcome');
});

test('native stderr takes precedence over broken stdout, while a successful process still requires valid JSON', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-diagnostic-')); const binary = join(directory, 'job.mjs');
  const diagnostic = 'QZ_NATIVE_JOB_FAILED\nCATALOG_EMPTY_SELECTION\n/private/catalog token=synthetic-secret';
  try {
    for (const failed of [true, false]) {
      await writeFile(binary, `#!${process.execPath}\nfor await (const chunk of process.stdin) {}\nprocess.stdout.write('{"broken":');\n${failed ? `process.stderr.write(${JSON.stringify(diagnostic)}); process.exitCode = 1;` : ''}\n`, { mode: 0o700 });
      await assert.rejects(executeNative(binary, 'simulate', {}, [], directory, BACKGROUND_CONTEXT), (error: unknown) => {
        assert.ok(error instanceof ExecutionFailure);
        if (failed) assert.equal(error.message, diagnostic);
        const detail = classifyExecutionError(error)!;
        assert.equal(detail.code, failed ? 'CATALOG_EMPTY_SELECTION' : 'native_result_invalid');
        assert.equal(detail.stage, failed ? 'execute' : 'decode'); assert.equal(detail.operation, 'simulate');
        assert.doesNotMatch(JSON.stringify(detail), /\/private|synthetic-secret/);
        return true;
      });
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
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
