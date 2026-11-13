import { randomUUID } from 'node:crypto';
import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';
import { cp, copyFile, mkdir, open, readFile, realpath, writeFile } from 'node:fs/promises';
import { basename, dirname, isAbsolute, relative, sep, join, delimiter } from 'node:path';
import { discoverRustEnvironment, materializeRustLauncher, renderRustLauncher, type RustEnvironmentSource, type RustFactoryModules } from './rust-environment.ts';

import { pathToFileURL } from 'node:url';
import { selectedCargoGraph, alignCopiedNativeManifest, type CargoResolvedMetadata } from './rust-native-dependencies.ts';
const execute = promisify(execFile);
type ResolvedMetadata = {
  packages: { id: string; name: string; version: string; source: string | null; manifest_path?: string }[];
  resolve: { nodes: { id: string; features: string[] }[] } | null;
};
function nativeResolution(metadata: ResolvedMetadata) {
  if (!metadata.resolve) throw new Error('rust_build_dependency_resolution_missing');
  return metadata.packages.filter(p => p.name.startsWith('nautilus-')).map(p => ({
    name: p.name, version: p.version, source: p.source ?? (p.manifest_path ? `path+${pathToFileURL(dirname(p.manifest_path)).href}` : null),
    precision: metadata.resolve!.nodes.find(n => n.id === p.id)?.features.filter(f => f === 'high-precision').sort() ?? [],
  }));
}
export function verifyOwnerNativeResolution(owner: ResolvedMetadata, generated: ResolvedMetadata): void {
  const prior = nativeResolution(owner); const current = nativeResolution(generated);
  if (!prior.length || !current.length) throw new Error('rust_build_native_dependencies_missing');
  // A source-only lock entry cannot establish provenance. Do not call a local fork official.
  if (prior.some(p => !p.source)) throw new Error('rust_build_path_native_provenance_unverified');
  if (new Set(current.map(p => p.name)).size !== current.length) throw new Error('rust_build_mixed_native_sources');
  for (const expected of prior) {
    const actual = current.filter(p => p.name === expected.name);
    if (actual.length !== 1 || JSON.stringify(actual[0]) !== JSON.stringify(expected)) throw new Error(`rust_build_owner_dependency_changed:${expected.name}`);
  }
}

export const nativeGlueFiles = [
  'Cargo.toml', 'Cargo.lock', 'apps/job/Cargo.toml', 'apps/job/build.rs', 'apps/job/build_identity.rs', 'apps/job/src', 'apps/job/examples', 'apps/job/tests',
  'crates/contracts/Cargo.toml', 'crates/contracts/src', 'crates/domain/Cargo.toml', 'crates/domain/src',
] as const;

/** Explicit build operation only. Never call while saving/inspecting configuration.
 * Builds execute the owner's trusted Rust/build code. Offline mode forbids dependency download;
 * no installation, SSH, network configuration, or native node execution is performed here.
 * The supplied glue source root is the application-shipped source bundle, not a user binary.
 */
export async function buildRustLauncher(options: {
  source: RustEnvironmentSource;
  factories: RustFactoryModules;
  glueSourceRoot: string;
  directory: string;
  /** QZ-owned compiler cache only; never an owner project target directory. */
  targetDirectory?: string;
  signal?: AbortSignal;
}): Promise<{ executable: string; manifestPath: string; capabilities: unknown; qualification: 'COMPILED_NOT_CONNECTED' }> {
  const discovery = await discoverRustEnvironment(options.source);
  if (!discovery.libraryTarget) throw new Error('rust_environment_public_library_required');
  const binaryName = `qz-owner-launcher-${randomUUID()}`;
  const main = renderRustLauncher(options.factories);
  const sourceRoot = await realpath(options.glueSourceRoot);
  // The caller allocates a QZ-owned build directory outside the owner's workspace.
  // materializeRustLauncher checks containment before creating its launcher subdirectory.
  if (!isAbsolute(options.directory)) throw new Error('rust_launcher_absolute_directory_required');
  const parent = await realpath(dirname(options.directory));
  const directory = join(parent, basename(options.directory));
  const relativeOwner = relative(await realpath(discovery.workspaceRoot), directory);
  if (!relativeOwner || (!relativeOwner.startsWith(`..${sep}`) && relativeOwner !== '..' && !isAbsolute(relativeOwner))) throw new Error('rust_launcher_must_be_outside_user_workspace');
  const rustc = await realpath(join(dirname(discovery.source.cargoBinary), process.platform === 'win32' ? 'rustc.exe' : 'rustc'));
  // Preflight bundle before creating partial output; never substitute a development checkout.
  await readFile(join(sourceRoot, 'apps/job/src/native_runner.rs'));
  await mkdir(directory);
  const glue = join(directory, 'glue');
  for (const path of nativeGlueFiles) {
    await mkdir(dirname(join(glue, path)), { recursive: true });
    await cp(join(sourceRoot, path), join(glue, path), { recursive: true, errorOnExist: true, force: false });
  }
  const targetDirectory = options.targetDirectory ? await realpath(options.targetDirectory) : join(directory, 'target');
  const relativeTarget = relative(await realpath(discovery.workspaceRoot), targetDirectory);
  if (!relativeTarget || (!relativeTarget.startsWith(`..${sep}`) && relativeTarget !== '..' && !isAbsolute(relativeTarget))) throw new Error('rust_build_target_must_be_outside_user_workspace');
  const env = { ...process.env, CARGO_PROFILE_DEV_DEBUG: '0', CARGO_INCREMENTAL: '0', RUSTC: rustc, PATH: `${dirname(rustc)}${delimiter}${process.env.PATH ?? ''}`, CARGO_TARGET_DIR: targetDirectory, CARGO_NET_OFFLINE: 'true', QZ_NATIVE_BUILD_LOCK: join(directory, 'launcher/Cargo.lock') };
  const compiler = await execute(rustc, ['-vV'], { env, signal: options.signal, maxBuffer: 64 * 1024 });
  const host = /^host: (\S+)$/m.exec(compiler.stdout)?.[1];
  if (!host) throw new Error('rust_build_compiler_host_missing');
  const metadata = async (manifest: string, locked: boolean, selectedFeatures: string[] = []) => {
    const args = ['metadata', '--offline', '--filter-platform', host, ...(locked ? ['--locked'] : []), '--format-version', '1', '--manifest-path', manifest, ...(selectedFeatures.length ? ['--features', selectedFeatures.join(',')] : [])];
    const { stdout } = await execute(discovery.source.cargoBinary, args, { cwd: dirname(manifest), env, signal: options.signal, maxBuffer: 32 * 1024 * 1024 });
    const parsed = JSON.parse(stdout) as CargoResolvedMetadata;
    // Match Rust's canonical local-source identity; no user file is changed.
    for (const pkg of parsed.packages) if (pkg.name.startsWith('nautilus-') && pkg.source === null) pkg.manifest_path = await realpath(pkg.manifest_path);
    return parsed;
  };
  const originalLock = await readFile(join(discovery.workspaceRoot, 'Cargo.lock'));
  const ownerMetadata = await metadata(discovery.source.manifestPath, true, discovery.source.features);
  const ownerPackage = ownerMetadata.packages.find(p => p.manifest_path === discovery.source.manifestPath);
  if (!ownerPackage) throw new Error('rust_build_owner_package_missing');
  const owner = selectedCargoGraph(ownerMetadata, ownerPackage.id);
  const copiedManifest = join(glue, 'apps/job/Cargo.toml');
  await writeFile(copiedManifest, alignCopiedNativeManifest(await readFile(copiedManifest, 'utf8'), owner));
  const features = options.factories.accountFactory ? ['account-runtime'] : [];
  if (nativeResolution(owner).find(p => p.name === 'nautilus-model')?.precision.includes('high-precision')) features.push('high-precision');
  const dependencies = `job = { path = ${JSON.stringify(join(glue, 'apps/job'))}, default-features = false, features = ${JSON.stringify(features)} }\nanyhow = "1"\nserde_json = "1"`;
  const materialized = await materializeRustLauncher(discovery, join(directory, 'launcher'), main, dependencies, binaryName);
  const generatedMetadata = await metadata(materialized.manifestPath, false);
  const launcher = generatedMetadata.packages.find(p => p.manifest_path === materialized.manifestPath);
  if (!launcher) throw new Error('rust_build_launcher_package_missing');
  const generated = selectedCargoGraph(generatedMetadata, launcher.id);
  verifyOwnerNativeResolution(owner, generated);
  const metadataPath = join(directory, 'resolved-cargo-metadata.json');
  await writeFile(metadataPath, JSON.stringify(generatedMetadata), { flag: 'wx' });
  Object.assign(env, { QZ_NATIVE_BUILD_METADATA: metadataPath });
  const buildLog = join(directory, 'build.log');
  const log = await open(buildLog, 'wx');
  try {
    await new Promise<void>((resolve, reject) => {
      if (options.signal?.aborted) { reject(new Error('rust_build_interrupted_outcome_unknown')); return; }
      const child = spawn(discovery.source.cargoBinary, ['build', '--frozen', '--target', host, '--manifest-path', materialized.manifestPath, '--bin', binaryName, '--jobs', '1'], {
        cwd: dirname(materialized.manifestPath), env, stdio: ['ignore', log.fd, log.fd],
      });
      let interrupted = false;
      const abort = () => { interrupted = true; child.kill(); };
      options.signal?.addEventListener('abort', abort, { once: true });
      child.once('error', error => { options.signal?.removeEventListener('abort', abort); reject(error); });
      child.once('close', code => {
        options.signal?.removeEventListener('abort', abort);
        // Killing Cargo cannot prove its compiler/build-script descendants have stopped.
        if (interrupted) reject(new Error('rust_build_interrupted_outcome_unknown'));
        else if (code !== 0) reject(new Error(`rust_build_failed:exit=${code}:log=${buildLog}`));
        else resolve();
      });
    });
  } finally { await log.close(); }
  if (!(await readFile(join(discovery.workspaceRoot, 'Cargo.lock'))).equals(originalLock)) throw new Error('rust_build_owner_lock_changed');
  const built = await realpath(join(targetDirectory, host, 'debug', process.platform === 'win32' ? `${binaryName}.exe` : binaryName));
  await mkdir(join(directory, 'bin'));
  const executable = join(directory, 'bin', process.platform === 'win32' ? 'qz-owner-launcher.exe' : 'qz-owner-launcher');
  await copyFile(built, executable);
  const { stdout } = await execute(executable, ['capabilities'], { cwd: directory, env, signal: options.signal, maxBuffer: 1024 * 1024 });
  const capabilities = JSON.parse(stdout);
  if (capabilities.protocol !== 'quazonai-native-process-v1' || capabilities.running !== false
    || capabilities.research?.configuredRunFactory !== Boolean(options.factories.backtestFactory)
    || capabilities.account?.registration !== Boolean(options.factories.accountFactory)) throw new Error('rust_build_capabilities_mismatch');
  verifyLauncherIdentity(generated, capabilities.identity);
  return { executable, manifestPath: materialized.manifestPath, capabilities, qualification: 'COMPILED_NOT_CONNECTED' };
}

export function verifyLauncherIdentity(metadata: ResolvedMetadata, identity: { nautilusSource?: string; nautilusVersion?: string; precision?: string } | undefined): void {
  const model = nativeResolution(metadata).filter(p => p.name === 'nautilus-model');
  if (model.length !== 1 || !identity || identity.nautilusSource !== model[0]!.source || identity.nautilusVersion !== model[0]!.version
    || identity.precision !== (model[0]!.precision.includes('high-precision') ? 'high' : 'standard')) throw new Error('rust_build_binary_identity_mismatch');
}
