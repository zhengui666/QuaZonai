import type { ApiErrorEnvelope } from './types';

export class ApiError extends Error {
  readonly code: string;
  readonly status: number;
  readonly details?: Record<string, unknown>;

  constructor(message: string, status: number, code = 'HTTP_ERROR', details?: Record<string, unknown>) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
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

  const response = await fetch(path, { ...options, headers, credentials: 'same-origin' });
  if (!response.ok) {
    let envelope: ApiErrorEnvelope = {};
    try {
      envelope = await response.json() as ApiErrorEnvelope;
    } catch {
      // A non-JSON error response is represented by the HTTP status below.
    }
    throw new ApiError(
      envelope.error?.message ?? `Request failed with HTTP ${response.status}`,
      response.status,
      envelope.error?.code,
      envelope.error?.details,
    );
  }
  if (response.status === 204) return undefined as T;

  const contentType = response.headers.get('content-type') ?? '';
  if (!contentType.includes('application/json')) return await response.text() as T;
  return await response.json() as T;
}

export function jsonBody(value: unknown): string {
  return JSON.stringify(value);
}

export function normalizeList<T>(value: T[] | { items?: T[]; data?: T[] } | null | undefined): T[] {
  if (Array.isArray(value)) return value;
  return value?.items ?? value?.data ?? [];
}
