import type { Problem } from '../../../packages/contracts/src/index.ts';

const API = '/api/v3';
export const authenticationEvents = new EventTarget();
export class ApiError extends Error {
  constructor(message: string, readonly status = 0) { super(message); }
}

const errors: Record<string, string> = {
  unsupported_evidence: '研究证据不支持', evidence_unavailable: '研究证据不可用',
  unauthorized: '请重新登录', invalid_password: '密码不正确',
  conflict: '操作冲突', not_found: '记录不存在', unavailable: '服务不可用',
};
export function shortError(error: unknown): string {
  const text = error instanceof Error ? error.message : typeof error === 'string' ? error : '';
  if (errors[text]) return errors[text];
  if (!text || /[{}\[\]<>]/.test(text)) return '请求失败';
  return text.split('\n')[0]!.slice(0, 72);
}

type Options = { method?: 'GET' | 'POST' | 'PATCH' | 'DELETE'; body?: unknown; rawBody?: string; signal?: AbortSignal };
export async function api<T>(path: string, options: Options = {}): Promise<T> {
  let response: Response;
  const body = options.rawBody ?? (options.body === undefined ? undefined : JSON.stringify(options.body));
  try {
    response = await fetch(`${API}${path}`, {
      method: options.method ?? 'GET', credentials: 'same-origin', cache: 'no-store',
      headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
      body, signal: options.signal,
    });
  } catch (error) {
    if (options.signal?.aborted) throw error;
    throw new ApiError('连接失败');
  }
  if (!response.ok) {
    if (response.status === 401 && !path.startsWith('/session')) authenticationEvents.dispatchEvent(new Event('required'));
    const problem: Problem | undefined = await response.json().catch(() => undefined);
    const fallback = response.status === 401 ? '请重新登录' : response.status === 404 ? '服务或记录不存在' : `请求失败 · ${response.status}`;
    throw new ApiError(shortError(problem?.message ?? (problem?.error && errors[problem.error]) ?? fallback), response.status);
  }
  if (response.status === 204 || response.headers.get('content-length') === '0') return undefined as T;
  try { return await response.json() as T; }
  catch { throw new ApiError('响应不可用', response.status); }
}
