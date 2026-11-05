import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { JsonObject } from '@earendil-works/pi-durable';
import { executeNative } from '../../research/plugins/native-portfolio.ts';
import { object } from '../src/facts.ts';
import { verifyOriginalPrefix } from '../src/pm-science.ts';

const binary = process.env.QZ_JOB_BINARY;
const fixturePath = process.env.QZ_PM_NATIVE_FIXTURE ?? process.env.QZ_BROWSER_FIXTURE;
// Real native Parquet/digest acceptance. The synthetic catalog proves protocol
// compatibility and source identity, not market provenance or performance.
test('PM original prefix accepts a native default-zero row quota and retains complete scientific identity',
  { skip: !binary || !fixturePath, timeout: 30_000 }, async () => {
    const fixture = object(JSON.parse(await readFile(fixturePath!, 'utf8')));
    const catalog = String(fixture.catalog_root ?? fixture.catalog);
    const originalSelection = fixture.catalog_root ? object(object(fixture.selection).selection)
      : object(object(object(fixture.request).forecast).selection);
    const { maximum_rows: _legacyRows, ...selection } = originalSelection;
    const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-pm-prefix-native-'));
    try {
      const expected = await executeNative(binary!, 'catalog-prefix-digest', selection, ['--catalog', catalog], directory, BACKGROUND_CONTEXT);
      assert.equal(object(expected.selection).maximum_rows, 0, 'Rust serde echoes its default for an omitted row quota');
      assert.match(String(expected.content_sha256), /^[a-f0-9]{64}$/);
      assert.ok(BigInt(String(expected.row_count)) > 1n, 'The real catalog must contain multiple original rows');
      const source: JsonObject = { request: { forecast: { selection, parameters: { total_fuel: '1' } } }, catalogPrefix: expected };
      const frozen = structuredClone(source);
      assert.deepEqual(await verifyOriginalPrefix(source, catalog, binary!, directory, BACKGROUND_CONTEXT), expected);
      assert.deepEqual(source, frozen, 'The original request, historical fuel and prefix receipt remain immutable');

      const legacySelection = { ...selection, maximum_rows: 1 };
      const legacyPrefix = await executeNative(binary!, 'catalog-prefix-digest', legacySelection, ['--catalog', catalog], directory, BACKGROUND_CONTEXT);
      assert.equal(legacyPrefix.row_count, expected.row_count, 'A historical quota cannot truncate the original prefix');
      assert.equal(legacyPrefix.content_sha256, expected.content_sha256);
      const legacySource: JsonObject = { request: { forecast: { selection: legacySelection, parameters: { total_fuel: '1000000' } } }, catalogPrefix: legacyPrefix };
      const legacyFrozen = structuredClone(legacySource);
      assert.deepEqual(await verifyOriginalPrefix(legacySource, catalog, binary!, directory, BACKGROUND_CONTEXT), legacyPrefix);
      assert.deepEqual(legacySource, legacyFrozen);

      const changedHash = structuredClone(source);
      const prefix = object(changedHash.catalogPrefix);
      prefix.content_sha256 = `${String(prefix.content_sha256)[0] === '0' ? '1' : '0'}${String(prefix.content_sha256).slice(1)}`;
      await assert.rejects(verifyOriginalPrefix(changedHash, catalog, binary!, directory, BACKGROUND_CONTEXT), /pm_original_catalog_prefix_changed/);
      const changedSeries = structuredClone(source);
      object((object(changedSeries.catalogPrefix).series as JsonObject[])[0]).content_sha256 = '0'.repeat(64);
      await assert.rejects(verifyOriginalPrefix(changedSeries, catalog, binary!, directory, BACKGROUND_CONTEXT), /pm_original_catalog_prefix_changed/);
      const changedTime = structuredClone(source);
      object(object(changedTime.catalogPrefix).selection).decision_cutoff_ns = (BigInt(String(selection.decision_cutoff_ns)) + 1n).toString();
      await assert.rejects(verifyOriginalPrefix(changedTime, catalog, binary!, directory, BACKGROUND_CONTEXT), /pm_original_catalog_prefix_evidence_invalid/);
      const changedMarket = structuredClone(source);
      object(object(changedMarket.catalogPrefix).selection).bar_types = ['OTHER.SIM-1-MINUTE-LAST-EXTERNAL'];
      await assert.rejects(verifyOriginalPrefix(changedMarket, catalog, binary!, directory, BACKGROUND_CONTEXT), /pm_original_catalog_prefix_evidence_invalid/);
    } finally { await rm(directory, { recursive: true, force: true }); }
  });
