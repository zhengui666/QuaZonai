/** Required host probe, deliberately outside *.test.ts. A successful unit test
 * or skipped native suite must never produce a verified recovery profile. */
import { execFile } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { release, arch, platform } from 'node:os';
import { pathToFileURL } from 'node:url';

export interface RecoveryProbe {
  binary: string;
  args: string[];
  exitCode: number | null;
  stdout: string;
  stderr: string;
  error?: string;
}
export interface RecoveryPreflight {
  schemaVersion: 1;
  observedAt: string;
  outcome: 'PREFLIGHT_ONLY' | 'BLOCKED';
  kernel: string;
  architecture: string;
  verifiedCommands: string[];
  probes: RecoveryProbe[];
  errors: string[];
}

function probe(binary: string, args: string[]): Promise<RecoveryProbe> {
  return new Promise(resolveProbe => {
    execFile(binary, args, { timeout: 60_000, maxBuffer: 1024 * 1024 }, (error, stdout, stderr) => {
      const code = (error as (Error & { code?: number | string }) | null)?.code;
      resolveProbe({ binary, args, exitCode: !error ? 0 : typeof code === 'number' ? code : null,
        stdout, stderr, ...(error && typeof code !== 'number' ? { error: error.message } : {}) });
    });
  });
}

/** These are necessary host checks, not scientific acceptance. Even on success
 * verifiedCommands stays empty until actual in-progress native restore tests. */
export async function recoveryPreflight(options: { runc: string; criu: string }): Promise<RecoveryPreflight> {
  const probes = await Promise.all([
    probe(options.runc, ['--version']),
    probe(options.criu, ['--version']),
    probe(options.criu, ['check', '--extra']),
  ]);
  const errors: string[] = [];
  if (platform() !== 'linux') errors.push('Linux is required for runC/CRIU.');
  if (probes[0]!.exitCode !== 0 || !/^runc version 1\.5\.2\s/m.test(probes[0]!.stdout)) errors.push('Official runC 1.5.2 is required.');
  if (probes[1]!.exitCode !== 0 || !/^Version: 4\.3\s*$/m.test(probes[1]!.stdout)) errors.push('Official CRIU 4.3 is required.');
  if (probes[2]!.exitCode !== 0) errors.push('CRIU kernel/capability probe failed; internal resume must remain disabled.');
  return { schemaVersion: 1, observedAt: new Date().toISOString(), outcome: errors.length ? 'BLOCKED' : 'PREFLIGHT_ONLY',
    kernel: release(), architecture: arch(), verifiedCommands: [], probes, errors };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = process.argv.slice(2);
  const outputIndex = args.indexOf('--output');
  if (outputIndex < 0 || !args[outputIndex + 1] || args.length !== 2) throw new Error('Usage: recovery-native-preflight.ts --output REPORT.json');
  const report = await recoveryPreflight({ runc: process.env.QZ_RUNC_BINARY ?? '/usr/local/sbin/runc',
    criu: process.env.QZ_CRIU_BINARY ?? '/usr/sbin/criu' });
  const output = resolve(args[outputIndex + 1]!);
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
  process.stdout.write(`${report.outcome}: ${output}\n`);
  if (report.outcome === 'BLOCKED') process.exitCode = 1;
}
