import createClient from 'openapi-fetch';
import type { components, paths } from './generated/api';
import { responseKind, validateProblem, validateResponse } from './generated/responses.cjs';

export type Schema = components['schemas'];
export type Problem = Schema['Problem'];
export const AUTH_CHANGED = 'quazonai-auth-changed';
export const REAUTH_REQUIRED = 'quazonai-reauth-required';

export class ApiFailure extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly status = 0,
    readonly problem?: Problem,
    readonly retryAt = 0,
  ) {
    super(message);
    this.name = 'ApiFailure';
  }
}
export function retryAt(value: string | null, now = Date.now()): number {
  if (value === null) return 0;
  const timestamp = /^\d+$/.test(value) ? now + Number(value) * 1000 : Date.parse(value);
  return Number.isFinite(timestamp) && timestamp > now ? timestamp : 0;
}
export async function responseFailure(response: Response, schemaPath?: string, method = 'GET'): Promise<ApiFailure> {
  const contentType = response.headers.get('content-type');
  const media = contentType?.split(';', 1)[0]?.trim().toLowerCase();
  const declared = media === 'application/problem+json' && response.status >= 400
    && (schemaPath === undefined || responseKind(schemaPath, method, response.status, contentType) === 'json');
  let value: unknown;
  if (declared) {
    try { value = await response.json(); } catch { value = undefined; }
  }
  if (declared && validateProblem(value) && value.status === response.status
    && (schemaPath === undefined || validateResponse(schemaPath, method, response.status, value, contentType))) {
    return new ApiFailure(value.code, value.detail, response.status, value, retryAt(response.headers.get('retry-after')));
  }
  return new ApiFailure('HTTP_CONTRACT_ERROR', `服务返回了无法识别的响应（HTTP ${response.status}）。未将它当成空列表或成功结果。`, response.status);
}

export function makeClient(baseUrl: string, fetcher: typeof fetch = fetch) {
  const origin = new URL(baseUrl).origin;
  const client = createClient<paths>({
    baseUrl: origin, credentials: 'same-origin', cache: 'no-store', redirect: 'error',
    fetch: fetcher,
  });
  client.use({
    onRequest({ request }) {
      if (new URL(request.url).origin !== origin) {
        throw new ApiFailure('INVALID_ORIGIN', '拒绝向不同来源发送业务请求。');
      }
      if (typeof navigator !== 'undefined' && navigator.onLine === false && !['GET', 'HEAD'].includes(request.method)) {
        throw new ApiFailure('OFFLINE', '当前离线，操作未提交。恢复连接后请手动确认并提交。');
      }
      return request;
    },
    async onResponse({ response, request, schemaPath }) {
      if (response.ok) {
        const kind = responseKind(schemaPath, request.method, response.status, response.headers.get('content-type'));
        if (kind === undefined) {
          throw new ApiFailure('HTTP_CONTRACT_ERROR', '响应状态或媒体类型不符合已生成的接口合同。', response.status);
        }
        // Only a declared operation/status/media combination may retain its body.
        // openapi-fetch, not this middleware, owns parseAs for bytes and streams.
        if (kind === 'binary' || kind === 'event-stream') return response;
        let value: unknown;
        if (kind === 'json') {
          try { value = await response.clone().json(); }
          catch { throw new ApiFailure('HTTP_CONTRACT_ERROR', '服务没有返回合同规定的 JSON 响应。'); }
        }
        if (!validateResponse(schemaPath, request.method, response.status, value, response.headers.get('content-type'))) {
          throw new ApiFailure('HTTP_CONTRACT_ERROR', '响应字段或合同版本不兼容。未将它当成空列表或成功操作。');
        }
        return response;
      }
      const failure = await responseFailure(response, schemaPath, request.method);
      if (typeof window !== 'undefined') {
        if (failure.code === 'AUTH_REQUIRED' && new URL(request.url).pathname !== '/api/v2/auth/session') {
          window.dispatchEvent(new Event(AUTH_CHANGED));
        }
        if (failure.code === 'RECENT_AUTH_REQUIRED') window.dispatchEvent(new Event(REAUTH_REQUIRED));
      }
      throw failure;
    },
    onError({ error }) {
      if (error instanceof ApiFailure) return error;
      if (error instanceof Error && error.name === 'AbortError') return error;
      return new ApiFailure('NETWORK_UNKNOWN', '连接中断，尚不能确定操作是否已提交。不要重复创建新操作；重试将沿用原幂等键。');
    },
  });
  return client;
}
export const api = makeClient(typeof window === 'undefined' ? 'http://localhost' : window.location.origin);

export function dataOf<T>(result: { data?: T }): T {
  if (result.data === undefined) throw new ApiFailure('HTTP_CONTRACT_ERROR', '成功响应缺少合同规定的数据。');
  return result.data;
}

/** One in-memory intent per form. Never persists credentials, form data or keys. */
export class Intent {
  private serialized: string | undefined;
  private key: string | undefined;
  headers(method: string, path: string, body: unknown): { 'Idempotency-Key': string } {
    const serialized = JSON.stringify([method, path, body]);
    if (serialized !== this.serialized || this.key === undefined) {
      this.serialized = serialized;
      this.key = crypto.randomUUID();
    }
    return { 'Idempotency-Key': this.key };
  }
  clear() { this.serialized = undefined; this.key = undefined; }
}

export function isCounter(value: string, positive = false): boolean {
  return /^(0|[1-9][0-9]{0,18})(?![\s\S])/.test(value)
    && BigInt(value) <= 9223372036854775807n && (!positive || value !== '0');
}
export function isDecimal(value: string): boolean {
  if (value.length > 64 || !/^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?![\s\S])/.test(value)) return false;
  const [integer = '', fraction = ''] = value.replace(/^-/, '').split('.');
  return integer.length <= 20 && fraction.length <= 18;
}
export function displayTime(value: string | null | undefined): string {
  if (value === null || value === undefined) return '尚无记录';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '时间合同不兼容' : date.toLocaleString('zh-CN', { hour12: false });
}
export function terminal(state: Schema['RunState']): boolean {
  return state === 'SUCCEEDED' || state === 'FAILED' || state === 'CANCELLED';
}
