import type { Problem } from '../../../packages/contracts/src/index.ts';

const API = '/api/v3';
export const authenticationEvents = new EventTarget();
export class ApiError extends Error {
  constructor(message: string, readonly status = 0, readonly code?: string) { super(message); }
}

const errors: Record<string, string> = {
  instance_settings_invalid: '实例配置格式不正确，请检查后重试',
  instance_settings_revision_conflict: '配置已在其他页面更新，请载入最新配置后重新编辑',
  instance_settings_save_failed: '配置未能保存，请核对重试',
  nautilus_configuration_invalid: '实例配置格式不正确', nautilus_backtest_instance_invalid: '请填写有效的回测实例标识与地址',
  nautilus_url_required: '请填写实例服务地址', nautilus_url_invalid: '实例地址须为 HTTP 或 HTTPS，且不含密码、查询参数或片段',
  nautilus_token_env_invalid: '凭据须填写服务端环境变量名称',
  nautilus_connections_invalid: '账户连接配置不正确', nautilus_connection_invalid: '请完整填写账户连接信息',
  nautilus_connection_id_conflict: '实例标识重复，请为每个账户连接使用不同标识',
  native_remote_execution_unsettled: '远端科学执行的原始回执仍未确认终态，暂时不能删除；请恢复原连接后按同一身份核对',
  mandate_deleted: '所属投资决策已删除，不能恢复其研究',
  research_delete_tasks_active: '请先停止研究并等待原始任务结束，再删除',
  research_delete_portfolio_in_use: '研究仍被组合使用，请先删除相关组合',
  research_delete_pm_in_use: '研究仍被投资决策使用，请先处理相关投资决策',
  factor_delete_portfolio_in_use: '因子仍被组合使用，请先删除相关组合',
  factor_delete_pm_in_use: '因子仍被投资决策使用，请先处理相关投资决策',
  portfolio_delete_tasks_active: '组合仍在计算，请等待原始任务结束后删除',
  portfolio_delete_observation_in_use: '组合仍有活动观测或待核对回执，请先停止观测并完成对账',
  pm_delete_requires_retirement: '请先退役当前策略，再删除投资决策',
  pm_delete_external_obligations: '投资决策仍有目标或账户对账责任，请先完成对账',
  pm_delete_tasks_active: '投资决策仍有活动任务，请等待原始任务结束后删除',
  research_deleted: '研究已删除', factor_deleted: '因子已删除', portfolio_deleted: '组合已删除',

  research_capital_scenarios_invalid: '资本情景须为精确正金额：小于 10²⁰、小数至多 18 位、原文至多 64 字符', research_participation_invalid: '成交量参与率须大于 0 且不超过 1（0.01 = 1%）',
  research_liquidity_age_invalid: '请填写正整数流动性数据时差', research_override_unsupported: '存在不支持的研究或容量设置',
  research_evaluation_unsupported: '当前原生评估仅支持最大化已注册指标', research_policy_unregistered: '结构化研究必须继承注册数据集的原始科学策略',
  account_reservation_unknown: '账户预留未知，不能交付新目标',
  unsupported_evidence: '研究证据不支持', evidence_unavailable: '研究证据不可用',
  authentication_required: '登录已过期', request_id_conflict: '原提交内容不一致',
  dataset_not_ready: '数据集尚未就绪', dataset_not_found: '数据集不存在',
  independent_evidence_missing: '缺少独立评估结果', heldout_observations_missing: '独立样本无有效观测',
  research_criterion_insufficient: '研究请求的独立指标不可用，不能用于组合', research_criterion_not_met: '独立评估未达到研究标准',
  strategy_adapter_not_configured: '数据集未配置组合策略', research_dataset_mismatch: '所选研究的数据不一致',
  strategy_selection_mismatch: '研究评估区间不一致', portfolio_weights_invalid: '权重须为正且合计为 1',
  strategy_policy_not_admitted_with_research: '研究未采用此组合策略', target_expired: '目标已过期',
  target_delivery_requires_reconciliation: '已有目标待核对', account_snapshot_out_of_order: '账户快照顺序异常', account_snapshot_conflict: '同一账户序列的快照事实冲突',
  service_extension_required: '此扩展为必需组件', extension_not_found: '扩展不存在',
  unauthorized: '请重新登录', invalid_password: '密码不正确',
  conflict: '操作冲突', not_found: '记录不存在', unavailable: '服务不可用',
  model_unavailable: '模型不可用，请连接服务并刷新目录', thinking_level_unsupported: '该模型不支持所选推理强度', service_tier_unsupported: '该模型不支持所选速度',
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

type Options = { method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE'; body?: unknown; rawBody?: string; signal?: AbortSignal };
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
