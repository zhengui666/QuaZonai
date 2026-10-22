import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import fs, { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { syncBuiltinESMExports } from 'node:module';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { test } from 'node:test';
import { datasetRevisionManifest, prepareDatasetRevision } from '../src/dataset-revision-publish.ts';
import { readSourceRevision } from '../src/pm-source.ts';
import type { DatasetConfig } from '../src/config.ts';

// Filesystem publication protocol only; bytes are not scientific/native results.
async function fixture() {
  const directory = await mkdtemp(join(tmpdir(), 'qz-revision-publish-')); const catalog = join(directory, 'producer-catalog'); await mkdir(catalog);
  const file = join(catalog, 'source.parquet'); await writeFile(file, 'initial controlled complete file');
  const dataset: DatasetConfig = { id: 'producer', name: 'Publication protocol', assetClass: 'TEST', currency: 'USD', source: 'controlled producer',
    ready: true, start: null, end: null, data: { catalog, request: { frozen: true } }, revisionManifest: datasetRevisionManifest(join(directory, 'dataset.json'), 'producer') };
  return { directory, file, dataset, cleanup: () => rm(directory, { recursive: true, force: true }) };
}

test('concurrent prepared publications compare the original head and cannot silently discard a newer completed revision', async () => {
  const f = await fixture();
  try {
    await (await prepareDatasetRevision(f.dataset, { source: 'same original producer' })).publish();
    const original = (await readSourceRevision(f.dataset))!;
    const unchanged = await prepareDatasetRevision(f.dataset, { source: 'same original producer' });
    await writeFile(f.file, 'candidate A'); const a = await prepareDatasetRevision(f.dataset, { source: 'same original producer' });
    await writeFile(f.file, 'candidate B'); const b = await prepareDatasetRevision(f.dataset, { source: 'same original producer' });
    await b.publish(); const head = await readFile(f.dataset.revisionManifest!);
    await assert.rejects(a.publish(), /dataset_revision_publication_conflict/);
    await assert.rejects(unchanged.publish(), /dataset_revision_publication_conflict/);
    assert.deepEqual(await readFile(f.dataset.revisionManifest!), head);
    assert.equal(await readFile(join(String(original.configured.data.catalog), 'source.parquet'), 'utf8'), 'initial controlled complete file');
    await writeFile(f.file, 'candidate A'); const rebased = await prepareDatasetRevision(f.dataset, { source: 'same original producer' });
    await rebased.publish(); assert.notEqual(rebased.sourceRevision, a.sourceRevision, 'retry prepares against the actual predecessor');
    const restored = await readSourceRevision(f.dataset); assert.equal(await readFile(join(String(restored!.configured.data.catalog), 'source.parquet'), 'utf8'), 'candidate A');
  } finally { await f.cleanup(); }
});

test('an interrupted publication mutex releases with its original process without a stale lease or a replacement task', { timeout: 10_000 }, async () => {
  const f = await fixture(); let child: ReturnType<typeof spawn> | null = null;
  try {
    const prepared = await prepareDatasetRevision(f.dataset, { source: 'original producer' });
    const mutex = join(dirname(f.dataset.revisionManifest!), 'publication-lock.sqlite');
    child = spawn(process.execPath, ['--input-type=module', '-e', `import {DatabaseSync} from 'node:sqlite'; const db=new DatabaseSync(${JSON.stringify(mutex)}); db.exec('BEGIN IMMEDIATE'); process.stdout.write('locked'); setInterval(()=>{},1000);`], { stdio: ['ignore', 'pipe', 'pipe'] });
    await once(child.stdout!, 'data');
    await assert.rejects(prepared.publish(), /locked|busy/i);
    const closed = once(child, 'exit'); child.kill('SIGKILL'); await closed; child = null;
    const published = await prepared.publish(); assert.equal(published.sourceRevision, prepared.sourceRevision);
  } finally { child?.kill('SIGKILL'); await f.cleanup(); }
});


test('completed publication replays its exact original receipt and cannot overwrite a later head', async () => {
  const f = await fixture();
  try {
    const prepared = await prepareDatasetRevision(f.dataset, { source: 'original producer' });
    const first = await prepared.publish(); const bytes = await readFile(f.dataset.revisionManifest!);
    assert.deepEqual(await prepared.publish(), first);
    assert.deepEqual(await readFile(f.dataset.revisionManifest!), bytes, 'replay preserves original publication clock and receipt');
    await writeFile(f.file, 'new completed source');
    await (await prepareDatasetRevision(f.dataset, { source: 'original producer' })).publish();
    await assert.rejects(prepared.publish(), /dataset_revision_publication_conflict/);
  } finally { await f.cleanup(); }
});

test('retry after an injected alias I/O failure publishes the same complete history and alias receipt', async () => {
  const f = await fixture(); const originalRename = fs.rename;
  try {
    const prepared = await prepareDatasetRevision(f.dataset, { source: 'original producer' });
    const history = join(dirname(f.dataset.revisionManifest!), 'revisions', prepared.sourceRevision.slice('sha256:'.length), 'manifest.json');
    // Only the filesystem's final rename fails. Snapshot files, the complete
    // history receipt, validation and process mutex remain actual filesystem work.
    fs.rename = async (from, to) => { if (to === f.dataset.revisionManifest) throw Object.assign(new Error('controlled_alias_io_failure'), { code: 'EIO' }); return originalRename(from, to); };
    syncBuiltinESMExports();
    await assert.rejects(prepared.publish(), /controlled_alias_io_failure/);
    const unpublished = JSON.parse(await readFile(history, 'utf8'));
    await assert.rejects(readFile(f.dataset.revisionManifest!), /ENOENT/);
    fs.rename = originalRename; syncBuiltinESMExports();
    const published = await prepared.publish();
    assert.deepEqual(await readFile(history), await readFile(f.dataset.revisionManifest!));
    assert.ok(BigInt(published.availableAtNs) >= BigInt(unpublished.availableAtNs), 'unpublished retry is finalized at its real retry clock');
    assert.equal(published.sourceRevision, unpublished.sourceRevision, 'original immutable source bytes retain their identity');
    assert.deepEqual(await prepared.publish(), published);
  } finally { fs.rename = originalRename; syncBuiltinESMExports(); await f.cleanup(); }
});


test('missing published source bytes cannot be treated as an absent manifest and reset the revision chain', async () => {
  const f = await fixture();
  try {
    const first = await (await prepareDatasetRevision(f.dataset, { source: 'original producer' })).publish();
    const alias = await readFile(f.dataset.revisionManifest!);
    await rm(join(String(first.configured.data.catalog), 'source.parquet'));
    await writeFile(f.file, 'next source');
    await assert.rejects(prepareDatasetRevision(f.dataset, { source: 'original producer' }), /ENOENT/);
    assert.deepEqual(await readFile(f.dataset.revisionManifest!), alias);
  } finally { await f.cleanup(); }
});
