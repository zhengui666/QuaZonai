import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { Requests } from '../../research/src/documents.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import type { JsonObject } from '@earendil-works/pi-durable';
import { json } from './facts.ts';
import type { ResearchAdmissions } from './facts.ts';

type RequestsValue = { requests: Record<string, { input: unknown; metadata?: JsonObject; sourceEvidence?: unknown; conversationId: number | null }> };
/** UI metadata comes from the same official intake that owns the research identity. */
export function admissionViews(value: RequestsValue | null | undefined): ResearchAdmissions {
  return Object.fromEntries(Object.entries(value?.requests ?? {}).flatMap(([key, request]) => {
    if (request.conversationId === null) return [];
    const metadata = request.metadata ?? {};
    const original = metadata.original;
    return [[key, { original: original && typeof original === 'object' && !Array.isArray(original) ? original : { requestId: key.slice('request:'.length), input: json(request.input) },
      input: json(request.input), ...(metadata.dataset ? { dataset: json(metadata.dataset) } : {}),
      ...(request.sourceEvidence ? { sourceEvidence: json(request.sourceEvidence) } : {}) }]];
  }));
}
export async function readResearchAdmissions(host: ResearchHost) {
  return admissionViews(await host.harness.snapshot(Requests, BACKGROUND_CONTEXT));
}
