import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT, withAbortSignal } from '@earendil-works/chord/context';
import { assessCapacity, factorCapacity, unavailableCapacity } from '../src/capacity.ts';
import { built, setup } from './support.ts';

const policy = { participation: '0.01', maximumAgeSeconds: 120, scenarios: ['1000.000000000000000001'] };

test('capacity retains a concrete native failure and original exact scenario inputs', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-capacity-test-'));
  try {
    const binary = join(directory, 'controlled-job');
    await writeFile(binary, '#!/bin/sh\nprintf "%s\\n" QZ_NATIVE_JOB_FAILED CATALOG_INSTRUMENT_MISSING >&2\nexit 1\n', { mode: 0o700 });
    const report = await assessCapacity(binary, directory, { settings: { base_currency: 'USD' } }, policy, directory, BACKGROUND_CONTEXT);
    assert.equal(report.status, 'UNAVAILABLE');
    assert.equal(report.reason, 'CATALOG_INSTRUMENT_MISSING');
    assert.equal(report.scenarios[0]!.capital, policy.scenarios[0]);
    assert.equal(report.maximumCapital, null);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('unknown capacity exceptions retain their original diagnostic instead of a catch-all result', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-capacity-test-'));
  try {
    const binary = join(directory, 'controlled-job');
    await writeFile(binary, '#!/bin/sh\nprintf "%s\\n" "private original capacity failure" >&2\nexit 1\n', { mode: 0o700 });
    await assert.rejects(assessCapacity(binary, directory, { settings: { base_currency: 'USD' } }, policy, directory, BACKGROUND_CONTEXT), /private original capacity failure/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('factor and stored-portfolio capacity expose known data and executable failures through HTTP', async () => {
  const state = await setup();
  try {
    const { view } = await built(state);
    const factor = (await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers })).json()[0];
    assert.equal(factor.combinable, true);
    const resources = [{ url: `/api/v3/factors/${factor.id}/capacity`, scope: factor.id, operation: 'forecast' },
      { url: `/api/v3/portfolios/${view.id}/capacity`, scope: view.id, operation: 'assess-capacity' }];
    const before = await state.service.work.snapshot();
    await writeFile(state.options.config.jobBinary, '#!/bin/sh\nprintf "%s\\n" QZ_NATIVE_JOB_FAILED FORECAST_INSUFFICIENT_WARMUP "private-context-not-for-response" >&2\nexit 1\n', { mode: 0o700 });
    for (const failure of [
      { code: 'FORECAST_INSUFFICIENT_WARMUP', category: 'data', cause: /预热/, action: /历史.*慢周期/ },
      { code: 'native_executable_unavailable', category: 'runtime', cause: /原生科学程序无法启动/, action: /程序.*安装/ },
    ]) {
      if (failure.code === 'native_executable_unavailable') await rm(state.options.config.jobBinary);
      for (const resource of resources) {
        const response = await state.service.app.inject({ method: 'POST', url: resource.url, headers: state.headers, payload: policy });
        assert.equal(response.statusCode, 200, response.body);
        const report = response.json();
        assert.equal(report.status, 'UNAVAILABLE'); assert.equal(report.reason, failure.code); assert.equal(report.scope, resource.scope);
        assert.equal(report.maximumCapital, null); assert.equal(report.minimumCapital, null);
        assert.equal(report.scenarios[0].capital, policy.scenarios[0]); assert.equal(report.scenarios[0].reason, failure.code);
        assert.equal(report.scenarios[0].usableCapital, null);
        assert.equal(report.errorDetail.code, failure.code); assert.equal(report.errorDetail.category, failure.category);
        assert.equal(report.errorDetail.operation, resource.operation); assert.equal(report.errorDetail.stage, 'execute');
        assert.match(report.errorDetail.message, failure.cause); assert.match(report.errorDetail.action, failure.action);
        assert.doesNotMatch(response.body, /private-context-not-for-response/);
        assert.equal(response.body.includes(state.directory), false, 'do not expose private host paths');
      }
    }
    const normal = unavailableCapacity('USD', 'liquidity_stale', 'portfolio', policy);
    await writeFile(state.options.config.jobBinary, `#!/bin/sh\nprintf '%s\\n' '${JSON.stringify(normal)}'\n`, { mode: 0o700 });
    const unchanged = await state.service.app.inject({ method: 'POST', url: resources[1]!.url, headers: state.headers, payload: policy });
    assert.equal(unchanged.statusCode, 200); assert.deepEqual(unchanged.json(), { ...normal, scope: view.id }, 'normal native unavailable reasons are not classification envelopes');
    assert.deepEqual(await state.service.work.snapshot(), before, 'read-only diagnosis cannot change original facts or old reports');
  } finally { await state.cleanup(); }
});

test('unknown and cancelled capacity failures remain HTTP errors and factor cancellation still propagates', async () => {
  const state = await setup();
  try {
    const { view } = await built(state);
    const factor = (await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers })).json()[0];
    const before = await state.service.work.snapshot();
    for (const [diagnostic, code] of [['private original capacity failure', 'internal_error'], ['custom_capacity_failure', 'custom_capacity_failure'], ['aborted', 'aborted']]) {
      await writeFile(state.options.config.jobBinary, `#!/bin/sh\nprintf '%s\\n' '${diagnostic}' >&2\nexit 1\n`, { mode: 0o700 });
      for (const url of [`/api/v3/factors/${factor.id}/capacity`, `/api/v3/portfolios/${view.id}/capacity`]) {
        const response = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload: policy });
        assert.equal(response.statusCode, 500, response.body); assert.deepEqual(response.json(), { error: code });
      }
    }
    const controller = new AbortController(); controller.abort();
    await assert.rejects(factorCapacity(state.service.research, state.options.config, factor.id, withAbortSignal(controller.signal, BACKGROUND_CONTEXT), policy));
    assert.deepEqual(await state.service.work.snapshot(), before);
  } finally { await state.cleanup(); }
});

test('an unavailable report does not invent liquidity or economic capacity', () => {
  const report = unavailableCapacity('USD', 'explicit_capacity_policy_missing');
  assert.equal(report.maximumCapital, null);
  assert.deepEqual(report.scenarios, []);
  assert.equal(report.errorDetail, undefined);
  assert.equal(report.economicCapacity?.status, 'UNAVAILABLE');
  assert.equal(report.economicCapacity?.maximumCapital, null);
  assert.deepEqual(report.economicCapacity?.missingInputs, ['matched_out_of_sample_returns_at_capital_scales', 'venue_calibrated_market_impact', 'matched_spread_fees_and_execution_timing', 'validated_exit_execution']);
  const assumed = unavailableCapacity('USD', 'liquidity_stale', 'original-factor', policy);
  assert.deepEqual(assumed.volumeStress?.map(row => row.volumeMultiplier), ['1', '0.5']);
  assert.ok(assumed.volumeStress?.every(row => row.maximumCapital === null && row.scenarios[0]!.capital === policy.scenarios[0] && row.scenarios[0]!.usableCapital === null));
});
