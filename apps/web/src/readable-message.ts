const unavailable = '结构化详情无法显示，请核对操作结果。';
const labels: Record<string, string> = {
  message: '说明', detail: '详情', error: '错误', errors: '错误', code: '错误代码',
  field: '字段', request_id: '请求编号', current_revision: '当前版本',
};

function summary(value: unknown, depth: number): string {
  if (depth > 12) return unavailable;
  if (value === null) return '未提供';
  if (typeof value === 'boolean') return value ? '是' : '否';
  if (typeof value === 'string') return format(value, depth + 1);
  if (Array.isArray(value)) return value.length ? value.map(item => summary(item, depth + 1)).join('、') : '无项目';
  if (typeof value === 'object') {
    const entries = Object.entries(value);
    return entries.length ? entries.map(([key, item]) => `${Object.hasOwn(labels, key) ? labels[key] : format(key, depth + 1)}：${summary(item, depth + 1)}`).join('；') : '无字段';
  }
  return String(value);
}

function parseExact(text: string): unknown {
  // Validate before converting numeric tokens to text so malformed numbers
  // cannot become accepted strings. Keep diagnostic counters and decimals exact.
  JSON.parse(text);
  const exact = text.replace(/"(?:\\[\s\S]|[^"\\])*"|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/g,
    (token, number: string | undefined) => number === undefined ? token : `"${number}"`);
  return JSON.parse(exact);
}

function structured(text: string, depth: number): string {
  try { return summary(parseExact(text), depth + 1); }
  catch {
    const lines = text.split(/\r?\n/).map(line => line.trim()).filter(Boolean);
    if (lines.length < 2) return unavailable;
    try { return lines.map(line => summary(parseExact(line), depth + 1)).join('；'); }
    catch { return unavailable; }
  }
}

function blockEnd(text: string, start: number): number | undefined {
  const stack: string[] = [];
  let quoted = false; let escaped = false;
  for (let index = start; index < text.length; index += 1) {
    const character = text[index];
    if (quoted) {
      if (escaped) escaped = false;
      else if (character === '\\') escaped = true;
      else if (character === '"') quoted = false;
    } else if (character === '"') quoted = true;
    else if (character === '{' || character === '[') stack.push(character);
    else if (character === '}' || character === ']') {
      if (stack.pop() !== (character === '}' ? '{' : '[')) return undefined;
      if (!stack.length) return index + 1;
    }
  }
  return undefined;
}

function quotedText(message: string, depth: number): string {
  return message.replace(/"(?:\\[\s\S]|[^"\\])*"/g, token => {
    try {
      const decoded: unknown = JSON.parse(token);
      return typeof decoded === 'string' && /^(?:\{|\[|```)/.test(decoded.trim())
        ? format(decoded, depth + 1) : token;
    } catch { return token; }
  });
}

function format(message: string, depth: number): string {
  if (depth > 12) return unavailable;
  const trimmed = message.trim();
  if (trimmed.startsWith('"') && trimmed.endsWith('"')) {
    try {
      const decoded: unknown = JSON.parse(trimmed);
      if (typeof decoded === 'string') return format(decoded, depth + 1);
    } catch { /* Continue with ordinary text and embedded blocks. */ }
  }
  const fenced = message.replace(/```(?:(json|jsonl|ndjson)\b)?[ \t]*\r?\n?([\s\S]*?)(?:```|$)/gi,
    (block: string, language: string | undefined, contents: string) => language || /^[{["]/.test(contents.trim())
      ? structured(contents.trim(), depth) : block);
  let result = ''; let cursor = 0;
  while (cursor < fenced.length) {
    const rest = fenced.slice(cursor);
    const candidate = rest.search(/\{\s*(?:"|\})|\[\s*(?:"|\{|\[|-?\d|true\b|false\b|null\b|\])/);
    if (candidate < 0) return result + quotedText(rest, depth);
    const start = cursor + candidate;
    result += quotedText(fenced.slice(cursor, start), depth);
    const end = blockEnd(fenced, start);
    if (end === undefined) return result + unavailable;
    result += structured(fenced.slice(start, end), depth);
    cursor = end;
  }
  return result;
}

/** Present server diagnostics as prose without exposing serialized payloads. */
export function readableMessage(message: string): string {
  return format(message, 0);
}
