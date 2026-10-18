import type { Problem } from '../../../packages/contracts/src/index.ts';

const API = '/api/v3';
export const authenticationEvents = new EventTarget();
export class ApiError extends Error {
  constructor(message: string, readonly status = 0, readonly code?: string) { super(message); }
}

const errors: Record<string, string> = {
  research_capital_scenarios_invalid: '资本情景须为精确正金额', research_participation_invalid: '成交量参与率须大于 0 且不超过 1（0.01 = 1%）',
  research_liquidity_age_invalid: '请填写正整数流动性数据时差', research_override_unsupported: '存在不支持的研究或容量设置',
  unsupported_evidence: '研究证据不支持', evidence_unavailable: '研究证据不可用',
  authentication_required: '登录已过期', request_id_conflict: '原提交内容不一致',
  dataset_not_ready: '数据集尚未就绪', dataset_not_found: '数据集不存在',
  independent_evidence_missing: '缺少独立评估结果', heldout_observations_missing: '独立样本无有效观测',
  strategy_adapter_not_configured: '数据集未配置组合策略', research_dataset_mismatch: '所选研究的数据不一致',
  strategy_selection_mismatch: '研究评估区间不一致', portfolio_weights_invalid: '权重须为正且合计为 1',
  strategy_policy_not_admitted_with_research: '研究未采用此组合策略', target_expired: '目标已过期',
  target_delivery_requires_reconciliation: '已有目标待核对', account_snapshot_out_of_order: '账户快照顺序异常',
  service_extension_required: '此扩展为必需组件', extension_not_found: '扩展不存在',
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
    const code = typeof problem?.error === 'string' && /^[a-z][a-z0-9_]+$/.test(problem.error) ? problem.error : undefined;
    throw new ApiError(shortError((code && errors[code]) ?? problem?.message ?? fallback), response.status, code);
  }
  if (response.status === 204 || response.headers.get('content-length') === '0') return undefined as T;
  try { return await response.json() as T; }
  catch { throw new ApiError('响应不可用', response.status); }
}
