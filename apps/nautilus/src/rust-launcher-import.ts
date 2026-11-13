import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { constants } from 'node:fs';
import { chmod, copyFile, mkdir, realpath, stat, writeFile } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join } from 'node:path';

const execute = promisify(execFile);
export type ImportedLauncherIdentity = {
  nautilusSource: string; nautilusVersion: string; precision: 'standard' | 'high';
};
export type ImportedLauncherCapabilities = {
  schemaVersion: 1; protocol: 'quazonai-native-process-v1';
  identity: ImportedLauncherIdentity & { nautilusRevision?: string; jobVersion: string };
  research: { singleStrategyRegistration: boolean; configuredRunFactory: boolean };
  account: { compiled: boolean; registration: boolean; command: 'account-runtime'; requiresExplicitStart: true };
  running: false;
};

function capabilities(value: unknown, expected?: ImportedLauncherIdentity): ImportedLauncherCapabilities {
  const v = value as ImportedLauncherCapabilities | null;
  if (!v || v.schemaVersion !== 1 || v.protocol !== 'quazonai-native-process-v1' || v.running !== false
    || !v.identity || !['nautilusSource', 'nautilusVersion', 'jobVersion'].every(key => typeof (v.identity as Record<string, unknown>)[key] === 'string' && String((v.identity as Record<string, unknown>)[key]).trim())
    || (v.identity.nautilusRevision !== undefined && typeof v.identity.nautilusRevision !== 'string')
    || !['standard', 'high'].includes(v.identity.precision)
    || typeof v.research?.singleStrategyRegistration !== 'boolean' || typeof v.research?.configuredRunFactory !== 'boolean'
    || typeof v.account?.compiled !== 'boolean' || typeof v.account?.registration !== 'boolean'
    || v.account.command !== 'account-runtime' || v.account.requiresExplicitStart !== true
    || (v.account.registration && !v.account.compiled)) throw new Error('rust_import_capabilities_invalid');
  if (expected && (v.identity.nautilusSource !== expected.nautilusSource || v.identity.nautilusVersion !== expected.nautilusVersion || v.identity.precision !== expected.precision)) throw new Error('rust_import_identity_mismatch');
  return v;
}

/** Explicit trusted-code execution, not a safe binary scanner: even `capabilities`
 * can execute arbitrary code in an owner-supplied executable. Never call on save.
 * Checks execute in this application's runtime, not on the original build host.
 * Identity is the executable's declaration, not independent source attestation.
 * No selection, account binding, BOOT, or retry is performed by this helper.
 */
export async function importRustLauncher(options: {
  sourceExecutable: string; directory: string; expectedIdentity?: ImportedLauncherIdentity; signal?: AbortSignal;
}): Promise<{ executable: string; capabilities: ImportedLauncherCapabilities; qualification: 'IMPORTED_NOT_CONNECTED' }> {
  if (!isAbsolute(options.sourceExecutable) || !isAbsolute(options.directory)) throw new Error('rust_import_absolute_paths_required');
  if (options.signal?.aborted) throw new Error('rust_import_cancelled_before_execution');
  const source = await realpath(options.sourceExecutable);
  if (!(await stat(source)).isFile()) throw new Error('rust_import_regular_file_required');
  const directory = join(await realpath(dirname(options.directory)), basename(options.directory));
  // Exclusive creation: an earlier/uncertain attempt and its logs are never overwritten.
  await mkdir(directory, { mode: 0o700 });
  const executable = join(directory, process.platform === 'win32' ? 'qz-owner-launcher.exe' : 'qz-owner-launcher');
  await copyFile(source, executable, constants.COPYFILE_EXCL);
  await chmod(executable, 0o700);
  let stdout = ''; let stderr = '';
  try {
    const output = await execute(executable, ['capabilities'], { cwd: directory, env: { PATH: process.env.PATH, TMPDIR: directory }, signal: options.signal, maxBuffer: 1024 * 1024 });
    stdout = output.stdout; stderr = output.stderr;
  } catch (error) {
    const failure = error as Error & { stdout?: string; stderr?: string };
    stdout = failure.stdout ?? ''; stderr = failure.stderr ?? '';
    await writeFile(join(directory, 'capabilities.stdout'), stdout, { flag: 'wx', mode: 0o600 });
    await writeFile(join(directory, 'capabilities.stderr'), `${stderr}\n${failure.message}`, { flag: 'wx', mode: 0o600 });
    // Killing the inspected process does not prove arbitrary descendants stopped.
    if (options.signal?.aborted) throw new Error('rust_import_interrupted_outcome_unknown', { cause: error });
    throw error;
  }
  await writeFile(join(directory, 'capabilities.stdout'), stdout, { flag: 'wx', mode: 0o600 });
  await writeFile(join(directory, 'capabilities.stderr'), stderr, { flag: 'wx', mode: 0o600 });
  return { executable, capabilities: capabilities(JSON.parse(stdout), options.expectedIdentity), qualification: 'IMPORTED_NOT_CONNECTED' };
}
