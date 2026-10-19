import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { assessCapacity, unavailableCapacity } from '../src/capacity.ts';

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

test('an unavailable report does not invent liquidity or economic capacity', () => {
  const report = unavailableCapacity('USD', 'explicit_capacity_policy_missing');
  assert.equal(report.maximumCapital, null);
  assert.deepEqual(report.scenarios, []);
  assert.equal(report.economicCapacity?.status, 'UNAVAILABLE');
  assert.equal(report.economicCapacity?.maximumCapital, null);
  assert.deepEqual(report.economicCapacity?.missingInputs, ['matched_out_of_sample_returns_at_capital_scales', 'venue_calibrated_market_impact', 'matched_spread_fees_and_execution_timing', 'validated_exit_execution']);
  const assumed = unavailableCapacity('USD', 'liquidity_stale', 'original-factor', policy);
  assert.deepEqual(assumed.volumeStress?.map(row => row.volumeMultiplier), ['1', '0.5']);
  assert.ok(assumed.volumeStress?.every(row => row.maximumCapital === null && row.scenarios[0]!.capital === policy.scenarios[0] && row.scenarios[0]!.usableCapital === null));
});
