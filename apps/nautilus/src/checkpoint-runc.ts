import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { access, lstat, mkdir, readFile, readdir, readlink, realpath, rename, rm } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { release } from 'node:os';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { atomicWriteFile, syncDirectory, syncTree } from '../../research/src/durable-files.ts';
import { canonicalJsonDigest, JsonReader, jsonChunks } from '../../research/src/native-json.ts';
import { isTickNativeCommand } from '../../../packages/contracts/src/nautilus.ts';
import type { NativeExecutionRequest, NativeJson } from '../../../packages/contracts/src/nautilus.ts';
import { fileDigest } from './completion-wrapper.ts';
import type { CompletionConfiguration, CompletionRecord } from './completion-wrapper.ts';
import type { CheckpointInspection, CheckpointManifest, CheckpointProfile, RecoveryDriver } from './checkpoint-driver.ts';

const exec = promisify(execFile);
const requestPattern = /^[a-f0-9]{64}$/;
type Request = Omit<NativeExecutionRequest, 'schemaVersion'> & { schemaVersion: 1 | 2; recoveryProfileId?: string; instanceId?: string };
type Artifact = { path: string; bytes: string; sha256: string; mode: number; uid: number; gid: number; linkTo?: string };
interface Manifest extends CheckpointManifest { schemaVersion: 1; implementationDigest: string; files: Artifact[] }

export interface VerifiedCheckpointProfile extends CheckpointProfile {
  schemaVersion: 1;
  runtime: { runc: '1.5.2'; criu: '4.3'; kernel: string; cpuDigest: string };
  /** Immutable rootfs identity established by the installed acceptance export. */
  rootfsDigest: string;
  /** Actual bytes are checked before admission and restore. A configured label
   * or a read-only bind alone is not evidence of immutable input contents. */
  frozenInputs: { root: string; contentDigest: string }[];
  evidence: { command: string; reportDigest: string }[];
}
export interface RuncRecoveryOptions {
  dataRoot: string;
  rootfs: string;
  rootfsDigest: string;
  implementationDigest: string;
  catalogRoots: string[];
  profilePath: string;
  runc?: string;
  criu?: string;
  flock?: string;
  nodeBinary?: string;
  binary?: string;
  tickBinary?: string;
}

function validName(name: string): boolean {
  return /^[a-zA-Z0-9_-][a-zA-Z0-9_./-]*$/.test(name) && name.split('/').every(part => part && part !== '.' && part !== '..');
}
function below(root: string, path: string): boolean {
  const suffix = relative(root, path); return suffix === '' || !isAbsolute(suffix) && suffix !== '..' && !suffix.startsWith(`..${sep}`);
}
function missing(error: unknown): boolean { return (error as NodeJS.ErrnoException).code === 'ENOENT'; }
async function exists(path: string): Promise<boolean> {
  try { await access(path); return true; } catch (error) { if (missing(error)) return false; throw error; }
}
async function json<T>(path: string): Promise<T> { return JSON.parse(await readFile(path, 'utf8')) as T; }
export async function checkpointCpuDigest(): Promise<string> {
  const text = await readFile('/proc/cpuinfo', 'utf8');
  const description = text.split('\n').filter(line => /^(vendor_id|cpu family|model\s*:|model name|stepping|flags|Features)\s*:/.test(line));
  return createHash('sha256').update([...new Set(description)].sort().join('\n')).digest('hex');
}
export async function checkpointTreeDigest(directory: string, options: { allowSymlinks?: boolean } = {}): Promise<string> {
  const links = new Map<string, string>();
  async function walk(root: string, prefix = ''): Promise<unknown[]> {
    const result: unknown[] = [];
    for (const name of (await readdir(root)).sort()) {
      const path = join(root, name); const key = prefix ? `${prefix}/${name}` : name; const stat = await lstat(path);
      const ownership = { mode: stat.mode & 0o7777, uid: stat.uid, gid: stat.gid };
      if (stat.isDirectory()) { result.push({ path: key, type: 'directory', ...ownership }); result.push(...await walk(path, key)); }
      else if (stat.isSymbolicLink()) {
        if (options.allowSymlinks === false) throw new Error('checkpoint_frozen_symlink_unsupported');
        result.push({ path: key, type: 'symlink', ...ownership, target: await readlink(path) });
      } else if (stat.isFile()) {
        const inode = `${stat.dev}:${stat.ino}`; const prior = links.get(inode); if (!prior) links.set(inode, key);
        result.push({ path: key, type: 'file', ...ownership, ...await fileDigest(path), ...(prior ? { linkTo: prior } : {}) });
      }
      else throw new Error('checkpoint_frozen_special_file_unsupported');
    }
    return result;
  }
  return canonicalJsonDigest(await walk(directory) as never);
}
async function artifacts(directory: string, prefix = '', links = new Map<string, string>()): Promise<Artifact[]> {
  const result: Artifact[] = [];
  for (const name of (await readdir(directory)).sort()) {
    // A prior attempt may have durably written this descriptor before its
    // generation-directory rename. It must never hash its own previous bytes.
    if (!prefix && name === 'manifest.json') continue;
    const path = join(directory, name); const stat = await lstat(path); const key = prefix ? `${prefix}/${name}` : name;
    if (stat.isDirectory()) result.push(...await artifacts(path, key, links));
    else if (stat.isFile()) {
      const inode = `${stat.dev}:${stat.ino}`; const prior = links.get(inode); if (!prior) links.set(inode, key);
      result.push({ path: key, ...await fileDigest(path), mode: stat.mode & 0o7777, uid: stat.uid, gid: stat.gid, ...(prior ? { linkTo: prior } : {}) });
    }
    else if (stat.isSymbolicLink()) throw new Error('checkpoint_work_symlinks_unsupported');
    else throw new Error('checkpoint_special_file_unsupported');
  }
  return result;
}

/** Host-side adapter for the official runC/CRIU APIs. It performs one requested
 * operation and returns; all recovery decisions and timers belong to Durable. */
export class RuncRecoveryDriver implements RecoveryDriver<Request> {
  readonly options: RuncRecoveryOptions;
  readonly root: string;
  private readonly acceptanceProfile?: CheckpointProfile;
  private cachedProfile: CheckpointProfile | null = null;
  constructor(options: RuncRecoveryOptions, acceptanceProfile?: CheckpointProfile) {
    this.acceptanceProfile = acceptanceProfile;
    this.options = options; this.root = resolve(options.dataRoot);
    if (options.criu && basename(options.criu) !== 'criu') throw new Error('checkpoint_criu_filename_invalid');
  }
  private directory(requestId: string): string {
    if (!requestPattern.test(requestId)) throw new Error('checkpoint_request_id_invalid');
    return join(this.root, 'executions', requestId);
  }
  private async locked<T>(requestId: string, action: () => Promise<T>): Promise<T> {
    const directory = this.directory(requestId); await mkdir(directory, { recursive: true, mode: 0o700 });
    if (await realpath(directory) !== directory) throw new Error('checkpoint_execution_path_invalid');
    // A flushed manifest inside a newly created invocation is insufficient if
    // its directory entry can disappear after power loss.
    await syncDirectory(dirname(directory)); await syncDirectory(this.root); await syncDirectory(dirname(this.root));
    // This is an operation mutex, not a task database or a lease. Every external
    // helper ALSO holds the flock below, so an orphan helper survives safely.
    const mutex = new DatabaseSync(join(directory, 'operation.sqlite'));
    try { mutex.exec('PRAGMA busy_timeout=0; BEGIN IMMEDIATE'); return await action(); }
    finally { if (mutex.isTransaction) mutex.exec('ROLLBACK'); mutex.close(); }
  }
  private async command(requestId: string, binary: string, args: string[]) {
    try {
      return await exec(this.options.flock ?? '/usr/bin/flock', ['--exclusive', '--nonblock', join(this.directory(requestId), 'helper.lock'), binary, ...args],
        { maxBuffer: 1024 * 1024, env: { PATH: `${dirname(this.options.criu ?? '/usr/sbin/criu')}:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`,
          ...(process.env.LD_LIBRARY_PATH ? { LD_LIBRARY_PATH: process.env.LD_LIBRARY_PATH } : {}) } });
    } catch (error) {
      const failure = error as { code?: unknown; signal?: unknown; stderr?: unknown; stdout?: unknown };
      // Retain actual helper diagnostics privately; the Unix/HTTP projection
      // returns only fixed codes. No provider environment enters this helper.
      try { await atomicWriteFile(join(this.directory(requestId), 'helper.last-error.json'), JSON.stringify({
        requestId, helper: basename(binary), at: new Date().toISOString(), code: failure.code ?? null, signal: failure.signal ?? null,
        stderr: String(failure.stderr ?? ''), stdout: String(failure.stdout ?? ''),
      })); } catch { /* A full/unavailable disk must not hide the original error. */ }
      throw error;
    }
  }
  private async runc(requestId: string, args: string[]) {
    // Runtime state is boot-local. Persisted PID records from a previous boot
    // must never make runC inspect or signal an unrelated recycled PID.
    const boot = (await readFile('/proc/sys/kernel/random/boot_id', 'utf8')).trim();
    return this.command(requestId, this.options.runc ?? '/usr/local/sbin/runc', ['--root', join(this.root, 'runc-state', boot), ...args]);
  }
  async profile(): Promise<CheckpointProfile | null> { return this.readProfile(false); }
  private async readProfile(force: boolean): Promise<CheckpointProfile | null> {
    // Acceptance runs deliberately cannot advertise the capability they test.
    if (this.acceptanceProfile) return null;
    let profile: VerifiedCheckpointProfile;
    try { profile = await json(this.options.profilePath); } catch (error) { if (missing(error)) return null; throw error; }
    if (profile.schemaVersion !== 1 || profile.runtime?.runc !== '1.5.2' || profile.runtime?.criu !== '4.3'
      || !requestPattern.test(profile.profileId) || profile.rootfsDigest !== this.options.rootfsDigest
      || !Number.isSafeInteger(profile.checkpointIntervalMs) || profile.checkpointIntervalMs <= 0
      || !Array.isArray(profile.verifiedCommands) || !profile.verifiedCommands.length || !Array.isArray(profile.evidence)
      || !Array.isArray(profile.frozenInputs)
      || profile.verifiedCommands.some(command => !profile.evidence.some(item => item.command === command && requestPattern.test(item.reportDigest)))) return null;
    const { profileId, ...body } = profile;
    if (canonicalJsonDigest(body) !== profileId || profile.runtime.kernel !== release() || profile.runtime.cpuDigest !== await checkpointCpuDigest()) return null;
    // Polling the official Task must not repeatedly hash a large immutable
    // catalog. Cache only this broker instance's verified exact profile body.
    // Admission and every restore explicitly force the byte/runtime checks.
    if (!force && this.cachedProfile?.profileId === profileId) return this.cachedProfile;
    if (await checkpointTreeDigest(this.options.rootfs) !== profile.rootfsDigest) return null;
    const standardPath = join(this.options.rootfs, this.options.binary ?? '/opt/quazonai/bin/job');
    if (!(await lstat(standardPath)).isFile()) return null;
    const standard = `sha256:${(await fileDigest(standardPath)).sha256}`;
    let implementationDigest = standard;
    if (this.options.tickBinary) {
      const tickPath = join(this.options.rootfs, this.options.tickBinary);
      if (!(await lstat(tickPath)).isFile()) return null;
      const tick = `sha256:${(await fileDigest(tickPath)).sha256}`;
      implementationDigest = `sha256:${canonicalJsonDigest({ standard, tick })}`;
    }
    if (implementationDigest !== this.options.implementationDigest) return null;
    for (const root of await Promise.all(this.options.catalogRoots.map(path => realpath(path)))) {
      const frozen = profile.frozenInputs.find(item => item.root === root);
      if (!frozen || await checkpointTreeDigest(root, { allowSymlinks: false }) !== frozen.contentDigest) return null;
    }
    const [runc, criu] = await Promise.all([
      exec(this.options.runc ?? '/usr/local/sbin/runc', ['--version']), exec(this.options.criu ?? '/usr/sbin/criu', ['--version']),
    ]);
    if (!/^runc version 1\.5\.2\s/m.test(runc.stdout) || !/^Version: 4\.3\s*$/m.test(criu.stdout)) return null;
    this.cachedProfile = { profileId, verifiedCommands: [...profile.verifiedCommands], checkpointIntervalMs: profile.checkpointIntervalMs };
    return this.cachedProfile;
  }
  private async requiredProfile(command?: string, force = false): Promise<CheckpointProfile> {
    const profile = this.acceptanceProfile ?? await this.readProfile(force);
    if (!profile || command && !profile.verifiedCommands.includes(command)) throw new Error('checkpoint_profile_unverified');
    return profile;
  }
  private async state(requestId: string): Promise<'absent' | 'running' | 'paused' | 'stopped' | 'unknown'> {
    try {
      const { stdout } = await this.runc(requestId, ['state', `qz-${requestId}`]);
      const state = JSON.parse(stdout) as { status?: string; pid?: number; bundle?: string };
      if (state.status === 'running' || state.status === 'paused') {
        if (state.bundle !== join(this.directory(requestId), 'bundle')) return 'unknown';
        const identityPath = join(this.directory(requestId), 'process-identity.json');
        const currentStart = state.pid ? await this.startTime(state.pid) : null;
        if (!state.pid || !currentStart) return 'unknown';
        // Reconcile run/restore -> rememberProcess crashes from our boot-local,
        // private runC root plus an immutable original launch/restore intent.
        if (!await exists(identityPath)) {
          if (!await exists(join(this.directory(requestId), 'launch-intent.json'))) return 'unknown';
          await this.rememberProcess(requestId);
        }
        const identity = await json<{ boot: string; pid: number; startTime: string }>(identityPath);
        if (identity.pid !== state.pid || identity.boot !== (await readFile('/proc/sys/kernel/random/boot_id', 'utf8')).trim()
          || identity.startTime !== currentStart) {
          const intent = await json<{ operation: string; bundle: string }>(join(this.directory(requestId), 'launch-intent.json'));
          if (intent.operation !== 'restore' || intent.bundle !== state.bundle) return 'unknown';
          await this.rememberProcess(requestId);
        }
      }
      return state.status === 'running' ? 'running' : state.status === 'paused' ? 'paused' : state.status === 'stopped' ? 'stopped' : 'unknown';
    } catch (error) {
      const stderr = String((error as { stderr?: unknown }).stderr ?? '');
      if (/container (?:does not exist|.* does not exist)/i.test(stderr)) return 'absent';
      throw error;
    }
  }
  private async startTime(pid: number): Promise<string> {
    const stat = await readFile(`/proc/${pid}/stat`, 'utf8');
    return stat.slice(stat.lastIndexOf(')') + 2).split(' ')[19]!;
  }
  private async rememberProcess(requestId: string): Promise<void> {
    const { stdout } = await this.runc(requestId, ['state', `qz-${requestId}`]);
    const state = JSON.parse(stdout) as { pid: number; status: string };
    if (!['running', 'paused'].includes(state.status) || !Number.isSafeInteger(state.pid) || state.pid <= 0) throw new Error('checkpoint_process_not_running');
    await atomicWriteFile(join(this.directory(requestId), 'process-identity.json'), JSON.stringify({
      boot: (await readFile('/proc/sys/kernel/random/boot_id', 'utf8')).trim(), pid: state.pid, startTime: await this.startTime(state.pid),
    }));
  }
  async prepare(request: Request): Promise<void> {
    return this.locked(request.requestId, async () => {
      const profile = await this.requiredProfile(request.command, true);
      const { requestId, ...content } = request;
      if (canonicalJsonDigest(content as never) !== requestId) throw new Error('checkpoint_request_identity_invalid');
      if (request.implementationDigest !== this.options.implementationDigest || request.recoveryProfileId !== profile.profileId) throw new Error('checkpoint_implementation_changed');
      const directory = this.directory(request.requestId); const admission = join(directory, 'admission.json');
      if (await exists(admission)) {
        if (canonicalJsonDigest(await json(admission)) !== canonicalJsonDigest(request)) throw new Error('checkpoint_request_conflict');
        const state = await this.state(requestId);
        if (state === 'unknown') throw new Error('checkpoint_process_identity_unknown');
        if ((state === 'absent' || state === 'stopped') && !await exists(join(directory, 'work', 'start'))) {
          // Before the initial gate has ever opened there is no scientific
          // progress to discard. Recreate only that waiting bootstrap wrapper.
          if (state === 'stopped') await this.runc(requestId, ['delete', `qz-${requestId}`]);
          await this.launch(requestId, 'initial');
        }
        return;
      }
      const rootfs = await realpath(this.options.rootfs);
      const work = join(directory, 'work'); await mkdir(work, { recursive: true, mode: 0o700 });
      const attached = join(work, 'attachments'); await mkdir(attached, { recursive: true, mode: 0o700 });
      for (const [name, content] of Object.entries(request.files)) {
        if (!validName(name) || typeof content !== 'string' || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(content)) throw new Error('checkpoint_attachment_invalid');
        await atomicWriteFile(join(attached, name), Buffer.from(content, 'base64'));
      }
      const roots = await Promise.all(this.options.catalogRoots.map(path => realpath(path)));
      const args: string[] = [request.command];
      const flags = new Set(['--summary-only', '--plan-only']);
      const scalar = new Set(['--dataset-revision-id', '--model-artifact-id', '--feature-artifact-id', '--book-cores']);
      const files = new Set(['--model', '--scope', '--features', '--calibration']);
      const catalogs = new Set(['--catalog', '--funding-catalog', '--partitions']);
      for (let index = 0; index < request.args.length; index++) {
        const option = request.args[index]!;
        if (flags.has(option)) { args.push(option); continue; }
        const value = request.args[++index]; if (!value) throw new Error('checkpoint_argument_missing');
        if (scalar.has(option) && !value.startsWith('-')) args.push(option, value);
        else if (files.has(option) || option === '--objects') {
          const prefix = option === '--objects' ? '@objects:' : '@file:';
          const name = value.startsWith(prefix) ? value.slice(prefix.length) : '';
          if (!validName(name) || !await exists(join(attached, name))) throw new Error('checkpoint_attachment_missing');
          args.push(option, `/work/attachments/${name}`);
        } else if (catalogs.has(option)) {
          const path = await realpath(value);
          if (path !== resolve(value) || !roots.some(root => below(root, path))) throw new Error('checkpoint_catalog_unregistered');
          args.push(option, path);
        } else throw new Error('checkpoint_argument_unsupported');
      }
      const binary = isTickNativeCommand(request.command) ? this.options.tickBinary ?? this.options.binary ?? '/opt/quazonai/bin/job' : this.options.binary ?? '/opt/quazonai/bin/job';
      const inputDigest = canonicalJsonDigest(request.input);
      const execution: CompletionConfiguration = { schemaVersion: 1, requestId: request.requestId, implementationDigest: request.implementationDigest, inputDigest,
        binary, args, environment: { PATH: '/usr/bin:/bin', TMPDIR: '/work', QZ_PROGRESS_PATH: '/work/progress.ndjson' } };
      await atomicWriteFile(join(work, 'request.json'), jsonChunks(request.input as never));
      await atomicWriteFile(join(work, 'execution.json'), JSON.stringify(execution));
      await atomicWriteFile(join(work, 'completion-wrapper.ts'), await readFile(fileURLToPath(new URL('./completion-wrapper.ts', import.meta.url))));
      const bundle = join(directory, 'bundle'); await mkdir(bundle, { recursive: true, mode: 0o700 });
      const spec = { ociVersion: '1.2.0', root: { path: rootfs, readonly: true }, hostname: `qz-${request.requestId.slice(0, 12)}`,
        process: { terminal: false, user: { uid: 0, gid: 0 }, cwd: '/work', args: ['/bin/sh', '-c', 'exec "$1" --experimental-strip-types /work/completion-wrapper.ts </work/wrapper.stdin >/work/wrapper.stdout.log 2>/work/wrapper.stderr.log', 'qz-completion', this.options.nodeBinary ?? '/usr/local/bin/node'],
          env: ['PATH=/usr/local/bin:/usr/bin:/bin', 'TMPDIR=/work'], capabilities: { bounding: [], effective: [], inheritable: [], permitted: [], ambient: [] }, noNewPrivileges: true },
        mounts: [{ destination: '/proc', type: 'proc', source: 'proc' }, { destination: '/dev', type: 'tmpfs', source: 'tmpfs', options: ['nosuid', 'strictatime', 'mode=755', 'size=65536k'] },
          { destination: '/work', type: 'bind', source: work, options: ['rbind', 'rw', 'rprivate'] },
          ...roots.map(path => ({ destination: path, type: 'bind', source: path, options: ['rbind', 'rro', 'rprivate'] }))],
        linux: { namespaces: ['pid', 'ipc', 'uts', 'mount', 'network', 'cgroup'].map(type => ({ type })) } };
      await atomicWriteFile(join(bundle, 'config.json'), JSON.stringify(spec));
      await atomicWriteFile(join(work, 'wrapper.stdin'), '');
      await syncTree(work); await syncDirectory(directory);
      await atomicWriteFile(admission, jsonChunks(request as never));
      await this.launch(request.requestId, 'initial');
    });
  }
  private async launch(requestId: string, operation: 'initial' | 'restore', generation?: number): Promise<void> {
    const directory = this.directory(requestId); const bundle = join(directory, 'bundle');
    await atomicWriteFile(join(directory, 'launch-intent.json'), JSON.stringify({ operation, requestId, bundle, ...(generation === undefined ? {} : { generation }) }));
    const restoreLogs = join(directory, 'restore-logs', String(generation));
    if (operation === 'restore') await mkdir(restoreLogs, { recursive: true, mode: 0o700 });
    const args = operation === 'initial' ? ['run', '--detach', '--bundle', bundle, `qz-${requestId}`]
      : ['restore', '--detach', '--bundle', bundle, '--image-path', join(directory, 'checkpoints', String(generation), 'images'), '--work-path', restoreLogs, `qz-${requestId}`];
    await this.runc(requestId, args); await this.rememberProcess(requestId);
  }
  private async completion(requestId: string): Promise<CheckpointInspection | null> {
    const directory = this.directory(requestId); const marker = join(directory, 'work', 'exit.json');
    if (!await exists(marker)) return null;
    const record = await json<CompletionRecord>(marker);
    const request = await json<Request>(join(directory, 'admission.json'));
    if (record.schemaVersion !== 1 || record.requestId !== requestId || record.implementationDigest !== request.implementationDigest
      || record.inputDigest !== canonicalJsonDigest(request.input)
      || canonicalJsonDigest(record.stdout) !== canonicalJsonDigest(await fileDigest(join(directory, 'work', 'stdout.partial')))
      || canonicalJsonDigest(record.stderr) !== canonicalJsonDigest(await fileDigest(join(directory, 'work', 'stderr.log')))) throw new Error('checkpoint_completion_invalid');
    if (record.signal !== null) return { phase: 'stopped', completion: { exitCode: record.exitCode, signal: record.signal, error: 'native_process_interrupted' } };
    if (record.exitCode !== 0) return { phase: 'failed', completion: { exitCode: record.exitCode, signal: null, error: 'native_process_failed' } };
    const parser = new JsonReader();
    try {
      for await (const chunk of createReadStream(join(directory, 'work', 'stdout.partial'), { encoding: 'utf8' })) parser.write(chunk);
      return { phase: 'completed', completion: { exitCode: 0, signal: null, result: parser.finish() as NativeJson } };
    } catch { return { phase: 'failed', completion: { exitCode: 0, signal: null, error: 'checkpoint_result_invalid' } }; }
  }
  async inspect(requestId: string): Promise<CheckpointInspection> {
    return this.locked(requestId, async () => {
      const completed = await this.completion(requestId); if (completed) return completed;
      if (!await exists(join(this.directory(requestId), 'admission.json'))) return { phase: 'absent' };
      const state = await this.state(requestId);
      return { phase: state === 'running' && !await exists(join(this.directory(requestId), 'work', 'start')) ? 'prepared' : state === 'absent' ? 'stopped' : state === 'paused' ? 'running' : state };
    });
  }
  async checkpoint(requestId: string, generation: number): Promise<CheckpointManifest> {
    return this.locked(requestId, async () => {
      const profile = await this.requiredProfile();
      if (!Number.isSafeInteger(generation) || generation < 0) throw new Error('checkpoint_generation_invalid');
      const directory = this.directory(requestId); const generations = join(directory, 'checkpoints');
      const published = join(generations, String(generation));
      if (await exists(join(published, 'manifest.json'))) return this.verifyManifest(requestId, generation);
      const staging = join(generations, `${generation}.pending`); await mkdir(staging, { recursive: true, mode: 0o700 });
      const state = await this.state(requestId);
      const dumped = join(staging, 'dump.complete.json');
      if (generation === 0 && !await exists(join(directory, 'work', 'wrapper.ready'))) {
        if (state === 'absent' || state === 'stopped') throw new Error('checkpoint_candidate_incomplete');
        throw new Error('checkpoint_bootstrap_not_ready');
      }
      // Default runC checkpoint is stop-and-dump. Never use --leave-running:
      // its continued writes would invalidate the matching filesystem state.
      if (state === 'running' || state === 'paused') {
        await rm(staging, { recursive: true, force: true }); await mkdir(staging, { mode: 0o700 });
        if (state === 'paused') await this.runc(requestId, ['resume', `qz-${requestId}`]);
        try {
          await this.runc(requestId, ['checkpoint', '--image-path', join(staging, 'images'), '--work-path', join(staging, 'logs'), `qz-${requestId}`]);
          if (!['absent', 'stopped'].includes(await this.state(requestId))) throw new Error('checkpoint_process_still_running');
          // The dump marker authorizes resuming a copy after process/power
          // loss. Both the CRIU bytes and the original matching file state must
          // be durable BEFORE that marker, not only before the final manifest.
          await syncTree(staging); await syncTree(join(directory, 'work')); await syncDirectory(directory);
          await atomicWriteFile(dumped, JSON.stringify({ requestId, generation }));
        } catch (error) {
          if (await this.state(requestId) === 'running') await this.runc(requestId, ['pause', `qz-${requestId}`]);
          throw error;
        }
      } else if (!await exists(dumped)) throw new Error('checkpoint_candidate_incomplete');
      if (!['absent', 'stopped'].includes(await this.state(requestId))) throw new Error('checkpoint_process_still_running');
      await rm(join(staging, 'work'), { recursive: true, force: true });
      await this.command(requestId, '/bin/cp', ['-a', '--reflink=auto', join(directory, 'work'), join(staging, 'work')]);
      await syncTree(staging);
      const body = { schemaVersion: 1 as const, generation, requestId, createdAt: new Date().toISOString(), profileId: profile.profileId,
        implementationDigest: this.options.implementationDigest, files: await artifacts(staging) };
      const manifest: Manifest = { ...body, contentDigest: canonicalJsonDigest(body) };
      await atomicWriteFile(join(staging, 'manifest.json'), JSON.stringify(manifest));
      await rename(staging, published); await syncDirectory(generations);
      // No current pointer is written here. The caller commits generation in
      // the official Durable task before requesting restore.
      return manifest;
    });
  }
  private async verifyManifest(requestId: string, generation: number): Promise<Manifest> {
    const root = join(this.directory(requestId), 'checkpoints', String(generation));
    const manifest = await json<Manifest>(join(root, 'manifest.json'));
    const { contentDigest, ...body } = manifest;
    if (manifest.schemaVersion !== 1 || manifest.requestId !== requestId || manifest.generation !== generation
      || manifest.implementationDigest !== this.options.implementationDigest || canonicalJsonDigest(body) !== contentDigest
      || manifest.profileId !== (await this.requiredProfile(undefined, true)).profileId) throw new Error('checkpoint_manifest_invalid');
    if (canonicalJsonDigest(await artifacts(root) as never) !== canonicalJsonDigest(manifest.files as never)) throw new Error('checkpoint_artifact_inventory_changed');
    for (const file of manifest.files) {
      if (!validName(file.path)) throw new Error('checkpoint_manifest_path_invalid');
      const path = join(root, file.path); const stat = await lstat(path);
      if (!stat.isFile() || stat.mode % 0o10000 !== file.mode || stat.uid !== file.uid || stat.gid !== file.gid
        || canonicalJsonDigest(await fileDigest(path)) !== canonicalJsonDigest({ bytes: file.bytes, sha256: file.sha256 })) throw new Error('checkpoint_artifact_corrupt');
      if (file.linkTo) {
        if (!validName(file.linkTo)) throw new Error('checkpoint_manifest_path_invalid');
        const original = await lstat(join(root, file.linkTo));
        if (original.ino !== stat.ino || original.dev !== stat.dev) throw new Error('checkpoint_artifact_link_corrupt');
      }
    }
    return manifest;
  }
  async restore(requestId: string, generation: number): Promise<void> {
    return this.locked(requestId, async () => {
      const directory = this.directory(requestId);
      const completion = await this.completion(requestId);
      if (completion && completion.completion?.signal === null) return;
      if (await exists(join(directory, 'cancelled.json'))) throw new Error('checkpoint_execution_cancelled');
      const state = await this.state(requestId);
      if (state === 'running') return;
      if (state === 'paused') throw new Error('checkpoint_process_still_running');
      if (state === 'unknown') throw new Error('checkpoint_process_identity_unknown');
      await this.verifyManifest(requestId, generation);
      if (state === 'stopped') await this.runc(requestId, ['delete', `qz-${requestId}`]);
      const snapshot = join(directory, 'checkpoints', String(generation));
      const restored = join(directory, 'work.restore');
      await rm(restored, { recursive: true, force: true });
      await this.command(requestId, '/bin/cp', ['-a', '--reflink=auto', join(snapshot, 'work'), restored]);
      await syncTree(restored);
      await rm(join(directory, 'work'), { recursive: true, force: true });
      await rename(restored, join(directory, 'work')); await syncDirectory(directory);
      await this.launch(requestId, 'restore', generation);
    });
  }
  async start(requestId: string): Promise<void> {
    return this.locked(requestId, async () => {
      const directory = this.directory(requestId);
      if (await exists(join(directory, 'cancelled.json'))) throw new Error('checkpoint_execution_cancelled');
      if (await this.state(requestId) !== 'running') throw new Error('checkpoint_process_not_running');
      await atomicWriteFile(join(directory, 'work', 'start'), 'start\n');
    });
  }
  async cancel(requestId: string): Promise<void> {
    return this.locked(requestId, async () => {
      const completion = await this.completion(requestId);
      if (completion && completion.completion?.signal === null) return;
      const state = await this.state(requestId);
      if (state === 'unknown') throw new Error('checkpoint_process_identity_unknown');
      if (state === 'running' || state === 'paused') await this.runc(requestId, ['kill', '--all', `qz-${requestId}`, 'KILL']);
      const after = await this.state(requestId);
      if (after === 'running' || after === 'unknown') throw new Error('checkpoint_cancellation_unconfirmed');
      await atomicWriteFile(join(this.directory(requestId), 'cancelled.json'), JSON.stringify({ requestId, cancelledAt: new Date().toISOString() }));
    });
  }
}

/** Only the explicit, non-server acceptance executable uses this factory. A
 * draft permits exercising real operations but profile() remains null, and no
 * verified profile file is generated until that executable validates oracles. */
export function createCheckpointAcceptanceDriver(options: RuncRecoveryOptions, commands: string[]): { driver: RuncRecoveryDriver; profile: CheckpointProfile } {
  const profile: CheckpointProfile = { profileId: canonicalJsonDigest({ acceptance: true, rootfsDigest: options.rootfsDigest, commands }),
    verifiedCommands: [...commands], checkpointIntervalMs: 60_000 };
  return { driver: new RuncRecoveryDriver(options, profile), profile };
}
