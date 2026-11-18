import type { Schema } from '@earendil-works/pi-schema';

/** Provider-facing projection only. Runtime validators must compile the original
 * generated schema: JavaScript `$` can also match before a final line break.
 * Replace only the generated scalar end assertion rejected by model providers;
 * preserve numeric ranges and do not visit examples/defaults or arbitrary data. */
export function nativeToolSchema<T>(schema: Schema<T>): Schema<T> {
  const end = String.raw`(?![\s\S])`;
  const childSchemas = new Set(['items', 'additionalProperties', 'not', 'contains', 'propertyNames', 'if', 'then', 'else']);
  const schemaMaps = new Set(['properties', 'patternProperties', '$defs', 'definitions', 'dependentSchemas']);
  const schemaArrays = new Set(['allOf', 'anyOf', 'oneOf', 'prefixItems']);
  const project = (value: unknown): unknown => {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return value;
    return Object.fromEntries(Object.entries(value).map(([key, item]) => {
      if (key === 'pattern' && typeof item === 'string' && item.endsWith(end)) return [key, `${item.slice(0, -end.length)}$`];
      if (childSchemas.has(key)) return [key, Array.isArray(item) ? item.map(project) : project(item)];
      if (schemaArrays.has(key) && Array.isArray(item)) return [key, item.map(project)];
      if (schemaMaps.has(key) && item && typeof item === 'object' && !Array.isArray(item)) return [key, Object.fromEntries(Object.entries(item).map(([name, child]) => [name, project(child)]))];
      return [key, item];
    }));
  };
  return project(schema) as Schema<T>;
}
