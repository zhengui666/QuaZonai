import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile, readdir, stat, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { withAbortSignal } from '@earendil-works/chord/context';
import { portfolioCapacity, unavailableCapacity } from '../src/capacity.ts';
import { Application, json } from '../src/facts.ts';
import { built, context, setup } from './support.ts';

const policy = { participation: '0.01', maximumAgeSeconds: 120, scenarios: ['1000.000000000000000001'] };

test('portfolio capacity uses only the exact saved native request/catalog and leaves original facts and tasks unchanged', async () => {
  const state = await setup();
  try {
    const { view } = await built(state);
    const original = await state.service.work.getPortfolio(view.id);
    assert.equal(original.view.capacity?.reason, 'explicit_capacity_policy_missing_or_incompatible');
    const before = await state.service.work.snapshot();
    const tasks = (await state.service.research.tasks()).records;
    const calls = state.faux.state.callCount;
    const capture = join(state.directory, 'capacity-call.jsonl');
    const script = join(state.directory, 'capacity-protocol.mjs');
    // Only a protocol response; the real-native test separately checks measured capacity.
    const wire = unavailableCapacity('USD', 'TEST_ONLY_REASSESSMENT', 'TEST_ONLY_NATIVE', policy);
    await writeFile(script, `import {appendFileSync, readFileSync} from 'node:fs';
const input = JSON.parse(readFileSync(0, 'utf8'));
appendFileSync(${JSON.stringify(capture)}, JSON.stringify({command:process.argv[2], args:process.argv.slice(3), input, cwd:process.cwd()}) + '\\n');
console.log(${JSON.stringify(JSON.stringify(wire))});\n`);
    await writeFile(state.options.config.jobBinary, `#!/bin/sh\nexec '${process.execPath}' '${script}' "$@"\n`, { mode: 0o700 });
    const response = await state.service.app.inject({ method: 'POST', url: `/api/v3/portfolios/${view.id}/capacity`, headers: state.headers, payload: policy });
    assert.equal(response.statusCode, 200, response.body);
    assert.deepEqual(response.json(), { ...wire, scope: view.id });
    const recorded = (await readFile(capture, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    assert.equal(recorded.length, 1, 'no forecasts, reselection, evaluation or backtest may execute');
    assert.equal(recorded[0].command, 'assess-capacity');
    assert.deepEqual(recorded[0].args, ['--catalog', original.native!.catalog]);
    assert.notEqual(original.native!.catalog, state.options.config.datasets[0]!.data.catalog, 'use the saved captured catalog, not current dataset configuration');
    assert.deepEqual(recorded[0].input, { schema_version: 1, simulation: original.native!.simulationRequest,
      policy: { schema_version: 1, maximum_age_seconds: 120, participation_limit: '0.01' }, capitals: policy.scenarios });
    await assert.rejects(stat(recorded[0].cwd), (error: unknown) => (error as NodeJS.ErrnoException).code === 'ENOENT');
    assert.deepEqual(await state.service.work.snapshot(), before, 'no portfolio, target, account or source facts may be written');
    assert.deepEqual((await state.service.research.tasks()).records, tasks, 'read-only capacity must not create a durable task');
    assert.equal(state.faux.state.callCount, calls);
    assert.deepEqual((await state.service.app.inject({ url: `/api/v3/portfolios/${view.id}`, headers: state.headers })).json().capacity, original.view.capacity);
  } finally { await state.cleanup(); }
});

test('old or uncomputed portfolio capacity is explicitly unavailable without reconstructing missing native inputs', async () => {
  const state = await setup();
  try {
    const { view } = await built(state);
    const original = await state.service.work.getPortfolio(view.id);
    const variants = [
      { status: 'FAILED', native: original.native, reason: 'portfolio_not_computed' },
      { status: 'COMPUTED', native: null, reason: 'portfolio_simulation_request_missing' },
      { status: 'COMPUTED', native: { catalog: original.native!.catalog }, reason: 'portfolio_simulation_request_missing' },
      { status: 'COMPUTED', native: { simulationRequest: original.native!.simulationRequest }, reason: 'portfolio_catalog_missing' },
      { status: 'COMPUTED', native: { ...original.native, catalog: join(state.directory, 'absent-original-catalog') }, reason: 'portfolio_catalog_missing' },
    ];
    await writeFile(state.options.config.jobBinary, '#!/bin/sh\necho unexpected_native_execution >&2\nexit 1\n', { mode: 0o700 });
    for (const variant of variants) {
      await state.service.research.harness.commit(async tx => {
        const facts = await tx.doc(Application);
        facts.portfolios[view.id] = json({ ...original, native: variant.native, view: { ...original.view, status: variant.status } });
      }, context);
      const before = await state.service.work.snapshot();
      const response = await state.service.app.inject({ method: 'POST', url: `/api/v3/portfolios/${view.id}/capacity`, headers: state.headers, payload: policy });
      assert.equal(response.statusCode, 200, response.body);
      const report = response.json();
      assert.equal(report.status, 'UNAVAILABLE'); assert.equal(report.reason, variant.reason); assert.equal(report.scope, view.id);
      assert.equal(report.maximumCapital, null); assert.equal(report.scenarios[0].capital, policy.scenarios[0]);
      assert.equal(report.scenarios[0].usableCapital, null);
      assert.deepEqual(await state.service.work.snapshot(), before);
    }
  } finally { await state.cleanup(); }
});

test('portfolio capacity preserves unknown execution errors and cancellation and cleans temporary work', async () => {
  const state = await setup();
  try {
    const { view } = await built(state);
    const original = await state.service.work.getPortfolio(view.id);
    const cwdPath = join(state.directory, 'capacity-cwd.txt'); const script = join(state.directory, 'capacity-error.mjs');
    await writeFile(script, `import {writeFileSync} from 'node:fs'; writeFileSync(${JSON.stringify(cwdPath)}, process.cwd()); console.error('original private portfolio capacity failure'); process.exit(1);\n`);
    await writeFile(state.options.config.jobBinary, `#!/bin/sh\nexec '${process.execPath}' '${script}' "$@"\n`, { mode: 0o700 });
    await assert.rejects(portfolioCapacity(original, state.options.config, context, policy), /original private portfolio capacity failure/);
    await assert.rejects(stat(await readFile(cwdPath, 'utf8')), (error: unknown) => (error as NodeJS.ErrnoException).code === 'ENOENT');
    const before = (await readdir(state.directory)).sort();
    const controller = new AbortController(); controller.abort();
    await assert.rejects(portfolioCapacity(original, state.options.config, withAbortSignal(controller.signal, context), policy));
    assert.deepEqual((await readdir(state.directory)).sort(), before);
    assert.deepEqual(await state.service.work.getPortfolio(view.id), original);
  } finally { await state.cleanup(); }
});

test('portfolio capacity validates capital assumptions before lookup/execution and preserves not-found identity', async () => {
  const state = await setup();
  try {
    const request = (payload: object) => state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios/absent-original/capacity', headers: state.headers, payload });
    const missing = await request(policy);
    assert.equal(missing.statusCode, 404); assert.equal(missing.json().error, 'portfolio_not_found');
    for (const [payload, code] of [
      [{ ...policy, maximumAgeSeconds: 0 }, 'research_liquidity_age_invalid'],
      [{ ...policy, participation: '2' }, 'research_participation_invalid'],
      [{ ...policy, scenarios: ['0.0000000000000000001'] }, 'research_capital_scenarios_invalid'],
      [{ ...policy, impact: '0.1' }, 'research_override_unsupported'],
    ] as const) {
      const invalid = await request(payload);
      assert.equal(invalid.statusCode, 400, invalid.body); assert.equal(invalid.json().error, code);
    }
  } finally { await state.cleanup(); }
});
