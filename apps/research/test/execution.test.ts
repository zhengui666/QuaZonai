import { reviewSourceChecks } from '../src/review-workflow.ts';
import type { ResearchInput } from '../src/documents.ts';
import { subscriptionSpeed } from '../src/models.ts';
import { createModels, fauxProvider, fauxAssistantMessage } from '@earendil-works/pi-ai';
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

test('research uses provider defaults and forwards explicitly configured native request options', () => {
  assert.equal(researchSettings().stream?.timeoutMs, undefined);
  assert.equal(researchSettings({ stream: { timeoutMs: 1 } }).stream?.timeoutMs, 1);
  assert.equal(researchSettings().stream?.maxRetries, 0);
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
    const data = { catalog: directory, request: { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { event_start_ns: '0', event_end_ns: '30' }, parameters: { total_fuel: '1' } } } };
    const original = structuredClone(data);
    const result = await evaluateWasm('(module)', data, BACKGROUND_CONTEXT);
    assert.equal(result.consumed_fuel, null);
    assert.equal(result.temporaryRoot, result.cwd);
    assert.equal((((result.request as JsonObject).forecast as JsonObject).parameters as JsonObject).total_fuel, '1');
    assert.deepEqual(data, original);
  } finally { if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous; await rm(directory, { recursive: true, force: true }); }
});

test('provider dispatch preserves explicit pi-ai options without a legacy checkpoint shim', async () => {
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  const wrapped = subscriptionSpeed(models).models;
  faux.setResponses([(_context, options) => {
    assert.equal(options?.maxTokens, 1);
    return fauxAssistantMessage('Original provider output is unmetered by the application.');
  }]);
  const result = await wrapped.completeSimple(faux.getModel(), { messages: [{ role: 'user', content: 'Resume the original request.', timestamp: Date.now() }] }, { maxTokens: 1 });
  assert.equal(result.stopReason, 'stop'); assert.equal(faux.state.callCount, 1);
});

test('native request echoes bind exact row and fuel contracts and scientific clocks', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-row-echo-')); const binary = join(directory, 'job'); const script = join(directory, 'echo.mjs');
  const echo = `let input='';for await(const chunk of process.stdin)input+=chunk;const request=JSON.parse(input);`;
  try {
    await writeFile(script, echo + `console.log(JSON.stringify({request,consumed_fuel:null}));`);
    await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${script}' "$@"\n`, { mode: 0o700 });
    for (const legacy of [{}, { maximum_rows: 0 }, { maximum_rows: 1 }] as JsonObject[]) {
      const data: JsonObject = { catalog: directory, request: { train_end_ns: '10', validation_end_ns: '20', forecast: {
        selection: { event_start_ns: '0', event_end_ns: '30', ...legacy }, parameters: { total_fuel: '1' } } } };
      const original = structuredClone(data);
      const result = await evaluateWasm('(module)', data, BACKGROUND_CONTEXT, undefined, binary);
      assert.equal((((result.request as JsonObject).forecast as JsonObject).parameters as JsonObject).total_fuel, '1');
      assert.deepEqual(data, original, 'execution normalization never rewrites the frozen request');
    }
    await writeFile(script, echo + `request.forecast.selection.event_end_ns='31';console.log(JSON.stringify({request,consumed_fuel:null}));`);
    const data: JsonObject = { catalog: directory, request: { train_end_ns: '10', validation_end_ns: '20', forecast: {
      selection: { event_start_ns: '0', event_end_ns: '30' }, parameters: {} } } };
    await assert.rejects(evaluateWasm('(module)', data, BACKGROUND_CONTEXT, undefined, binary), /native_result_request_mismatch/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('review causality rejects changed fuel, rows and scientific clocks', () => {
  const input: ResearchInput = { goal: 'Protocol-only source review', model: { provider: 'test', modelId: 'test' }, policy: {}, data: {
    request: { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { event_start_ns: '0', event_end_ns: '30' }, parameters: { total_fuel: '1' } } } } };
  const original = structuredClone(input);
  const request = iterationRequest(input.data); const forecast = request.forecast as JsonObject;
  const trial: JsonObject = { taskId: 1, status: 'COMPUTED', proposal: { model: '(module)' }, result: { request, partitions: [{ partition: 'VALIDATION' }] } };
  const checks = reviewSourceChecks(input, trial, [trial], 'test:validation');
  assert.equal(checks.find(check => check.id === 'no_leakage')?.status, 'PASS');
  assert.deepEqual(input, original);
  (forecast.selection as JsonObject).event_end_ns = '21';
  assert.equal(reviewSourceChecks(input, trial, [trial], 'test:validation').find(check => check.id === 'no_leakage')?.status, 'UNKNOWN');
});
