#!/usr/bin/env node
// Assemble a minimal native OCI filesystem from an already built, locked Rust job
// and its native runtime dependencies. Docker creates the immutable image identity.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { spawnSync } from 'node:child_process';
import { copyNativeFile, nativeDestination } from './native-files.mjs';
import { nativeManifest } from './build-identity.mjs';

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const { values } = parseArgs({ options: {
  profile: { type: 'string', default: 'release' },
  'image-version': { type: 'string', default: 'development' },
  'job-binary': { type: 'string' },
  'output-dir': { type: 'string' },
  'isolation-probe': { type: 'string' },
  'prepare-only': { type: 'boolean', default: false },
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
const executable = path.resolve(values['job-binary'] ?? path.join(process.env.CARGO_TARGET_DIR ?? path.join(repository, 'target'), values.profile, 'job'));
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
  return nativeDestination(root, absolute);
}
function copyNative(source, target) {
  copyNativeFile(root, source, target);
}
try {
  run('job-version', executable, ['--version']);
  const timeoutBinary = '/usr/bin/timeout';
  const archiveBinaries = ['/usr/bin/tar', '/usr/bin/zstd'];
  const timeoutVersion = run('timeout-version', timeoutBinary, ['--version']).stdout;
  if (!timeoutVersion.includes('GNU coreutils')) throw new Error('Native GNU timeout required');
  const sourceCommit = run('tested-source', 'git', ['rev-parse', 'HEAD']).stdout.trim();
  copyNative(executable, '/usr/local/bin/job');
  const identity = await nativeManifest({ job: executable });
  fs.mkdirSync(destination('/usr/share/doc/quazonai'), { recursive: true, mode: 0o755 });
  fs.writeFileSync(destination('/usr/share/doc/quazonai/native-manifest.json'), JSON.stringify(identity, null, 2) + '\n');
  copyNative(timeoutBinary, '/usr/bin/timeout');
  for (const binary of archiveBinaries) copyNative(binary, binary);
  const nativeFiles = [executable, timeoutBinary, ...archiveBinaries];
  if (values['isolation-probe']) {
    const probe = fs.realpathSync(values['isolation-probe']);
    copyNative(probe, '/usr/local/bin/isolation-probe');
    nativeFiles.push(probe);
  }
  const inspectedFiles = [...new Set(nativeFiles.map(file => fs.realpathSync(file)))];
  fs.writeFileSync(path.join(directory, 'native-elf-inputs.json'), JSON.stringify(inspectedFiles, null, 2) + '\n', { mode: 0o600 });
  for (const [index, file] of inspectedFiles.entries()) {
    // Resolve symlink aliases before ldd so $ORIGIN is the actual executable's
    // directory.
    const result = run('native-ldd-' + index, 'ldd', [file], [0, 1]);
    const text = (result.stdout ?? '') + (result.stderr ?? '');
    if (result.status !== 0 && !/not a dynamic executable|statically linked/.test(text)) throw new Error('Unrecognized native dependency inspection: native-ldd-' + index);
    if (/not found/.test(text)) throw new Error('A required native shared library is unavailable: native-ldd-' + index + ' (' + path.basename(file) + ')');
    for (const line of text.split('\n')) {
      const match = line.match(/=>\s+(\/.*?)\s+\(0x/) ?? line.match(/^\s*(\/.*?)\s+\(0x/);
      if (!match) continue;
      const source = match[1];
      copyNative(source, source);
      // usr-merged hosts resolve /lib64's ELF interpreter into /usr/lib64.
      // scratch still needs the absolute path requested by the executable.
      const requested = line.match(/^\s*(\/.*?)\s+=>/);
      if (requested && requested[1] !== source) copyNative(source, requested[1]);
    }
  }
  for (const name of ['tmp', 'input', 'output']) fs.mkdirSync(destination('/' + name), { mode: name === 'tmp' ? 0o1777 : 0o755 });
  fs.chmodSync(destination('/tmp'), 0o1777);
  // Preserve upstream notices from installed distributions.
  for (const component of ['coreutils', 'tar', 'zstd', 'libacl1', 'libzstd1', 'liblz4-1', 'liblzma5', 'zlib1g', 'libc6', 'libgcc-s1', 'libstdc++6']) {
    const copyright = '/usr/share/doc/' + component + '/copyright';
    if (fs.existsSync(copyright)) copyNative(copyright, copyright);
  }
  for (const name of ['GPL-2', 'GPL-3', 'LGPL-2.1']) {
    const license = '/usr/share/common-licenses/' + name;
    if (fs.existsSync(license)) copyNative(license, license);
  }
  fs.copyFileSync(path.join(repository, 'Cargo.lock'), destination('/usr/share/doc/quazonai/Cargo.lock'));
  // Only selected public native files enter this image. A private host toolchain
  // or caller umask must not prevent the fixed OCI uid from reading/executing them.
  const normalizePermissions = (directory) => {
    fs.chmodSync(directory, 0o755);
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) normalizePermissions(file);
      else if (entry.isFile()) fs.chmodSync(file, fs.statSync(file).mode & 0o111 ? 0o755 : 0o644);
    }
  };
  normalizePermissions(root);
  fs.chmodSync(destination('/tmp'), 0o1777);
  const report = { stage: 'PREPARED_NOT_BUILT', native_image_id: null, source_commit: sourceCommit, profile: values.profile, target: 'x86_64-unknown-linux-gnu', test_probe_included: Boolean(values['isolation-probe']), base: 'scratch', commands };
  if (!values['prepare-only']) {
    const dockerfile = path.join(repository, 'runtimes/native/native-job.Dockerfile');
    run('native-image-build', 'docker', ['build', '--network=none', '--platform=linux/amd64', '--build-arg', 'VERSION=' + values['image-version'], '--build-arg', 'REVISION=' + sourceCommit, '--iidfile', path.join(directory, 'image-id.txt'), '--file', dockerfile, context]);
    const image = fs.readFileSync(path.join(directory, 'image-id.txt'), 'utf8').trim();
    if (!/^sha256:[0-9a-f]{64}$/.test(image)) throw new Error('Docker did not return a native immutable image ID');
    const info = JSON.parse(run('native-image-inspect', 'docker', ['image', 'inspect', image]).stdout)[0];
    if (info.Id !== image || info.Os !== 'linux' || info.Architecture !== 'amd64') throw new Error('Native image identity mismatch');
    run('native-image-job-version', 'docker', ['run', '--rm', '--network=none', '--read-only', '--cap-drop=ALL', '--security-opt=no-new-privileges', image, '--version']);
    report.native_image_id = image;
    report.stage = 'BUILT_AND_EXECUTED';
  }
  fs.writeFileSync(path.join(directory, 'result.json'), JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
  console.log(report.native_image_id ?? 'PREPARED_NOT_BUILT');
} catch (error) {
  fs.writeFileSync(path.join(directory, 'failure.txt'), String(error instanceof Error ? error.message : error) + '\n', { mode: 0o600 });
  throw error;
}
