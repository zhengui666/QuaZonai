import type {
  AnswerIdeaDraftRequest,
  ApiErrorEnvelope,
  CreateIdeaDraftRequest,
  IdeaDraft,
  ResearchProgram,
  StartIdeaDraftRequest,
  UUID,
} from './types';

export type ApiFailure =
  | { kind: 'api'; message: string }
  | { kind: 'http'; status: number }
  | { kind: 'network' }
  | { kind: 'decode' }
  | { kind: 'contract'; message: string };

export class ApiError extends Error {
  readonly failure: ApiFailure;
  readonly code?: string;
  readonly status: number;
  readonly details?: Record<string, unknown>;

  constructor(
    failure: ApiFailure,
    status: number,
    code?: string,
    details?: Record<string, unknown>,
    diagnosticMessage?: string,
  ) {
    super('message' in failure ? failure.message : diagnosticMessage ?? failure.kind);
    this.name = 'ApiError';
    this.failure = failure;
    this.status = status;
    this.code = code ?? (failure.kind === 'api' || failure.kind === 'http' ? 'HTTP_ERROR' : undefined);
    this.details = details;
  }
}

export interface RequestOptions extends RequestInit {
  idempotent?: boolean;
}

function isBodyJson(body: BodyInit | null | undefined): body is string {
  return typeof body === 'string';
}

export async function apiRequest<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const headers = new Headers(options.headers);
  if (isBodyJson(options.body) && !headers.has('Content-Type')) headers.set('Content-Type', 'application/json');
  if (options.idempotent && !headers.has('Idempotency-Key')) headers.set('Idempotency-Key', crypto.randomUUID());

  let response: Response;
  try {
    response = await fetch(path, { ...options, headers, credentials: 'same-origin' });
  } catch (error) {
    const diagnosticMessage = error instanceof Error ? error.message : typeof error === 'string' ? error : undefined;
    throw new ApiError({ kind: 'network' }, 0, undefined, undefined, diagnosticMessage);
  }
  if (!response.ok) {
    if (response.status === 401 && !path.startsWith('/api/v1/auth/') && typeof window !== 'undefined') {
      window.dispatchEvent(new Event('quazonai:auth-required'));
    }
    let envelope: ApiErrorEnvelope = {};
    try {
      envelope = await response.json() as ApiErrorEnvelope;
    } catch {
      // A non-JSON error response is represented by the HTTP status below.
    }
    const message = envelope.error?.message;
    const failure: ApiFailure = message === undefined
      ? { kind: 'http', status: response.status }
      : { kind: 'api', message };
    throw new ApiError(failure, response.status, envelope.error?.code, envelope.error?.details);
  }
  if (response.status === 204 || response.status === 205) return undefined as T;
  try {
    return await response.json() as T;
  } catch (error) {
    const diagnosticMessage = error instanceof Error ? error.message : typeof error === 'string' ? error : undefined;
    throw new ApiError({ kind: 'decode' }, response.status, undefined, undefined, diagnosticMessage);
  }
}

export function jsonBody(value: unknown): string {
  return JSON.stringify(value);
}

export function normalizeList<T>(value: T[] | { items?: T[]; data?: T[] } | null | undefined): T[] {
  if (Array.isArray(value)) return value;
  if (value && typeof value === 'object') {
    const envelope = value as { items?: unknown; data?: unknown };
    if (Array.isArray(envelope.items)) return envelope.items as T[];
    if (Array.isArray(envelope.data)) return envelope.data as T[];
  }
  throw new ApiError(
    { kind: 'contract', message: 'Expected a list response with an items or data array.' },
    0,
    'CONTRACT_MISMATCH',
  );
}

export const createIdeaDraft = (payload: CreateIdeaDraftRequest) => apiRequest<IdeaDraft>(
  '/api/v1/idea-drafts',
  { method: 'POST', body: jsonBody(payload), idempotent: true },
);

export const answerIdeaDraft = (id: UUID, payload: AnswerIdeaDraftRequest) => apiRequest<IdeaDraft>(
  `/api/v1/idea-drafts/${id}/answers`,
  { method: 'POST', body: jsonBody(payload), idempotent: true },
);

export const startIdeaDraft = (id: UUID, payload: StartIdeaDraftRequest) => apiRequest<ResearchProgram>(
  `/api/v1/idea-drafts/${id}/start`,
  { method: 'POST', body: jsonBody(payload), idempotent: true },
);
