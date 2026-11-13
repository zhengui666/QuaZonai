import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdir, readFile, realpath, writeFile } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';

const execute = promisify(execFile);
export type RustEnvironmentSource = {
  cargoBinary: string;
  manifestPath: string;
  packageName: string;
  features?: string[];
};
type MetadataPackage = {
  name: string; manifest_path: string;
  targets: { name: string; kind: string[] }[];
  dependencies: { name: string; source: string | null; req: string; features: string[]; uses_default_features: boolean; path?: string }[];
  features: Record<string, string[]>;
  rust_version: string | null;
};
export type RustEnvironmentDiscovery = {
  source: RustEnvironmentSource;
  workspaceRoot: string;
  libraryTarget: string | null;
  targets: MetadataPackage['targets'];
  nativeDependencies: MetadataPackage['dependencies'];
  declaredFeatures: Record<string, string[]>;
  rustVersion: string | null;
  qualification: 'MANIFEST_ONLY_NOT_COMPILE_OR_PRECISION_VERIFIED';
};

/** Discovery does not resolve dependencies, run a build, install a toolchain, or launch a node.
 * The owner selects the actual Cargo executable, not an implicitly installed toolchain.
 * Cargo config is evaluated from the selected manifest directory; no claim of sandboxing it.
 */
export async function discoverRustEnvironment(source: RustEnvironmentSource): Promise<RustEnvironmentDiscovery> {
  if (!isAbsolute(source.cargoBinary) || !isAbsolute(source.manifestPath)) throw new Error('rust_environment_absolute_paths_required');
  const cargoBinary = await realpath(source.cargoBinary);
  if (/^rustup(?:\.exe)?$/.test(basename(cargoBinary))) throw new Error('rust_environment_actual_cargo_required');
  const manifestPath = await realpath(source.manifestPath);
  const { stdout } = await execute(cargoBinary, ['metadata', '--no-deps', '--offline', '--locked', '--format-version', '1', '--manifest-path', manifestPath], {
    cwd: dirname(manifestPath), maxBuffer: 8 * 1024 * 1024,
  });
  const metadata = JSON.parse(stdout) as { packages: MetadataPackage[]; workspace_root: string };
  const selected = metadata.packages.filter(p => p.name === source.packageName);
  if (selected.length !== 1) throw new Error('rust_environment_package_not_unique');
  const pkg = selected[0]!;
  if ((source.features ?? []).some(feature => !Object.hasOwn(pkg.features, feature))) throw new Error('rust_environment_unknown_feature');
  return {
    source: { ...source, cargoBinary, manifestPath: pkg.manifest_path }, workspaceRoot: metadata.workspace_root,
    libraryTarget: pkg.targets.find(t => t.kind.includes('lib') || t.kind.includes('rlib'))?.name ?? null,
    targets: pkg.targets, nativeDependencies: pkg.dependencies.filter(d => d.name.startsWith('nautilus-')),
    declaredFeatures: pkg.features, rustVersion: pkg.rust_version,
    qualification: 'MANIFEST_ONLY_NOT_COMPILE_OR_PRECISION_VERIFIED',
  };
}

export function publicRustPath(value: string): string {
  if (!/^[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z_][A-Za-z0-9_]*)*$/.test(value)) throw new Error('rust_environment_invalid_factory_path');
  return value;
}

/** Materialize only QZ-owned launcher source. The native integration supplies the reviewed main
 * and dependency stanza. Never change the original workspace, lockfile, or private factory visibility.
 * A copied lock is only a starting point: a new package may require resolution in this directory.
 */
export async function materializeRustLauncher(discovery: RustEnvironmentDiscovery, directory: string, main: string, glueDependencies: string): Promise<{ manifestPath: string; qualification: 'SOURCE_ONLY_NOT_BUILT' }> {
  if (!discovery.libraryTarget) throw new Error('rust_environment_public_library_required');
  const workspace = await realpath(discovery.workspaceRoot);
  if (!isAbsolute(directory)) throw new Error('rust_launcher_absolute_directory_required');
  const normalized = resolve(directory);
  const parent = await realpath(dirname(normalized));
  const destination = join(parent, basename(normalized));
  const within = relative(workspace, destination);
  if (!isAbsolute(directory) || within === '' || (!within.startsWith(`..${sep}`) && within !== '..' && !isAbsolute(within))) throw new Error('rust_launcher_must_be_outside_user_workspace');
  const lock = await readFile(join(workspace, 'Cargo.lock'));
  // Exclusive directory creation prevents retries overwriting a prior source or build.
  await mkdir(destination);
  await mkdir(join(destination, 'src'));
  const dependency = `owner_strategy = { package = ${JSON.stringify(discovery.source.packageName)}, path = ${JSON.stringify(dirname(discovery.source.manifestPath))}, features = ${JSON.stringify(discovery.source.features ?? [])} }`;
  await writeFile(join(destination, 'Cargo.lock'), lock, { flag: 'wx' });
  await writeFile(join(destination, 'Cargo.toml'), `[package]\nname = "qz-owner-launcher"\nversion = "0.0.0"\nedition = "2024"\n\n[workspace]\n\n[dependencies]\n${dependency}\n${glueDependencies}\n`, { flag: 'wx' });
  await writeFile(join(destination, 'src/main.rs'), main, { flag: 'wx' });
  return { manifestPath: join(destination, 'Cargo.toml'), qualification: 'SOURCE_ONLY_NOT_BUILT' };
}

export type RustFactoryModules = { backtestFactory?: string; accountFactory?: string };
/** Module paths name explicit public entry contracts, not arbitrary functions or dynamic plugins.
 * owner module backtest: config(Path,name,Value) and install(BacktestEngine,name,Value).
 * owner module account: strategy(name,Value) returning its concrete official Strategy type.
 */
export function renderRustLauncher(factories: RustFactoryModules): string {
  if (!factories.backtestFactory && !factories.accountFactory) throw new Error('rust_environment_factory_required');
  const backtest = factories.backtestFactory ? publicRustPath(factories.backtestFactory) : null;
  const account = factories.accountFactory ? publicRustPath(factories.accountFactory) : null;
  return `${backtest ? `fn configured(catalog: &std::path::Path, name: &str, config: &serde_json::Value) -> anyhow::Result<job::native_configured::ConfiguredNativeRun> {
    let native_config = owner_strategy::${backtest}::config(catalog, name, config)?;
    let name = name.to_owned();
    let config = config.clone();
    Ok(job::native_configured::ConfiguredNativeRun {
        config: native_config,
        install: Box::new(move |engine| owner_strategy::${backtest}::install(engine, &name, &config)),
    })
}
` : ''}${account ? `fn account(registrar: &mut job::account_runtime::AccountStrategyRegistrar, name: &str, config: &serde_json::Value) -> anyhow::Result<()> {
    registrar.add(owner_strategy::${account}::strategy(name, config)?)
}
` : ''}fn main() {
    job::native_runner::main_with_hooks(None, ${backtest ? 'Some(configured)' : 'None'}, ${account ? 'Some(account)' : 'None'});
}
`;
}
