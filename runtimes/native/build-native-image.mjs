#!/usr/bin/env node
// Assemble a minimal native OCI filesystem from an already built, locked Rust job
// and the exact rustup toolchain. Docker alone creates the immutable image identity.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { spawnSync } from 'node:child_process';

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const { values } = parseArgs({ options: {
  profile: { type: 'string', default: 'release' },
  'output-dir': { type: 'string' },
  'isolation-probe': { type: 'string' },
}, allowPositionals: false });
if (!['debug', 'release'].includes(values.profile) || !values['output-dir']) {
  throw new Error('Use --profile debug|release --output-dir NEW_DIRECTORY');
}
if (process.platform !== 'linux' || process.arch !== 'x64') throw new Error('Native image requires Linux x86_64');
const directory = path.resolve(values['output-dir']);
if (directory === repository || directory.startsWith(repository + path.sep)) {
  throw new Error('Image assembly must use a new directory outside the checkout');
}
fs.mkdirSync(directory, { mode: 0o700 });
const context = path.join(directory, 'context');
fs.mkdirSync(context, { mode: 0o755 });
const root = path.join(context, 'native-root');
fs.mkdirSync(root, { mode: 0o755 });
const executable = path.join(repository, 'target', values.profile, 'job');
const env = Object.fromEntries(['PATH', 'HOME', 'RUSTUP_HOME', 'CARGO_HOME', 'TMPDIR', 'LANG', 'LC_ALL', 'DOCKER_HOST', 'DOCKER_CONTEXT'].filter(name => process.env[name] !== undefined).map(name => [name, process.env[name]]));
const commands = [];
function run(name, binary, args, allowed = [0]) {
  const result = spawnSync(binary, args, { cwd: repository, env, encoding: 'utf8', timeout: 600000, maxBuffer: 16 * 1024 * 1024 });
  fs.writeFileSync(path.join(directory, name + '.stdout'), result.stdout ?? '', { mode: 0o600 });
  fs.writeFileSync(path.join(directory, name + '.stderr'), result.stderr ?? String(result.error ?? ''), { mode: 0o600 });
  commands.push({ name, binary, args, exit_code: result.status });
  fs.writeFileSync(path.join(directory, 'commands.json'), JSON.stringify(commands, null, 2) + '\n', { mode: 0o600 });
  if (result.error || !allowed.includes(result.status)) throw new Error(name + ': native command failed');
  return result;
}
function destination(absolute) {
  if (!path.isAbsolute(absolute) || path.normalize(absolute) !== absolute) throw new Error('Noncanonical native image path');
  return path.join(root, absolute.slice(1));
}
function copyNative(source, target) {
  const metadata = fs.statSync(source);
  if (!metadata.isFile()) throw new Error('Expected an original native regular file');
  const selected = destination(target);
  fs.mkdirSync(path.dirname(selected), { recursive: true, mode: 0o755 });
  if (fs.existsSync(selected)) {
    if (!fs.readFileSync(selected).equals(fs.readFileSync(source))) throw new Error('Conflicting native library');
    return;
  }
  fs.copyFileSync(source, selected, fs.constants.COPYFILE_EXCL);
  fs.chmodSync(selected, metadata.mode & 0o777);
}
function isElf(file) {
  const header = Buffer.alloc(4);
  const descriptor = fs.openSync(file, 'r');
  try { return fs.readSync(descriptor, header, 0, 4, 0) === 4 && header.equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46])); }
  finally { fs.closeSync(descriptor); }
}
function nativeExecutables(directory) {
  const result = [];
  for (const item of fs.readdirSync(directory, { withFileTypes: true })) {
    const file = path.join(directory, item.name);
    if (item.isDirectory()) result.push(...nativeExecutables(file));
    else if ((item.isFile() || item.isSymbolicLink()) && isElf(file)) result.push(file);
  }
  return result;
}
try {
  const rustcVersion = run('rustc-version', 'rustup', ['run', '1.98.1', 'rustc', '-Vv']).stdout;
  if (!rustcVersion.includes('release: 1.98.1\n') || !rustcVersion.includes('host: x86_64-unknown-linux-gnu\n')) throw new Error('Exact native Rust toolchain required');
  const sysroot = fs.realpathSync(run('rust-sysroot', 'rustup', ['run', '1.98.1', 'rustc', '--print', 'sysroot']).stdout.trim());
  const installed = run('rust-targets', 'rustup', ['target', 'list', '--installed', '--toolchain', '1.98.1']).stdout;
  if (!installed.split(/\r?\n/).includes('wasm32-unknown-unknown')) throw new Error('Install the native wasm32-unknown-unknown standard target first');
  run('job-version', executable, ['--version']);
  const timeoutBinary = '/usr/bin/timeout';
  const timeoutVersion = run('timeout-version', timeoutBinary, ['--version']).stdout;
  if (!timeoutVersion.includes('GNU coreutils')) throw new Error('Native GNU timeout required');
  run('tested-source', 'git', ['rev-parse', 'HEAD']);
  copyNative(executable, '/usr/local/bin/job');
  copyNative(timeoutBinary, '/usr/bin/timeout');
  fs.mkdirSync(destination('/opt/rust'), { recursive: true, mode: 0o755 });
  copyNative(path.join(sysroot, 'bin/rustc'), '/opt/rust/bin/rustc');
  fs.cpSync(path.join(sysroot, 'lib'), destination('/opt/rust/lib'), { recursive: true, dereference: false, verbatimSymlinks: true, errorOnExist: true, force: false });
  const nativeFiles = [executable, timeoutBinary, path.join(sysroot, 'bin/rustc'), ...nativeExecutables(path.join(sysroot, 'lib'))];
  if (values['isolation-probe']) {
    const probe = fs.realpathSync(values['isolation-probe']);
    copyNative(probe, '/usr/local/bin/isolation-probe');
    nativeFiles.push(probe);
  }
  for (const [index, file] of [...new Set(nativeFiles)].entries()) {
    const result = run('native-ldd-' + index, 'ldd', [file], [0, 1]);
    const text = (result.stdout ?? '') + (result.stderr ?? '');
    if (result.status !== 0 && !/not a dynamic executable|statically linked/.test(text)) throw new Error('Unrecognized native dependency inspection');
    if (/not found/.test(text)) throw new Error('A required native shared library is unavailable');
    for (const line of text.split('\n')) {
      const match = line.match(/=>\s+(\/.*?)\s+\(0x/) ?? line.match(/^\s*(\/.*?)\s+\(0x/);
      if (!match) continue;
      const source = match[1];
      // rustc/LLVM use their native relative RPATH under the relocated complete sysroot.
      if (source.startsWith(sysroot + path.sep)) continue;
      copyNative(source, source);
    }
  }
  for (const name of ['tmp', 'input', 'output']) fs.mkdirSync(destination('/' + name), { mode: name === 'tmp' ? 0o1777 : 0o755 });
  fs.chmodSync(destination('/tmp'), 0o1777);
  for (const name of ['LICENSE', 'NOTICE']) {
    const source = path.join(repository, name);
    if (fs.existsSync(source)) copyNative(source, '/usr/share/doc/quazonai/' + name);
  }
  // Preserve upstream notices from the installed native distributions, not invented licenses.
  const noticeRoot = path.join(sysroot, 'share/doc/rust');
  if (fs.existsSync(noticeRoot)) fs.cpSync(noticeRoot, destination('/usr/share/doc/rust'), { recursive: true, dereference: false, verbatimSymlinks: true });
  for (const component of ['coreutils', 'libc6', 'libgcc-s1', 'libstdc++6']) {
    const copyright = '/usr/share/doc/' + component + '/copyright';
    if (fs.existsSync(copyright)) copyNative(copyright, copyright);
  }
  fs.copyFileSync(path.join(repository, 'Cargo.lock'), destination('/usr/share/doc/quazonai/Cargo.lock'));
  // Image directory permissions must not inherit a restrictive caller umask: the
  // selected files contain no secrets and must be readable by the fixed OCI uid.
  const normalizeDirectories = (directory) => {
    fs.chmodSync(directory, 0o755);
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      if (entry.isDirectory()) normalizeDirectories(path.join(directory, entry.name));
    }
  };
  normalizeDirectories(root);
  fs.chmodSync(destination('/tmp'), 0o1777);
  const dockerfile = path.join(repository, 'runtimes/native/native-job.Dockerfile');
  run('native-image-build', 'docker', ['build', '--network=none', '--platform=linux/amd64', '--iidfile', path.join(directory, 'image-id.txt'), '--file', dockerfile, context]);
  const image = fs.readFileSync(path.join(directory, 'image-id.txt'), 'utf8').trim();
  if (!/^sha256:[0-9a-f]{64}$/.test(image)) throw new Error('Docker did not return a native immutable image ID');
  const info = JSON.parse(run('native-image-inspect', 'docker', ['image', 'inspect', image]).stdout)[0];
  if (info.Id !== image || info.Os !== 'linux' || info.Architecture !== 'amd64') throw new Error('Native image identity mismatch');
  run('native-image-job-version', 'docker', ['run', '--rm', '--network=none', '--read-only', '--cap-drop=ALL', '--security-opt=no-new-privileges', image, '--version']);
  const report = { native_image_id: image, profile: values.profile, compiler: '1.98.1', target: 'wasm32-unknown-unknown', test_probe_included: Boolean(values['isolation-probe']), base: 'scratch', commands };
  fs.writeFileSync(path.join(directory, 'result.json'), JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
  console.log(image);
} catch (error) {
  fs.writeFileSync(path.join(directory, 'failure.txt'), String(error instanceof Error ? error.message : error) + '\n', { mode: 0o600 });
  throw error;
}
