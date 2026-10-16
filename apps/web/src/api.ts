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
  model_unavailable: '模型不可用，请连接服务并刷新目录', thinking_level_unsupported: '该模型不支持所选推理强度',
  model_login_in_progress: '此服务已有连接操作，请刷新后继续', model_login_not_found: '连接操作已结束，请重新连接',
  model_login_cancelled: '连接已取消', model_prompt_changed: '授权步骤已更新，请重试',
  model_login_failed: '模型服务连接失败', model_logout_failed: '退出未确认，请重试',
  provider_status_unavailable: '模型服务状态不可用', settings_field_not_mutable: '设置仅支持模型配置',
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
