import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import type { JsonValue } from '@earendil-works/chord';
import type { ConversationId, JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { ResearchHost } from '../../research/src/host.ts';
import { Study } from '../../research/src/documents.ts';
import { nativeResultView, nativeToolMaxBytes } from '../../research/src/native-result-view.ts';
import { isNativeTable, readNativeTable } from '../../research/src/native-table.ts';
import { NativeEvidencePart } from '../../research/plugins/native-evidence.ts';
import { nativeExposurePage } from '../../research/plugins/research-assessment.ts';
import type { NativeResultTablePage, NativeResultTableReference } from '../../../packages/contracts/src/native-result-tables.ts';
import { object, Problem } from './facts.ts';

/** Uses existing compact references, including retained fold/trial results. */
export function nativeResultTableReferences(result: JsonObject, taskId: number): NativeResultTableReference[] {
  const references: NativeResultTableReference[] = [];
  const visit = (value: JsonValue) => {
    if (!value || typeof value !== 'object') return;
    if (!Array.isArray(value) && value.source === 'UPSTREAM_CANONICAL_REPORT_REFERENCE') {
      for (const [name, table] of Object.entries(object(value.tables, 'native_tables'))) {
        const item = object(table, 'native_table');
        references.push({ name, path: String(item.path), rowCount: Number(item.rowCount), resultPath: String(value.resultPath) });
      }
      return;
    }
    for (const child of Object.values(value)) visit(child);
  };
  visit(nativeResultView(result, taskId));
  return references;
}

/** Fixed HTTP transport budget, not a model context or a total evidence limit. */
export function nativeWebTablePage(rows: JsonValue[], taskId: number, path: string | undefined, offset: number, pageSize: number): NativeResultTablePage {
  let selected: JsonObject;
  try {
    selected = nativeExposurePage(rows, { taskId, path: path ?? '$.tables', offset, pageSize }, nativeToolMaxBytes * 4, 512);
  } catch (error) {
    if (error instanceof Error && error.message.startsWith('assessment_evidence_row_exceeds_model_page_budget:')) throw new Problem('native_table_row_exceeds_page_budget', 422);
    throw error;
  }
  return { taskId, kind: path === undefined ? 'TABLES' : 'ROWS', path: path ?? null,
    offset, endExclusive: Number(selected.endExclusive), totalRows: rows.length,
    nextOffset: selected.nextOffset === null ? null : Number(selected.nextOffset),
    maxPayloadBytes: nativeToolMaxBytes, scope: path === undefined ? 'ORIGINAL_TASK_TABLES' : 'ORIGINAL_TASK_ROWS', rows: selected.rows as JsonValue[] };
}

export async function researchNativeTables(host: ResearchHost, id: number, taskId: number, path: string | undefined, offset = 0, pageSize = 200): Promise<NativeResultTablePage> {
  if (![id, taskId, offset, pageSize].every(Number.isSafeInteger) || id < 1 || taskId < 1 || offset < 0 || pageSize < 1 || pageSize > 1000
    || path !== undefined && (!path || path.length > 2048)) throw new Problem('native_table_page_invalid', 422);
  return host.harness.commit(async tx => {
    const source = await tx.task(taskId as TaskId);
    if (!source || source.conversationId !== id || source.kind !== 'qz.native-strategy.backtest') throw new Problem('research_evidence_not_found', 404);
    const study = await tx.doc(Study, id as ConversationId);
    if (!study.input || study.deletedAt) throw new Problem('research_not_found', 404);
    if (source.state.status !== 'terminal' || source.state.outcome.status !== 'completed') throw new Problem('native_result_not_completed', 409);
    const result = object(source.state.outcome.result, 'native_result');
    const references = nativeResultTableReferences(result, taskId);
    if (path === undefined) {
      if (offset > references.length) throw new Problem('native_table_page_invalid', 422);
      return nativeWebTablePage(references as unknown as JsonValue[], taskId, undefined, offset, pageSize);
    }
    if (!references.some(reference => reference.path === path)) throw new Problem('native_table_path_invalid', 422);
    let selected: JsonValue = result;
    for (const key of path.replace(/^\$\.?/, '').replace(/\[(\d+)\]/g, '.$1').split('.')) {
      if (!selected || typeof selected !== 'object' || !Object.hasOwn(selected, key)) throw new Problem('native_table_path_invalid', 422);
      selected = (selected as Record<string, JsonValue>)[key]!;
    }
    if (isNativeTable(selected)) {
      const reference = selected;
      if (reference.conversationId !== id) throw new Problem('research_evidence_not_found', 404);
      // Existing parsers can assign table parts to the backtest owner or its original
      // native execution child. Never read another task's parts merely by key.
      if (reference.taskId !== taskId) {
        const producer = await tx.task(reference.taskId);
        if (!producer || producer.conversationId !== id || producer.owner !== taskId || producer.kind !== 'qz.native.execution') throw new Problem('research_evidence_not_found', 404);
      }
      if (offset > reference.rowCount) throw new Problem('native_table_page_invalid', 422);
      const rows = await readNativeTable(reference, offset, pageSize, async key => {
        if (!key.startsWith(`${reference.taskId}:table:`)) throw new Problem('native_table_reference_invalid', 409);
        const doc = await tx.doc(NativeEvidencePart, reference.conversationId, key, null);
        if (!doc.data) throw new Problem('native_evidence_part_missing', 409);
        return doc.data;
      });
      if (!rows.length && offset < reference.rowCount) throw new Problem('native_evidence_table_page_unavailable', 409);
      const page = nativeWebTablePage(rows, taskId, path, 0, pageSize);
      const endExclusive = offset + page.endExclusive;
      return { ...page, offset, endExclusive, totalRows: reference.rowCount, nextOffset: endExclusive < reference.rowCount ? endExclusive : null };
    }
    if (!Array.isArray(selected) || offset > selected.length) throw new Problem('native_table_page_invalid', 422);
    return nativeWebTablePage(selected, taskId, path, offset, pageSize);
  }, context);
}
