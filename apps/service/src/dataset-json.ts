/** Native serde rejects duplicate fields: a JS handoff must not keep the last
 * duplicate silently before comparing the original preparation/request. */
export function parseOriginalJson(contents: string): unknown {
  let value: unknown;
  try { value = JSON.parse(contents); } catch { throw new Error('dataset_input_invalid_json'); }
  const stack: (Set<string> | null)[] = [];
  for (const token of contents.matchAll(/"(?:[^"\\]|\\.)*"(\s*:)?|[{}\[\]]/g)) {
    if (token[0] === '{') stack.push(new Set());
    else if (token[0] === '[') stack.push(null);
    else if (token[0] === '}' || token[0] === ']') stack.pop();
    else if (token[1]) {
      const key = JSON.parse(token[0].slice(0, -token[1].length)) as string;
      const fields = stack.at(-1)!;
      if (fields?.has(key)) throw new Error('dataset_input_duplicate_json_key');
      fields?.add(key);
    }
  }
  return value;
}
