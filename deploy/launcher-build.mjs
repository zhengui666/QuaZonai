// Standalone distribution of the existing owner-environment builder, not a native engine.
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const [command, configPath, ...extra] = process.argv.slice(2);
if (command === '--version' && !configPath) {
  const { version } = JSON.parse(await readFile(new URL('../distribution.json', import.meta.url), 'utf8'));
  console.log(`quazonai-launcher-builder ${version}`);
} else if (!command || (command === '--help' && !configPath)) {
  console.log(`Usage: quazonai-launcher-builder inspect|build CONFIG.json

Builds a dedicated launcher against your existing Rust/Nautilus strategy project.
Requires an installed actual Cargo toolchain, cached dependencies and public factory modules.
Does not bundle Nautilus, install dependencies, connect an account or start trading.
Build executes your project's trusted Rust/build code; use the target runtime's ABI.
A successful build is COMPILED_NOT_CONNECTED, not general strategy compatibility.

CONFIG.json:
{"source":{"cargoBinary":"/absolute/toolchain/bin/cargo","manifestPath":"/absolute/owner/Cargo.toml","packageName":"owner","features":[]},"factories":{"backtestFactory":"backtest","accountFactory":"account"},"directory":"/absolute/new-output-directory"}

inspect reads source metadata only. build uses the packaged QZ glue and offline Cargo.
The output directory must not exist and its parent must exist, outside the owner workspace.
Optional targetDirectory selects an existing QZ-owned compiler cache outside that workspace.
Changing the owner strategy or Nautilus dependency requires rebuilding the launcher.`);
} else {
  try {
    if (!['inspect', 'build'].includes(command) || !configPath || extra.length) throw new Error('Expected inspect|build CONFIG.json; use --help');
    const options = JSON.parse(await readFile(configPath, 'utf8'));
    if (!options || !options.source || typeof options.source.cargoBinary !== 'string' || typeof options.source.manifestPath !== 'string' || typeof options.source.packageName !== 'string') throw new Error('source requires cargoBinary, manifestPath and packageName');
    if (command === 'inspect') {
      const { discoverRustEnvironment } = await import('./rust-environment.ts');
      console.log(JSON.stringify(await discoverRustEnvironment(options.source), null, 2));
    } else {
      if (!options.factories || typeof options.directory !== 'string') throw new Error('build requires factories and directory');
      const { buildRustLauncher } = await import('./rust-launcher-build.ts');
      const result = await buildRustLauncher({source: options.source, factories: options.factories, directory: options.directory,
        targetDirectory: options.targetDirectory, glueSourceRoot: fileURLToPath(new URL('../glue/', import.meta.url))});
      console.log(JSON.stringify(result, null, 2));
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
