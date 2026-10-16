import { DatabaseSync } from 'node:sqlite';
import { chmod, mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import type { AuthOperationOptions, Credential, CredentialStore } from '@earendil-works/pi-ai';

/** App-owned private storage for the official provider's login/refresh protocol. */
export class ModelCredentials implements CredentialStore {
  private queue: Promise<unknown> = Promise.resolve();
  private database: DatabaseSync;
  private constructor(database: DatabaseSync) { this.database = database; }
  static async open(directory: string) {
    await mkdir(directory, { recursive: true, mode: 0o700 });
    await chmod(directory, 0o700);
    const path = join(directory, 'credentials.sqlite');
    const database = new DatabaseSync(path);
    try {
      await chmod(path, 0o600);
      database.exec('CREATE TABLE IF NOT EXISTS credentials (provider TEXT PRIMARY KEY, value TEXT NOT NULL)');
      return new ModelCredentials(database);
    } catch (error) { database.close(); throw error; }
  }
  private serial<T>(operation: () => T | Promise<T>, options?: AuthOperationOptions): Promise<T> {
    const begin = () => { options?.signal?.throwIfAborted(); return operation(); };
    const result = this.queue.then(begin, begin);
    this.queue = result.then(() => {}, () => {});
    return result;
  }
  private credential(provider: string): Credential | undefined {
    const row = this.database.prepare('SELECT value FROM credentials WHERE provider=?').get(provider);
    return row ? JSON.parse(String(row.value)) as Credential : undefined;
  }
  read(provider: string, options?: AuthOperationOptions) { return this.serial(() => this.credential(provider), options); }
  list(options?: AuthOperationOptions) {
    return this.serial(() => this.database.prepare('SELECT provider,value FROM credentials').all().map(row => ({
      providerId: String(row.provider), type: (JSON.parse(String(row.value)) as Credential).type,
    })), options);
  }
  modify(provider: string, change: (current: Credential | undefined) => Promise<Credential | undefined>, options?: AuthOperationOptions) {
    return this.serial(async () => {
      this.database.exec('BEGIN IMMEDIATE');
      try {
        const previous = this.credential(provider);
        const next = await change(previous);
        if (next !== undefined) this.database.prepare('INSERT INTO credentials(provider,value) VALUES(?,?) ON CONFLICT(provider) DO UPDATE SET value=excluded.value').run(provider, JSON.stringify(next));
        this.database.exec('COMMIT');
        return next ?? previous;
      } catch (error) { this.database.exec('ROLLBACK'); throw error; }
    }, options);
  }
  delete(provider: string, options?: AuthOperationOptions) {
    return this.serial(() => { this.database.prepare('DELETE FROM credentials WHERE provider=?').run(provider); }, options);
  }
  close() { return this.serial(() => { this.database.close(); }); }
}
