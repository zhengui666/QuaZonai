/** Runs inside the scientific OCI container. Its process and the entire child
 * tree belong to the same CRIU image. No HTTP or host-side pipe is involved. */
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { access, open, readFile, rename } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { setTimeout } from 'node:timers/promises';
import { pathToFileURL } from 'node:url';

export interface CompletionConfiguration {
  schemaVersion: 1;
  requestId: string;
  implementationDigest: string;
  inputDigest: string;
  binary: string;
  args: string[];
  environment: Record<string, string>;
}
export interface CompletionRecord {
  schemaVersion: 1;
  requestId: string;
  implementationDigest: string;
  inputDigest: string;
  exitCode: number | null;
  signal: string | null;
  stdout: { bytes: string; sha256: string };
  stderr: { bytes: string; sha256: string };
}

export async function fileDigest(path: string): Promise<{ bytes: string; sha256: string }> {
  const hash = createHash('sha256'); let bytes = 0n;
  for await (const chunk of createReadStream(path)) { bytes += BigInt(chunk.length); hash.update(chunk); }
  return { bytes: String(bytes), sha256: hash.digest('hex') };
}

export async function runCompletionWrapper(work = '/work'): Promise<void> {
  const config = JSON.parse(await readFile(`${work}/execution.json`, 'utf8')) as CompletionConfiguration;
  if (config.schemaVersion !== 1 || !/^[a-f0-9]{64}$/.test(config.requestId)
    || !config.binary.startsWith('/') || !Array.isArray(config.args)) throw new Error('completion_configuration_invalid');
  const ready = await open(`${work}/wrapper.ready`, 'w', 0o600);
  try { await ready.writeFile(config.requestId); await ready.sync(); } finally { await ready.close(); }
  // The initial checkpoint is taken while this wrapper is at this barrier.
  // The controller publishes start only after generation zero is committed.
  for (;;) {
    try { await access(`${work}/start`); break; }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
    await setTimeout(50);
  }
  const stdin = await open(`${work}/request.json`, 'r');
  const stdout = await open(`${work}/stdout.partial`, 'w', 0o600);
  const stderr = await open(`${work}/stderr.log`, 'w', 0o600);
  let exitCode: number | null; let signal: string | null;
  try {
    const child = spawn(config.binary, config.args, { cwd: work, env: config.environment,
      stdio: [stdin.fd, stdout.fd, stderr.fd] });
    const result = await new Promise<{ code: number | null; signal: string | null }>((resolve, reject) => {
      child.once('error', reject);
      child.once('close', (code, signal) => resolve({ code, signal }));
    });
    exitCode = result.code; signal = result.signal;
    await stdout.sync(); await stderr.sync();
  } finally { await Promise.all([stdin.close(), stdout.close(), stderr.close()]); }
  const record: CompletionRecord = { schemaVersion: 1, requestId: config.requestId,
    implementationDigest: config.implementationDigest, inputDigest: config.inputDigest, exitCode, signal,
    stdout: await fileDigest(`${work}/stdout.partial`), stderr: await fileDigest(`${work}/stderr.log`) };
  const pending = await open(`${work}/exit.next.json`, 'w', 0o600);
  try { await pending.writeFile(JSON.stringify(record)); await pending.sync(); } finally { await pending.close(); }
  await rename(`${work}/exit.next.json`, `${work}/exit.json`);
  const directory = await open(work, 'r');
  try { await directory.sync(); } finally { await directory.close(); }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await runCompletionWrapper(process.argv[2]);
}
