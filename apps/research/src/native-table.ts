import { gzipSync, gunzipSync } from 'node:zlib';
import { JSONParser } from './native-json.ts';
import type { JsonObject, TaskId, ConversationId } from '@earendil-works/pi-durable';
import type { JsonValue } from '@earendil-works/chord';

const format = 'qz.native-table/v1';
const pageBytes = 64 * 1024;
export function isNativeTable(value: unknown): value is JsonObject & { rowCount: number; pages: JsonObject[]; conversationId: ConversationId; taskId: TaskId } {
  return !!value && typeof value === 'object' && !Array.isArray(value) && (value as JsonObject).format === format;
}
export function nativeRowCount(value: unknown): number {
  if (Array.isArray(value)) return value.length;
  if (isNativeTable(value)) return value.rowCount;
  throw new Error('native_rows_invalid');
}
/** Receive original canonical rows one at a time into the existing Pi evidence
 * documents. No new execution, store, or financial calculation is introduced. */
export class NativeTableReader {
  private parser = new JSONParser();
  private value: JsonValue | undefined;
  private pending: { key: string; data: string }[] = [];
  private tables = new WeakMap<object, { rows: JsonValue[]; bytes: number; count: number; pages: JsonObject[]; path: string }>();
  private owner: { conversationId: ConversationId; taskId: TaskId };
  private save: (key: string, data: string) => Promise<void>;
  constructor(owner: { conversationId: ConversationId; taskId: TaskId }, save: (key: string, data: string) => Promise<void>) {
    this.owner = owner; this.save = save;
    this.parser.onValue = ({ value, parent, key, stack }) => {
      const keys = stack.map(frame => frame.key).filter(key => key !== undefined);
      // Only canonical report tables are market sized. Metadata, folds,
      // candidates and official daily equity statistics remain unchanged.
      if (Array.isArray(parent) && keys.at(-2) === 'canonical_result') {
        let table = this.tables.get(parent);
        if (!table) { table = { rows: [], bytes: 2, count: 0, pages: [], path: JSON.stringify(keys) }; this.tables.set(parent, table); }
        const row = value as JsonValue;
        const bytes = Buffer.byteLength(JSON.stringify(row)) + 1;
        if (table.rows.length && table.bytes + bytes > pageBytes) this.flush(table);
        table.rows.push(row); table.bytes += bytes; table.count++;
        parent.length = 0;
      } else if (Array.isArray(value) && keys.at(-1) === 'canonical_result') {
        const table = this.tables.get(value);
        if (table) {
          if (table.pages.length) {
            this.flush(table);
            (parent as JsonObject)[String(key)] = { format, ...this.owner, rowCount: table.count, pages: table.pages };
          } else (parent as JsonObject)[String(key)] = table.rows;
        }
      }
      if (!parent) this.value = value as JsonValue;
    };
  }
  private flush(table: { rows: JsonValue[]; bytes: number; count: number; pages: JsonObject[]; path: string }) {
    if (!table.rows.length) return;
    const key = `${this.owner.taskId}:table:${table.path}:${table.pages.length}`;
    table.pages.push({ key, start: table.count - table.rows.length, count: table.rows.length });
    this.pending.push({ key, data: gzipSync(JSON.stringify(table.rows)).toString('base64') });
    table.rows = []; table.bytes = 2;
  }
  async write(text: string) {
    // Bound pending writes even when an HTTP implementation returns a large chunk.
    for (let start = 0; start < text.length;) {
      let end = Math.min(text.length, start + 16 * 1024);
      if (end < text.length && /[\uD800-\uDBFF]/.test(text[end - 1]!)) end++;
      this.parser.write(text.slice(start, end)); start = end;
      for (const part of this.pending.splice(0)) await this.save(part.key, part.data);
    }
  }
  async finish(): Promise<JsonValue> {
    if (!this.parser.isEnded) this.parser.end();
    for (const part of this.pending.splice(0)) await this.save(part.key, part.data);
    if (this.value === undefined) throw new Error('native_json_value_missing');
    return this.value;
  }
}

/** Load only the persisted pages intersecting one requested row interval. */
export async function readNativeTable(value: JsonObject, offset: number, count: number, load: (key: string) => Promise<string>): Promise<JsonValue[]> {
  if (!isNativeTable(value) || !Number.isSafeInteger(offset) || offset < 0 || offset > value.rowCount) throw new Error('native_rows_invalid');
  const rows: JsonValue[] = [];
  // One stored page per response keeps a request for a billion rows bounded.
  const page = value.pages.find(page => Number(page.start) + Number(page.count) > offset);
  if (!page || count === 0) return rows;
  const data = JSON.parse(gunzipSync(Buffer.from(await load(String(page.key)), 'base64')).toString('utf8')) as JsonValue[];
  if (!Array.isArray(data) || data.length !== page.count) throw new Error('native_evidence_table_page_invalid');
  return data.slice(offset - Number(page.start), Math.min(data.length, offset - Number(page.start) + count));
}
