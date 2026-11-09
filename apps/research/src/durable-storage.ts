import type { Storage } from '@earendil-works/pi-durable';
import { SqliteStorage } from '@earendil-works/pi-durable/storage/sqlite';
import { openNodeSqliteDatabase } from '@earendil-works/pi-durable/storage/sqlite/node';
import { lstat, mkdir, realpath } from 'node:fs/promises';
import { basename, dirname, join, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { atomicWriteFile, syncDirectory, syncTree } from './durable-files.ts';

/** SQLite is only the OS-held publication mutex here; it contains no task,
 * heartbeat or lease state. Process death releases the lock automatically. */
export interface DurableOwner { path: string; close(): void }
export async function acquireDurableOwner(path: string): Promise<DurableOwner> {
  const absolute = resolve(path);
  const created = await mkdir(dirname(absolute), { recursive: true });
  if (created) { await syncTree(created); await syncDirectory(dirname(created)); }
  let canonical: string;
  try {
    canonical = await realpath(absolute);
    if ((await lstat(canonical)).nlink > 1) throw new Error('durable_database_hardlink_unsupported');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    canonical = join(await realpath(dirname(absolute)), basename(absolute));
  }
  const mutex = new DatabaseSync(`${canonical}.owner.sqlite`);
  try { mutex.exec('PRAGMA busy_timeout=0; BEGIN IMMEDIATE'); }
  catch (error) {
    mutex.close();
    if ((error as { errcode?: number }).errcode === 5 || /database is locked/.test(String(error))) throw new Error('durable_database_already_owned', { cause: error });
    throw error;
  }
  let closed = false;
  return { path: canonical, close() {
    if (closed) return;
    closed = true;
    try { if (mutex.isTransaction) mutex.exec('ROLLBACK'); } finally { mutex.close(); }
  } };
}

/** Explicit installed/maintenance callers must find the original ledger. This
 * check never initializes an absent, empty or unrelated SQLite file. */
export function assertOriginalDurableDatabase(path: string): void {
  const original = new DatabaseSync(path, { readOnly: true });
  try {
    if (!original.prepare('SELECT next_id FROM durable_metadata WHERE singleton = 1').get()) throw new Error('original_durable_metadata_unavailable');
  } finally { original.close(); }
}

/** Official storage, with power-loss-safe acknowledged commits and exactly one
 * live owner. Every production writer (including maintenance) uses this open. */
export async function openDurableStorage(path: string, options: { requireOriginal?: boolean; owner?: DurableOwner } = {}): Promise<Storage> {
  const owner = options.owner ?? await acquireDurableOwner(path);
  let database: Awaited<ReturnType<typeof openNodeSqliteDatabase>> | undefined;
  try {
    // This durable initialization fact is written before any admission. It has
    // no replacement installation/task identity: a missing original ledger on
    // subsequent startup is an error, rather than an implicit empty install.
    const initialized = `${owner.path}.initialized`;
    let previouslyOpened = false;
    try { previouslyOpened = (await lstat(initialized)).isFile(); }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
    if (options.requireOriginal || previouslyOpened) assertOriginalDurableDatabase(owner.path);
    database = await openNodeSqliteDatabase(owner.path);
    await database.exec('PRAGMA synchronous = FULL');
    const synchronous = await database.get<{ synchronous: number }>('PRAGMA synchronous');
    const journal = await database.get<{ journal_mode: string }>('PRAGMA journal_mode');
    if (synchronous?.synchronous !== 2 || journal?.journal_mode !== 'wal') throw new Error('durable_database_persistence_unavailable');
    const storage = await SqliteStorage.open(database);
    // Persist creation of a new ledger before admitting work into it.
    await syncDirectory(dirname(owner.path));
    if (!previouslyOpened) await atomicWriteFile(initialized, JSON.stringify({ schemaVersion: 1 }));
    const close = storage.close.bind(storage);
    let closing: Promise<void> | undefined;
    storage.close = context => closing ??= (async () => {
      try { await close(context); } finally { if (!options.owner) owner.close(); }
    })();
    return storage;
  } catch (error) {
    try { await database?.close(); } finally { if (!options.owner) owner.close(); }
    throw error;
  }
}
