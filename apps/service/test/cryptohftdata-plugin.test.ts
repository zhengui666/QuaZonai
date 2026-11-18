import assert from 'node:assert/strict';
import { test } from 'node:test';
import { DataSourceState } from '../src/data-sources.ts';
import { setup, context } from './support.ts';

test('retired CryptoHFTData records retain unprepared archive facts without loading the archive', async () => {
  const state = await setup();
  try {
    const archive = { id: 'original-chd-archive', name: 'Original CHD archive', source: 'CryptoHFTData', assetClass: 'CRYPTO', currency: 'MULTI',
      start: '2026-02-12T00:00:00.000Z', end: '2026-10-14T00:00:00.000Z', ready: false,
      preparation: { archiveLoaded: true, nativePrepared: false, scientificQualification: 'UNVERIFIED' },
      data: { archiveDirectory: '/unavailable/archive', acquisition: '/unavailable/archive/dataset.json', streams: ['perpetual trades', 'spot orderbook'] },
      inputCoverage: { objects: 3368701, bytes: 279118890859 } };
    const original = { enabled: true, config: { archiveDirectory: '/unavailable/archive' }, datasets: [archive], retainedDatasets: [archive], revision: 4,
      lastLoadedAt: '2026-10-15T00:00:00.000Z', error: null };
    await state.service.research.harness.commit(async tx => { (await tx.doc(DataSourceState)).sources['qz.data.cryptohftdata'] = original; }, context);
    await state.restart();
    const saved = (await state.service.research.harness.snapshot(DataSourceState, context))!.sources['qz.data.cryptohftdata'];
    assert.deepEqual(saved, original);
    const restored = state.service.config.datasets.find(row => row.id === archive.id)!;
    assert.equal(restored.ready, false); assert.equal(restored.nativeModality, undefined); assert.equal(restored.data.catalog, undefined);
    assert.deepEqual(restored.preparation, archive.preparation);
    assert.deepEqual(restored.inputCoverage, archive.inputCoverage);
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/data-sources/qz.data.cryptohftdata/refresh', headers: state.headers });
    assert.equal(response.statusCode, 410); assert.match(response.body, /data_source_plugins_retired/);
    assert.deepEqual((await state.service.research.harness.snapshot(DataSourceState, context))!.sources['qz.data.cryptohftdata'], original);
  } finally { await state.cleanup(); }
});
