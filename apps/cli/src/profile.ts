import { chmod, mkdir, open, readFile, rename, rm } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { homedir } from 'node:os';
import { randomUUID } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

export interface Profile {
  schema_version: 1; origin: string; token: string; development_http: boolean; ca_certificate: string | null;
}
export class CliError extends Error {
  code: string; status?: number;
  constructor(code: string, status?: number) { super(code); this.code = code; this.status = status; }
}
export function profilePath(env = process.env): string {
  const root = process.platform === 'win32' ? env.APPDATA : env.XDG_CONFIG_HOME ?? join(env.HOME ?? homedir(), '.config');
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
async function privatePath(path: string, directory: boolean) {
  if (process.platform === 'win32') {
    const script = `$ErrorActionPreference = 'Stop'; $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User; $directory = $env:QZ_PRIVATE_DIRECTORY -eq '1'; $acl = if ($directory) { New-Object Security.AccessControl.DirectorySecurity } else { New-Object Security.AccessControl.FileSecurity }; $acl.SetOwner($sid); $acl.SetAccessRuleProtection($true, $false); $inherit = if ($directory) { [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit' } else { [Security.AccessControl.InheritanceFlags]::None }; $rule = New-Object Security.AccessControl.FileSystemAccessRule($sid, 'FullControl', $inherit, 'None', 'Allow'); $acl.AddAccessRule($rule); Set-Acl -LiteralPath $env:QZ_PRIVATE_PATH -AclObject $acl`;
    await promisify(execFile)('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], { windowsHide: true, env: { ...process.env, QZ_PRIVATE_PATH: path, QZ_PRIVATE_DIRECTORY: directory ? '1' : '0' } });
  } else await chmod(path, directory ? 0o700 : 0o600);
}
export async function saveProfile(value: Profile, path = profilePath()): Promise<void> {
  const directory = dirname(path);
  await mkdir(directory, { recursive: true, mode: 0o700 });
  await privatePath(directory, true);
  const temporary = join(directory, `.client-${randomUUID()}.json`);
  try {
    const file = await open(temporary, 'wx', 0o600);
    try { await file.writeFile(`${JSON.stringify(value)}\n`); await file.sync(); } finally { await file.close(); }
    await privatePath(temporary, false);
    await rename(temporary, path);
  } finally { await rm(temporary, { force: true }); }
}
export async function forgetProfile(path = profilePath()) { await rm(path, { force: true }); }
