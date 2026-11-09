import { randomUUID } from 'node:crypto';
import { lstat, mkdir, open, readdir, rename, rm } from 'node:fs/promises';
import { dirname, join } from 'node:path';

/** A successful publication includes the directory entry, not only file data. */
export async function syncDirectory(path: string): Promise<void> {
  const directory = await open(path, 'r');
  try { await directory.sync(); } finally { await directory.close(); }
}

/** Persist a complete private tree before publishing the manifest that names it.
 * Symlinks have no file contents to flush; their entries are covered by their
 * parent directory. Special files are not durable scientific artifacts. */
export async function syncTree(path: string): Promise<void> {
  const stat = await lstat(path);
  if (stat.isSymbolicLink()) return;
  if (stat.isDirectory()) {
    for (const name of await readdir(path)) await syncTree(join(path, name));
    await syncDirectory(path);
  } else if (stat.isFile()) {
    const file = await open(path, 'r');
    try { await file.sync(); } finally { await file.close(); }
  } else throw new Error('durable_regular_file_required');
}

export type DurableFileData = string | Uint8Array | Iterable<string | Uint8Array> | AsyncIterable<string | Uint8Array>;

/** Replace a file only after all chunks have been written and flushed. Errors
 * before rename leave the previous version intact. Errors after rename remain
 * errors: callers must not acknowledge a publication without directory fsync. */
export async function atomicWriteFile(path: string, data: DurableFileData, mode = 0o600): Promise<void> {
  const parent = dirname(path);
  const created = await mkdir(parent, { recursive: true });
  if (created) {
    await syncTree(created);
    await syncDirectory(dirname(created));
  }
  const temporary = join(parent, `.qz-write-${randomUUID()}.tmp`);
  let published = false;
  try {
    const file = await open(temporary, 'wx', mode);
    try {
      if (typeof data === 'string' || data instanceof Uint8Array) await file.writeFile(data);
      else for await (const chunk of data) await file.writeFile(chunk);
      await file.sync();
    } finally { await file.close(); }
    await rename(temporary, path);
    published = true;
    await syncDirectory(parent);
  } finally {
    if (!published) await rm(temporary, { force: true });
  }
}
