import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { remoteSshArguments, remoteSshEnvironment } from './remote-ssh.ts';
import { CliError } from './profile.ts';

const execute = promisify(execFile);
const fields = ['nautilusSource', 'nautilusVersion', 'nautilusRevision', 'precision', 'jobVersion'] as const;
type Identity = Record<typeof fields[number], string>;
function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new CliError('remote_owner_configuration_invalid');
  return value as Record<string, unknown>;
}
/** Explicit trusted executable inspection, not account connection or source attestation. */
export function remoteOwnerInspection(value: unknown): { args: string[]; expected: Identity } {
  const input = record(value);
  if (Object.keys(input).some(key => !['host', 'user', 'port', 'knownHostsFile', 'ownerExecutable', 'expectedIdentity'].includes(key))) throw new CliError('remote_owner_configuration_invalid');
  if (typeof input.ownerExecutable !== 'string' || !/^\/[a-zA-Z0-9_./-]+$/.test(input.ownerExecutable)
    || input.ownerExecutable.split('/').includes('..')) throw new CliError('remote_owner_executable_invalid');
  const expected = record(input.expectedIdentity);
  if (Object.keys(expected).some(key => !fields.includes(key as typeof fields[number]))
    || fields.some(key => typeof expected[key] !== 'string' || !String(expected[key]).trim())
    || !['standard', 'high'].includes(String(expected.precision))) throw new CliError('remote_owner_identity_required');
  return { expected: expected as Identity, args: remoteSshArguments(input, `${input.ownerExecutable} capabilities`) };
}
export function inspectedRemoteOwner(value: unknown, expected: Identity): Record<string, unknown> {
  const response = record(value), identity = record(response.identity), account = record(response.account), research = record(response.research);
  if (response.schemaVersion !== 1 || response.protocol !== 'quazonai-native-process-v1' || response.running !== false
    || fields.some(key => identity[key] !== expected[key])
    || typeof account.compiled !== 'boolean' || typeof account.registration !== 'boolean' || account.registration && !account.compiled
    || account.command !== 'account-runtime' || account.requiresExplicitStart !== true
    || typeof research.singleStrategyRegistration !== 'boolean' || typeof research.configuredRunFactory !== 'boolean') throw new CliError('remote_owner_capabilities_mismatch');
  // Return a bounded allowlist, not arbitrary executable stdout or sensitive diagnostics.
  return { qualification: 'DECLARED_IDENTITY_MATCH_INSPECTION_ONLY', protocol: response.protocol, identity: expected,
    account: { compiled: account.compiled, registration: account.registration, requiresExplicitStart: true },
    research: { singleStrategyRegistration: research.singleStrategyRegistration, configuredRunFactory: research.configuredRunFactory },
    accountConnectionRequested: false, tradingStartRequested: false, sourceAttested: false, automaticRetry: false };
}
export async function inspectRemoteOwner(value: unknown, signal?: AbortSignal): Promise<Record<string, unknown>> {
  const plan = remoteOwnerInspection(value);
  if (signal?.aborted) throw new CliError('remote_owner_inspection_cancelled');
  let stdout: string;
  try {
    ({ stdout } = await execute('ssh', plan.args, { signal, timeout: 20000, maxBuffer: 1024 * 1024,
      env: remoteSshEnvironment() }));
  } catch { throw new CliError('remote_owner_inspection_failed_outcome_unknown'); }
  let result: unknown;
  try { result = JSON.parse(stdout); } catch { throw new CliError('remote_owner_capabilities_invalid'); }
  return inspectedRemoteOwner(result, plan.expected);
}
