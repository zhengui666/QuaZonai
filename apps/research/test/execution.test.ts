import type { ResearchInput } from '../src/documents.ts';
import { subscriptionSpeed } from '../src/models.ts';
import { createModels, fauxProvider, fauxAssistantMessage } from '@earendil-works/pi-ai';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { nativeStrategyResult } from '../plugins/native-strategy-backtest.ts';
import { mkdtemp, realpath, rm, writeFile } from 'node:fs/promises';
import { basename, dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT, withAbortSignal } from '@earendil-works/chord/context';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';
import { executeNativeValue as executeNative } from '../src/native-execution.ts';
import { classifyExecutionError, ExecutionFailure } from '../src/execution-errors.ts';
import { researchSettings } from '../src/settings.ts';

test('research uses provider defaults and forwards explicitly configured native request options', () => {
  assert.equal(researchSettings().stream?.timeoutMs, undefined);
  assert.equal(researchSettings({ stream: { timeoutMs: 1 } }).stream?.timeoutMs, 1);
  assert.equal(researchSettings().stream?.maxRetries, 0);
});

test('upstream strategy cancellation propagates without attempting to commit a scientific failure', async () => {
  const controller = new AbortController(); controller.abort();
  const compute = NativeStrategyBacktestTask.definition.phases.compute;
  let commits = 0;
  const task = { input: { request: {}, catalog: '/unused', target: '/unused' } } as unknown as Parameters<typeof compute>[0];
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

test('native serialization preserves exact input and uses the configured isolated temporary root', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-indicator-protocol-')); const binary = join(directory, 'job'); const script = join(directory, 'echo.mjs');
  try {
    await writeFile(script, `import { tmpdir } from 'node:os'; let input=''; for await (const chunk of process.stdin) input += chunk; const request=JSON.parse(input); console.log(JSON.stringify({request,temporaryRoot:tmpdir(),cwd:process.cwd()}));`);
    await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${script}' "$@"\n`, { mode: 0o700 });
    const request = { schema_version: 1, instrument_id: 'BTC.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'BTC.SIM' }], event_start_ns: '1790000000000000001', event_end_ns: '1790000000000000031', available_end_ns: '1790000000000000031', venue: { base_currency: null }, strategy: { kind: 'EMA_CROSS', fast_period: 2, slow_period: 3, trade_size: '0.500000000000000001' } };
    const original = structuredClone(request);
    const result = await executeNative(binary, 'native-strategy-backtest', request, [], directory, BACKGROUND_CONTEXT) as JsonObject;
    const canonicalPath = async (path: string) => join(await realpath(dirname(path)), basename(path));
    assert.equal(await canonicalPath(String(result.temporaryRoot)), await canonicalPath(String(result.cwd)));
    assert.deepEqual(result.request, request); assert.deepEqual(request, original);
  } finally { await rm(directory, { recursive: true, force: true }); }
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

test('upstream request echoes reject changed source selectors, decimal quantities and nanosecond clocks', () => {
  const request: JsonObject = { schema_version: 1, instrument_id: 'BTC.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'BTC.SIM' }], venue: { base_currency: null },
    event_start_ns: '1790000000000000001', event_end_ns: '1790000000000000031', available_end_ns: '1790000000000000031', strategy: { kind: 'EMA_CROSS', trade_size: '0.500000000000000001' } };
  const original = structuredClone(request);
  for (const echoed of [
    { ...request, event_end_ns: '1790000000000000032' },
    { ...request, selectors: [{ data_type: 'QuoteTick', identifier: 'OTHER.SIM' }] },
    { ...request, strategy: { ...(request.strategy as JsonObject), trade_size: '0.500000000000000002' } },
  ]) assert.throws(() => nativeStrategyResult({ request: echoed }, request), /native_strategy_request_mismatch/);
  assert.throws(() => nativeStrategyResult({ request }, request), /native_strategy_version_mismatch/, 'matching request alone cannot imply a completed backtest');
  assert.deepEqual(request, original);
});
