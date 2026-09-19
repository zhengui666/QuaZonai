/**
 * Test-only use of the shipped systemd user units with a disposable release.
 * systemd owns supervision and restart; this adapter only commands and observes it.
 * No account, persistent unit installation, global manager changes or process fallback.
 */
import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { copyFile, lstat, mkdir, readFile, readlink, realpath, rm, stat, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const properties = [
  'Id', 'LoadState', 'UnitFileState', 'ActiveState', 'SubState', 'MainPID',
  'ExecMainPID', 'ExecMainCode', 'ExecMainStatus', 'Result', 'NRestarts',
  'Type', 'Restart', 'RestartUSec', 'TimeoutStopUSec', 'KillMode', 'KillSignal',
  'UMask', 'StandardOutput', 'StandardError', 'WorkingDirectory', 'FragmentPath',
  'DropInPaths', 'ControlGroup', 'InvocationID',
];
function quoted(value, specifiers = false) {
  if (typeof value !== 'string' || /[\0\r\n]/.test(value)) throw new Error('Invalid test service configuration value');
  const escaped = value.replaceAll('\\', '\\\\').replaceAll('"', '\\"');
  return `"${specifiers ? escaped.replaceAll('%', '%%') : escaped}"`;
}
// WorkingDirectory and EnvironmentFile consume one path, not shell words.
// Their v255 parsers expand specifiers but do not remove surrounding quotes.
export function unitPath(value) {
  if (typeof value !== 'string' || !isAbsolute(value) || /[\0\r\n]/.test(value)
    || value.trim() !== value || value.endsWith('\\')) {
    throw new Error('Invalid single-line absolute test unit path');
  }
  return value.replaceAll('%', '%%');
}
async function absent(path) {
  try { await lstat(path); return false; }
  catch (error) { if (error.code === 'ENOENT') return true; throw error; }
}

export class NativeUserServices {
  constructor({ repo, root, release, binary, run, env, interrupted }) {
    this.repo = repo; this.root = root; this.release = release; this.binary = binary;
    this.run = run; this.env = env; this.interrupted = interrupted;
    this.units = [];
    this.quiescent = true;
    this.evidence = { scope: 'real shipped user units; disposable same-user loopback installation',
      snapshots: [], cleanup_complete: false };
  }

  async control(name, args, { cleanup = false, recordStage = true, timeout = 15_000 } = {}) {
    return this.run(name, '/usr/bin/systemctl', ['--user', '--no-pager', ...args], {
      env: this.env, privateOutput: true, cleanup, recordStage, timeout,
    });
  }

  unit(kind) {
    const unit = this.units.find(item => item.kind === kind);
    if (!unit) throw new Error('Unknown test-owned service');
    return unit;
  }

  async install(environment) {
    if (process.platform !== 'linux' || !process.getuid || process.getuid() === 0) {
      throw new Error('Native user-service acceptance requires ordinary-user Linux');
    }
    const runtime = this.env.XDG_RUNTIME_DIR;
    if (!runtime || !isAbsolute(runtime) || (await stat(runtime)).uid !== process.getuid()) {
      throw new Error('Supply the actual running service user manager XDG_RUNTIME_DIR');
    }
    await stat('/sys/fs/cgroup/cgroup.controllers');
    this.evidence.manager_version = (await this.control('native-user-manager-version',
      ['show', '--property=Version', '--value'])).trim();
    assert.ok(this.evidence.manager_version);
    this.environment = environment;
    const unitRoot = resolve(runtime, 'systemd/user');
    await mkdir(unitRoot, { recursive: true, mode: 0o700 });
    const directory = resolve(this.root, 'units');
    await mkdir(directory, { mode: 0o700 });
    const environmentFile = resolve(this.root, 'service.env');
    await writeFile(environmentFile, Object.entries(environment)
      .map(([key, value]) => {
        if (!/^[A-Z][A-Z0-9_]*$/.test(key)) throw new Error('Invalid test environment key');
        return `${key}=${quoted(value)}`;
      }).join('\n') + '\n', { mode: 0o600, flag: 'wx' });
    const suffix = randomBytes(12).toString('hex');
    for (const [kind, command] of [['api', 'serve'], ['worker', 'worker']]) {
      const name = `quazonai-web-${suffix}-${kind}.service`;
      const path = resolve(directory, name);
      const dropin = resolve(unitRoot, `${name}.d`);
      const links = [resolve(unitRoot, name), resolve(unitRoot, 'default.target.wants', name)];
      if (!(await absent(dropin)) || !(await absent(links[0])) || !(await absent(links[1]))) {
        throw new Error('Refuse to replace an existing unit path');
      }
      const known = await this.control(`require-fresh-${kind}-unit`,
        ['list-unit-files', '--no-legend', '--full', '--plain']);
      if (known.split('\n').some(line => line.trim().split(/\s+/)[0] === name)) {
        throw new Error('Refuse to replace an existing native user unit');
      }
      const unit = { kind, command, name, path, dropin, links, createdDropin: false, group: null };
      this.units.push(unit);
      const original = resolve(this.repo, 'deploy/systemd', `quazonai-${kind}.service`);
      const bytes = await readFile(original);
      assert.ok(bytes.toString().includes(`ExecStart=/opt/quazonai/current/bin/server ${command}\n`));
      await copyFile(original, path);
      assert.deepEqual(await readFile(path), bytes, 'Use the shipped unit without rewriting its policy');
      await mkdir(dropin, { mode: 0o700 });
      unit.createdDropin = true;
      // Only disposable installation paths change. In particular RestartSec,
      // KillMode, stop deadline, output destination and Type stay with the unit.
      await writeFile(resolve(dropin, 'override.conf'), `[Service]\n`
        + `WorkingDirectory=${unitPath(this.release)}\n`
        + `EnvironmentFile=\nEnvironmentFile=${unitPath(environmentFile)}\n`
        + `ExecStart=\nExecStart=${quoted(this.binary, true)} ${command}\n`,
      { mode: 0o600, flag: 'wx' });
    }
    // Keep attempted registration even if the command loses its acknowledgement.
    this.registrationAttempted = true;
    await this.control('enable-native-user-units',
      ['enable', '--runtime', ...this.units.map(unit => unit.path)]);
    for (const unit of this.units) {
      for (const link of unit.links) assert.equal(await realpath(link), unit.path);
      const current = await this.show(unit);
      await this.policy(unit, current);
      this.capture('installed', unit, current);
    }
  }

  async show(unit, cleanup = false) {
    const raw = await this.control(`inspect-${unit.kind}`, [
      'show', '--all', `--property=${properties.join(',')}`, unit.name,
    ], { cleanup, recordStage: false });
    const result = Object.fromEntries(raw.trim().split('\n').filter(Boolean).map(line => {
      const at = line.indexOf('=');
      if (at < 1) throw new Error('Invalid native unit property output');
      return [line.slice(0, at), line.slice(at + 1)];
    }));
    assert.equal(result.Id, unit.name);
    assert.equal(result.LoadState, 'loaded');
    if (result.ControlGroup) {
      assert.ok(result.ControlGroup.startsWith('/') && result.ControlGroup.endsWith(`/${unit.name}`));
      // Retain the original group before later process/identity checks can fail.
      unit.group = result.ControlGroup;
    }
    return result;
  }

  async policy(unit, current) {
    for (const [key, value] of Object.entries({
      UnitFileState: 'enabled-runtime', Type: 'exec', Restart: 'on-failure',
      RestartUSec: '15s', TimeoutStopUSec: '5min', KillMode: 'mixed',
      KillSignal: '15', UMask: '0077', StandardOutput: 'journal', StandardError: 'journal',
      WorkingDirectory: this.release,
      DropInPaths: resolve(unit.dropin, 'override.conf'),
    })) assert.equal(current[key], value, `Native ${unit.kind} ${key}`);
    // systemd may name its loaded runtime symlink rather than the target file.
    // Both spellings must still resolve to this fixture's original unit.
    assert.ok([unit.path, unit.links[0]].includes(current.FragmentPath), 'Unexpected native unit load path');
    assert.equal(await realpath(current.FragmentPath), await realpath(unit.path), 'Native unit source differs');
  }

  capture(phase, unit, current) {
    // This closed set never contains Environment or command output/credentials.
    this.evidence.snapshots.push({ phase, role: unit.kind, ...current });
  }

  async process(unit, current) {
    await this.policy(unit, current);
    const pid = Number(current.MainPID);
    assert.ok(Number.isSafeInteger(pid) && pid > 1);
    assert.equal(current.ActiveState, 'active');
    assert.equal(current.SubState, 'running');
    const [executable, cwd, status, command, environment] = await Promise.all([
      realpath(`/proc/${pid}/exe`), realpath(`/proc/${pid}/cwd`),
      readFile(`/proc/${pid}/status`, 'utf8'), readFile(`/proc/${pid}/cmdline`, 'utf8'),
      readFile(`/proc/${pid}/environ`, 'utf8'),
    ]);
    assert.equal(executable, await realpath(this.binary));
    assert.equal(cwd, await realpath(this.release));
    assert.deepEqual(command.split('\0').filter(Boolean), [this.binary, unit.command]);
    const uid = status.match(/^Uid:\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)$/m);
    assert.ok(uid && uid.slice(1).every(value => Number(value) === process.getuid()));
    const actual = Object.fromEntries(environment.split('\0').filter(Boolean).map(item => {
      const at = item.indexOf('='); return [item.slice(0, at), item.slice(at + 1)];
    }));
    for (const [key, value] of Object.entries(this.environment)) {
      if (actual[key] !== value) throw new Error(`Native ${unit.kind} environment mismatch for ${key}`);
    }
    for (const key of ['QUAZONAI_WEB_TEST_ADMIN_URL', 'GITHUB_TOKEN', 'GH_TOKEN',
      'OPENAI_API_KEY', 'CODEX_DEPLOYMENT', 'MISSION_API_ORIGIN', 'MISSION_WORKSPACES']) {
      if (actual[key]) throw new Error(`Unexpected ambient ${key} in test service`);
    }
    assert.equal(unit.group, current.ControlGroup);
    assert.ok(unit.group);
    return pid;
  }

  async running(kind, restarts, previousPid) {
    const unit = this.unit(kind);
    const deadline = Date.now() + 45_000;
    while (Date.now() < deadline && !this.interrupted()) {
      const current = await this.show(unit);
      if (Number(current.NRestarts) > restarts) throw new Error(`Unexpected repeated ${kind} restart`);
      if (current.ActiveState === 'active' && current.SubState === 'running'
        && Number(current.NRestarts) === restarts && Number(current.MainPID) !== previousPid) {
        const pid = await this.process(unit, current);
        this.capture('running', unit, current);
        return { pid, restarts, invocation: current.InvocationID };
      }
      await delay(200);
    }
    throw new Error(`Native ${kind} did not reach the expected running state`);
  }

  async start(kind) {
    const unit = this.unit(kind);
    this.quiescent = false;
    await this.control(`start-native-${kind}`, ['start', unit.name]);
    return this.running(kind, 0);
  }

  async assertRunning(kind, expected) {
    const unit = this.unit(kind);
    const current = await this.show(unit);
    assert.equal(Number(current.MainPID), expected.pid, `The original ${kind} process changed unexpectedly`);
    assert.equal(Number(current.NRestarts), expected.restarts);
    await this.process(unit, current);
    this.capture('still-running', unit, current);
  }

  async crashWorker(expected) {
    await this.assertRunning('worker', expected);
    const started = Date.now();
    await this.control('kill-idle-native-worker', [
      'kill', '--kill-whom=main', '--signal=SIGKILL', this.unit('worker').name,
    ]);
    // No start/restart call here: observe the unchanged native Restart policy.
    const restarted = await this.running('worker', expected.restarts + 1, expected.pid);
    assert.notEqual(restarted.invocation, expected.invocation);
    this.evidence.worker_restart = {
      previous_pid: expected.pid, current_pid: restarted.pid,
      previous_restarts: expected.restarts, current_restarts: restarted.restarts,
      elapsed_ms: Date.now() - started, idle_only: true,
    };
    return restarted;
  }

  async emptyGroup(unit) {
    if (!unit.group) return;
    const events = resolve('/sys/fs/cgroup', `.${unit.group}`, 'cgroup.events');
    try { assert.match(await readFile(events, 'utf8'), /^populated 0$/m); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
  }

  async stop(kind, { cleanup = false, graceful = true } = {}) {
    const unit = this.unit(kind);
    // A partially started service may never have reached process verification.
    // Observe its original cgroup before native stop can remove that property.
    await this.show(unit, cleanup);
    await this.control(`stop-native-${kind}`, ['stop', unit.name], { cleanup, timeout: 310_000 });
    const current = await this.show(unit, cleanup);
    assert.equal(current.MainPID, '0');
    assert.ok(['inactive', 'failed'].includes(current.ActiveState));
    await this.emptyGroup(unit);
    if (graceful) {
      assert.equal(current.Result, 'success');
      assert.equal(current.ExecMainCode, '1'); // CLD_EXITED, not a forced signal.
      assert.equal(current.ExecMainStatus, '0');
    }
    this.capture('stopped', unit, current);
    return current;
  }

  async cleanup({ graceful = false } = {}) {
    let failure;
    if (this.registrationAttempted) {
      let stopped = true;
      for (const unit of [...this.units].reverse()) {
        try { await this.stop(unit.kind, { cleanup: true, graceful }); }
        catch (error) { stopped = false; failure ??= error; }
      }
      this.quiescent = stopped;
      if (!stopped) throw failure;
      for (const unit of this.units) {
        try {
          unit.log = await this.run(`native-${unit.kind}-journal`, '/usr/bin/journalctl', [
            '--user', '--no-pager', '--output=cat', '--lines=200', `--unit=${unit.name}`,
          ], { env: this.env, privateOutput: true, cleanup: true, recordStage: false, timeout: 15_000 });
        } catch (error) { failure ??= error; }
      }
      await this.control('disable-native-user-units',
        ['disable', '--runtime', ...this.units.map(unit => unit.name)], { cleanup: true });
    }
    for (const unit of this.units) {
      // Native disable removes links. Remove any remaining own link only after
      // checking its destination, never a similarly named pre-existing file.
      for (const link of unit.links) if (!(await absent(link))) {
        assert.equal(resolve(dirname(link), await readlink(link)), unit.path);
        await rm(link);
      }
      if (unit.createdDropin) await rm(unit.dropin, { recursive: true });
    }
    if (this.registrationAttempted) {
      await this.control('reload-after-native-unit-cleanup', ['daemon-reload'], { cleanup: true });
      for (const unit of this.units) {
        for (const link of unit.links) assert.ok(await absent(link));
        assert.ok(await absent(unit.dropin));
        await this.emptyGroup(unit);
      }
    }
    if (failure) throw failure;
    this.evidence.cleanup_complete = true;
  }
}
