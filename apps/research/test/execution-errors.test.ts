import assert from 'node:assert/strict';
import { test } from 'node:test';
import { classifyExecutionError, ExecutionFailure } from '../src/execution-errors.ts';

for (const [raw, code, category] of [
  ['QZ_SIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED\nSIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED', 'QZ_SIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED', 'runtime'],
  ['QZ_NATIVE_JOB_FAILED\nCATALOG_EMPTY_SELECTION', 'CATALOG_EMPTY_SELECTION', 'data'],
  ['QZ_NATIVE_JOB_FAILED\nSIGNAL_ABI_MISMATCH', 'SIGNAL_ABI_MISMATCH', 'model'],
  ['QZ_NATIVE_JOB_FAILED\nSIGNAL_EXECUTION_FAILED', 'SIGNAL_EXECUTION_FAILED', 'model'],
  ['QZ_NATIVE_JOB_FAILED\nresearch contract fields are invalid; split_policy.train_size [OUT_OF_RANGE]', 'native_request_invalid', 'request'],
  ['heldout_already_exposed', 'heldout_already_exposed', 'request'],
  ['strategy_market_allocations_required', 'strategy_market_allocations_required', 'request'],
  ['strategy_instrument_fee_missing', 'strategy_instrument_fee_missing', 'request'],
  ['ENOSPC: private file', 'ENOSPC', 'resource'],
  ['qz-native: line 1: /private/job: No such file or directory', 'native_executable_unavailable', 'runtime'],
  ['validate failed:\nfactor.wat:1:2: error: invalid memory limits', 'wasm_compile_failed', 'model'],
  ['aborted', 'aborted', 'cancelled'],
]) test(`classifies retained diagnostic ${code}`, () => {
  const detail = classifyExecutionError(raw)!;
  assert.equal(detail.code, code); assert.equal(detail.category, category);
  assert.match(detail.message, /[\u4e00-\u9fff]/); assert.match(detail.action, /[\u4e00-\u9fff]/);
  assert.doesNotMatch(JSON.stringify(detail), /\/private|split_policy|factor\.wat/);
});

for (const code of ['strategy_market_not_allowed', 'strategy_instrument_not_allowed']) test(`${code} explains the original research universe`, () => {
  const detail = classifyExecutionError(code)!;
  assert.equal(detail.code, code); assert.equal(detail.category, 'request');
  assert.match(detail.message, /原研究.*范围/); assert.match(detail.action, /已登记.*市场.*标的/);
});

test('unrecognized lowercase public codes keep their exact value and an honest unknown explanation', () => {
  const code = 'custom_evaluator_failed';
  for (const context of [{}, { stage: 'compile' }, { stage: 'decode' }]) {
    const detail = classifyExecutionError(code, 'backtest_failed', context)!;
    assert.equal(detail.code, code); assert.equal(detail.category, 'unknown');
    assert.match(detail.message, /无法确定具体原因/); assert.match(detail.action, /原研究、试验或任务编号/);
  }
  assert.equal(classifyExecutionError('x'.repeat(128))!.code, 'x'.repeat(128));
});

test('unknown freeform diagnostics and uppercase codes are not echoed as public text', () => {
  for (const raw of ['QZ_NEW_PRIVATE_DETAIL', '/private/path token=synthetic-secret\n(module private-source)', 'Error source: secret',
    'custom_evaluator_failed: /private/path', ' custom_evaluator_failed', 'custom_evaluator_failed\n', 'x'.repeat(129)]) {
    const detail = classifyExecutionError(raw, 'backtest_failed', { operation: '/private/operation', stage: 'private-source' })!;
    assert.equal(detail.code, 'backtest_failed'); assert.equal(detail.category, 'unknown');
    assert.doesNotMatch(JSON.stringify(detail), /synthetic|private|secret/);
    assert.equal(detail.operation, undefined); assert.equal(detail.stage, undefined);
  }
  assert.equal(classifyExecutionError(null), null); assert.equal(classifyExecutionError(''), null);
  assert.equal(classifyExecutionError('QZ_NATIVE_JOB_FAILED\nprivate source')!.code, 'QZ_NATIVE_JOB_FAILED');
});

test('compile-stage context does not turn a resource failure or unknown compiler error into a syntax diagnosis', () => {
  assert.equal(classifyExecutionError(new ExecutionFailure(new Error('ENOMEM: private compiler'), { stage: 'compile' }))!.category, 'resource');
  const unknown = classifyExecutionError(new ExecutionFailure(new Error('private compiler failure'), { stage: 'compile' }))!;
  assert.equal(unknown.code, 'wasm_compile_failed'); assert.equal(unknown.category, 'unknown');
});

test('execution context retains the original diagnostic and distinguishes decoding from execution', () => {
  const raw = new SyntaxError('private-source');
  const error = new ExecutionFailure(raw, { operation: 'simulate', stage: 'decode' });
  assert.equal(error.message, raw.message); assert.equal(error.cause, raw);
  const detail = classifyExecutionError(error)!;
  assert.equal(detail.code, 'native_result_invalid'); assert.equal(detail.operation, 'simulate'); assert.equal(detail.stage, 'decode');
  assert.doesNotMatch(JSON.stringify(detail), /private-source/);
});

test('progress lines preserve fixed native codes and specific causes', () => {
  const progress = 'catalog_ensemble: partition=2/909 observations=20 markets=3\n';
  assert.equal(classifyExecutionError(`${progress}QZ_NATIVE_JOB_FAILED\nprivate cause`)!.code, 'QZ_NATIVE_JOB_FAILED');
  const specific = classifyExecutionError(`${progress}QZ_NATIVE_JOB_FAILED\nCATALOG_EMPTY_SELECTION: /private/catalog`, 'scientific_execution_failed', { exitCode: 137 })!;
  assert.equal(specific.code, 'CATALOG_EMPTY_SELECTION'); assert.equal(specific.category, 'data');
  assert.equal(specific.stage, 'execute'); assert.equal(specific.exitCode, 137); assert.doesNotMatch(specific.message, /SIGKILL/);
});

test('native exit status survives child context without asserting a signal or OOM', () => {
  const raw = 'catalog_ensemble: partition=2/909 observations=20 markets=3';
  const error = new ExecutionFailure(raw, { operation: 'replay-catalog-ensemble', stage: 'execute', exitCode: 137 });
  const child = classifyExecutionError(error)!;
  const restored = classifyExecutionError(new ExecutionFailure(raw, child))!;
  assert.equal(child.exitCode, 137);
  assert.deepEqual(restored, child); assert.equal(child.category, 'unknown');
  assert.match(child.message, /137/); assert.match(child.message, /可能/); assert.match(child.message, /显式/);
  assert.doesNotMatch(child.message, /OOM|内存不足|内存耗尽/);
  assert.equal('signal' in child, false);
  for (const exitCode of [-1, 1.5, NaN, Infinity, '137']) {
    assert.equal('exitCode' in classifyExecutionError(raw, 'scientific_execution_failed', { exitCode })!, false);
  }
});
