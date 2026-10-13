import { chmod, mkdir, open, readFile, rename, rm } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { homedir } from 'node:os';
import { randomUUID } from 'node:crypto';

export interface Profile {
  schema_version: 1; origin: string; token: string; development_http: boolean; ca_certificate: string | null;
}
export class CliError extends Error {
  code: string; status?: number;
  constructor(code: string, status?: number) { super(code); this.code = code; this.status = status; }
}
export function profilePath(env = process.env): string {
  const root = env.XDG_CONFIG_HOME ?? join(env.HOME ?? homedir(), '.config');
  if (!root) throw new CliError('client_config_location_missing');
  return join(root, 'quazonai', 'client.json');
}
export function origin(value: string): string {
  const url = new URL(value);
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.pathname !== '/' || url.search || url.hash) throw new CliError('invalid_origin');
  return url.origin;
}
export async function loadProfile(path = profilePath()): Promise<Profile> {
  let value: Profile;
  try { value = JSON.parse(await readFile(path, 'utf8')) as Profile; }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') throw new CliError('login_required');
    throw new CliError('client_profile_invalid');
  }
  if (value.schema_version !== 1 || typeof value.origin !== 'string' || typeof value.token !== 'string' || !value.token || typeof value.development_http !== 'boolean' || (value.ca_certificate !== null && typeof value.ca_certificate !== 'string')) throw new CliError('client_profile_invalid');
  origin(value.origin);
  return value; // qzc and new service tokens remain opaque, never rewritten.
}
export async function saveProfile(value: Profile, path = profilePath()): Promise<void> {
  const directory = dirname(path);
  await mkdir(directory, { recursive: true, mode: 0o700 });
  await chmod(directory, 0o700);
  const temporary = join(directory, `.client-${randomUUID()}.json`);
  try {
    const file = await open(temporary, 'wx', 0o600);
    try { await file.writeFile(`${JSON.stringify(value)}\n`); await file.sync(); } finally { await file.close(); }
    await chmod(temporary, 0o600);
    await rename(temporary, path);
  } finally { await rm(temporary, { force: true }); }
}
export async function forgetProfile(path = profilePath()) { await rm(path, { force: true }); }
