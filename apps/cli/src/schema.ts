import { readFile } from 'node:fs/promises';
import { CliError } from './profile.ts';

// Read the actual installed source/native schemas, not a hand-maintained mirror.
// Packed assets come directly from source at prepack; source-tree uses live DTOs.
async function installed(source: string, packed: string) {
  try { return await readFile(new URL(source, import.meta.url), 'utf8'); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    return readFile(new URL(packed, import.meta.url), 'utf8');
  }
}
export async function schemas(name?: string, domain = false) {
  try {
    if (!domain) {
      const source = await installed('../../../packages/contracts/src/index.ts', '../schemas/contracts.ts.txt');
      const names = [...source.matchAll(/export (?:interface|type) (\w+)/g)].map((match) => match[1]!);
      if (name && !names.includes(name)) throw new CliError('schema_not_found');
      return name ? { format: 'typescript', entry: name, source } : { format: 'typescript', names };
    }
    const document = JSON.parse(await installed('../../../contracts/generated/domain-v1.openapi.json', '../schemas/domain-v1.openapi.json'));
    const all = document.components.schemas as Record<string, unknown>;
    if (!name) return { format: 'json-schema', names: Object.keys(all) };
    if (!(name in all)) throw new CliError('schema_not_found');
    const selected: Record<string, unknown> = {};
    const pending = [name];
    while (pending.length) {
      const current = pending.pop()!;
      if (current in selected) continue;
      selected[current] = all[current];
      for (const ref of JSON.stringify(all[current]).matchAll(/"\$ref":"#\/components\/schemas\/([^"]+)"/g)) pending.push(ref[1]!);
    }
    return { format: 'json-schema', schema: all[name], components: { schemas: selected } };
  } catch (error) {
    if (error instanceof CliError) throw error;
    throw new CliError('installed_schema_unavailable');
  }
}
