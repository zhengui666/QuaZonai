import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { Exposures } from '../plugins/native-exposure.ts';
import { CatalogScopes } from '../plugins/native-catalog-scope.ts';
import research from '../plugins/research.ts';
import factor from '../plugins/factor-wasm.ts';
import heldout from '../plugins/heldout-wasm.ts';

const context = BACKGROUND_CONTEXT;
const tool = (name: string, args: JsonObject = {}) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
const model = '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 1))';

// Controlled child protocol tests storage/recovery/exposure, never numerical results or model quality.
test('large private scope is retained once and survives reopen/copies without resetting exposure or attempts', { timeout: 30_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-catalog-research-'));
  const binary = join(directory, 'job.mjs'); const calls = join(directory, 'calls');
  const source = join(directory, 'scope.json'); const catalog = join(directory, 'catalog'); const copied = join(directory, 'copied');
  await mkdir(catalog); await mkdir(copied);
  const types = Array.from({ length: 12_000 }, (_, i) => `PRIVATE_SCOPE_${String(i).padStart(6, '0')}_${'X'.repeat(64)}.SIM-1-MINUTE-LAST-EXTERNAL`);
  await writeFile(source, JSON.stringify(types));
  await writeFile(binary, `#!${process.execPath}
import { readFileSync, appendFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const args = process.argv.slice(2); appendFileSync(${JSON.stringify(calls)}, args[0] + '\\n');
if (args[0] === 'catalog-factor-scope') process.stdout.write(readFileSync(${JSON.stringify(source)}));
else {
  if (args[0] === 'scan-factor-catalog') assert.deepEqual(JSON.parse(readFileSync(args[args.indexOf('--scope') + 1])), JSON.parse(readFileSync(${JSON.stringify(source)})));
  let input = ''; for await (const chunk of process.stdin) input += chunk;
  console.log(JSON.stringify({ request: JSON.parse(input), test_only: 'CONTROLLED_PROTOCOL_NO_NUMERICAL_EVIDENCE' }));
}
`, { mode: 0o700 });
  const previous = process.env.QZ_JOB_BINARY; process.env.QZ_JOB_BINARY = binary;
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const parameters = { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1, total_fuel: null };
  const scan = { schema_version: 1, event_start_ns: '0', event_end_ns: '40', decision_cutoff_ns: '40', train_end_ns: '10', validation_end_ns: '40', maximum_rows: 100, parameters };
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [research, factor, heldout] };
  const input = { goal: 'Controlled full-scope persistence protocol, not market research', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog, catalog_scan_request: scan }, policy: { max_trials: 2 } };
  let host = await openResearch(options);
  try {
    faux.setResponses([tool('factor_evaluate', { hypothesis: 'Protocol A', model }), tool('factor_evaluate', { hypothesis: 'Protocol B', model }), tool('research_finish', { reason: 'End the bounded protocol, without scientific qualification.' })]);
    const first = await host.start('catalog-protocol', input);
    await (await host.harness.submission(first.submissionId, context))!.wait(context);
    const facts = (await host.state(first.conversationId)).research!;
    assert.deepEqual(facts.trials.map((trial) => trial.status), ['COMPUTED', 'COMPUTED'], JSON.stringify(facts.trials));
    assert.deepEqual(facts.input, input);
    const scopes = (await host.harness.snapshot(CatalogScopes, context))!;
    assert.equal(scopes.roots.length, 1);
    const reference = scopes.roots[0]!.reference as JsonObject;
    assert.equal(reference.native, undefined);
    const scopeTask = await host.harness.getTask(reference.taskId as TaskId<JsonObject>, context);
    assert.equal(scopeTask?.state.outcome?.status, 'completed');
    if (scopeTask?.state.outcome?.status === 'completed') assert.equal((scopeTask.state.outcome.result.native as JsonObject).format, 'qz.native-evidence/gzip-json/v1');
    assert.equal(reference.count, '12000');
    const windows = (await host.harness.snapshot(Exposures, context))!.windows;
    assert.equal(windows.length, 2);
    assert.deepEqual(windows[0]!.barTypesRef, windows[1]!.barTypesRef);
    assert.equal(windows[0]!.barTypes, undefined);
    assert.ok(JSON.stringify(windows).length < 2_000);
    assert.equal(JSON.stringify((await host.view(first.conversationId)).entries).includes(types.at(-1)!), false);
    assert.deepEqual((await readFile(calls, 'utf8')).trim().split('\n'), ['catalog-factor-scope', 'scan-factor-catalog', 'scan-factor-catalog']);
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.start('catalog-protocol', input), first);
    assert.deepEqual((await host.state(first.conversationId)).research, facts);
    assert.deepEqual(await host.harness.snapshot(CatalogScopes, context), scopes);

    const request = { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: { schema_version: 1, parameters,
      selection: { schema_version: 1, bar_types: [types.at(-1)!], event_start_ns: '0', event_end_ns: '40', decision_cutoff_ns: '40', maximum_rows: 100 } } };
    const legacy = { catalog: copied, datasetId: 'renamed', request };
    let trialId: number;
    faux.setResponses([tool('factor_evaluate', { hypothesis: 'Narrow legacy prefix', model }), async () => {
      const windows = (await host.harness.snapshot(Exposures, context))!.windows;
      trialId = windows.at(-1)!.taskId as number;
      return tool('factor_finalize', { trialId, reason: 'Must reject a suffix already exposed through full-catalog feedback.' });
    }]);
    const second = await host.start('copied-legacy-protocol', { ...input, data: { ...legacy, sealed: legacy } });
    await (await host.harness.submission(second.submissionId, context))!.wait(context);
    const rejected = (await host.state(second.conversationId)).research!;
    assert.equal(rejected.trials[0]!.status, 'COMPUTED');
    assert.equal(rejected.final!.status, 'FAILED');
    assert.equal(rejected.final!.error, 'heldout_already_exposed');
    assert.equal((await host.harness.snapshot(Exposures, context))!.windows.length, 3);
    assert.deepEqual((await readFile(calls, 'utf8')).trim().split('\n'), ['catalog-factor-scope', 'scan-factor-catalog', 'scan-factor-catalog', 'evaluate-factor']);

    faux.setResponses([tool('factor_evaluate', { hypothesis: 'Reject ambiguous scope before reading native inputs', model }), tool('research_finish', { reason: 'Failure is disclosed.' })]);
    const third = await host.start('ambiguous-catalog-protocol', { ...input, data: { ...input.data, request } });
    await (await host.harness.submission(third.submissionId, context))!.wait(context);
    const invalid = (await host.state(third.conversationId)).research!;
    assert.equal(invalid.trials.length, 1); assert.equal(invalid.trials[0]!.status, 'FAILED');
    assert.equal(invalid.trials[0]!.error, 'ambiguous_factor_request');
    assert.equal((await host.harness.snapshot(Exposures, context))!.windows.length, 3);

    faux.setResponses([tool('factor_evaluate', { hypothesis: 'Reuse the original private scope from another conversation after reopen', model }), tool('research_finish', { reason: 'Stop the bounded protocol.' })]);
    const fourth = await host.start('cached-scope-protocol', input);
    await (await host.harness.submission(fourth.submissionId, context))!.wait(context);
    assert.equal((await host.state(fourth.conversationId)).research!.trials[0]!.status, 'COMPUTED');
    assert.equal((await host.harness.snapshot(CatalogScopes, context))!.roots.length, 1);
    assert.equal((await host.harness.snapshot(Exposures, context))!.windows.length, 4);
    assert.deepEqual((await readFile(calls, 'utf8')).trim().split('\n'), ['catalog-factor-scope', 'scan-factor-catalog', 'scan-factor-catalog', 'evaluate-factor', 'scan-factor-catalog']);
  } finally {
    await host.close();
    if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous;
    await rm(directory, { recursive: true, force: true });
  }
});
