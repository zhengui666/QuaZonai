import { readFile } from 'node:fs/promises';

// Source of truth is the generated official-execution contract, not a second version pin.
const executionSchemas = JSON.parse(await readFile(new URL('../../../contracts/generated/domain-v1.openapi.json', import.meta.url), 'utf8')).components.schemas;
type ExecutionVariant = { properties: { adapter_kind: { enum: string[] }; upstream_version?: { enum: string[] } } };
export const executionVersions = new Set<string>((executionSchemas.NativeModelRefV1.oneOf as ExecutionVariant[])
  .filter(variant => variant.properties.adapter_kind.enum[0]?.startsWith('NAUTILUS_'))
  .flatMap(variant => variant.properties.upstream_version?.enum ?? []));

