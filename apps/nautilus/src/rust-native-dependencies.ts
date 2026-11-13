import { dirname } from 'node:path';

export type CargoResolvedMetadata = {
  packages: { id: string; name: string; version: string; source: string | null; manifest_path: string }[];
  resolve: { nodes: { id: string; features: string[]; deps: { pkg: string; dep_kinds: { kind: string | null }[] }[] }[] } | null;
};

/** Scope identity to the chosen package's normal/build dependency closure. Workspace
 * siblings and dev-only dependencies do not identify this executable. */
export function selectedCargoGraph(metadata: CargoResolvedMetadata, packageId: string): CargoResolvedMetadata {
  if (!metadata.resolve) throw new Error('rust_build_dependency_resolution_missing');
  const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node]));
  const selected = new Set<string>(); const pending = [packageId];
  while (pending.length) {
    const id = pending.pop()!; if (selected.has(id)) continue;
    const node = nodes.get(id); if (!node) throw new Error(`rust_build_dependency_node_missing:${id}`);
    selected.add(id);
    for (const dep of node.deps) if (dep.dep_kinds.some(kind => kind.kind !== 'dev')) pending.push(dep.pkg);
  }
  return { packages: metadata.packages.filter(p => selected.has(p.id)), resolve: { nodes: metadata.resolve.nodes.filter(n => selected.has(n.id)) } };
}

/** Modify only the copied QZ manifest's known Nautilus dependency declarations.
 * Cargo remains the TOML parser/resolver and the actual Rust API compatibility test.
 * No user manifests, lockfiles, cached upstream sources, or dependency code are edited. */
export function alignCopiedNativeManifest(manifest: string, owner: CargoResolvedMetadata): string {
  const packages = owner.packages.filter(p => p.name.startsWith('nautilus-'));
  const byName = new Map<string, typeof packages[number]>();
  for (const p of packages) {
    if (byName.has(p.name)) throw new Error(`rust_build_ambiguous_owner_package:${p.name}`);
    byName.set(p.name, p);
  }
  const model = byName.get('nautilus-model');
  if (!model) throw new Error('rust_build_native_dependencies_missing');
  const specification = (name: string) => {
    const pkg = byName.get(name);
    const source = pkg?.source ?? (pkg ? null : model.source);
    if (source?.startsWith('git+')) {
      const uri = new URL(source.slice(4).split('#')[0]!);
      const keys = [...uri.searchParams.keys()];
      if (keys.some(key => !['rev', 'branch', 'tag'].includes(key)) || keys.length > 1) throw new Error('rust_build_git_source_unsupported');
      const key = keys[0]; const ref = key ? `, ${key} = ${JSON.stringify(uri.searchParams.get(key))}` : '';
      uri.search = '';
      // Missing optional QZ crates may come from the same monorepo source. Cargo
      // resolves them offline; active package identity is checked after resolution.
      return `git = ${JSON.stringify(uri.toString())}${ref}`;
    }
    if (!pkg) throw new Error(`rust_build_owner_package_source_missing:${name}`);
    if (source === null) return `path = ${JSON.stringify(dirname(pkg.manifest_path))}`;
    if (source === 'registry+https://github.com/rust-lang/crates.io-index' || source === 'registry+sparse+https://index.crates.io/') return `version = ${JSON.stringify(`=${pkg.version}`)}`;
    throw new Error(`rust_build_registry_source_unsupported:${name}`);
  };
  let section = ''; let count = 0;
  const lines = manifest.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!; const header = /^\[(.+)\]$/.exec(line);
    if (header) { section = header[1]!; continue; }
    const inline = /^(nautilus-[\w-]+) = \{ git = "[^"]+", rev = "[^"]+", (.*) \}$/.exec(line);
    if (inline) { lines[i] = `${inline[1]} = { ${specification(inline[1]!)}, ${inline[2]} }`; count++; continue; }
    const table = /^(?:dev-)?dependencies\.(nautilus-[\w-]+)$/.exec(section);
    if (table && /^git = "/.test(line)) {
      if (!/^rev = "[^"]+"$/.test(lines[i + 1] ?? '')) throw new Error('rust_build_glue_manifest_source_shape_changed');
      lines[i] = specification(table[1]!).replace(/, (rev|branch|tag) = /, '\n$1 = '); lines[i + 1] = ''; count++;
    }
  }
  if (!count) throw new Error('rust_build_glue_native_declarations_missing');
  return lines.join('\n');
}
