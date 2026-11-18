import i18n from './i18n';
import type { Problem } from '../../../packages/contracts/src/index.ts';

const API = '/api/v3';
export const authenticationEvents = new EventTarget();
export class ApiError extends Error {
  constructor(message: string, readonly status = 0, readonly code?: string) { super(message); }
}

const errors: Record<string, string> = {
  connection_failed: "Connection failed.", response_unreadable: "The response could not be read.",
  language_unsupported: "Choose a supported language.",
  remote_account_local_origin_required: "Remote account management requires the configured local, same-origin session.",
  remote_account_target_not_allowed: "This remote service is not authorized by the deployment.",
  remote_account_probe_stale: "The probe expired or the session changed. Run a new probe explicitly.",
  remote_account_binding_changed: "The remote session changed. Keep the original request; do not rebind it automatically.",
  remote_account_binding_superseded: "The previous binding is retained in history. Select a new session you have explicitly accepted.",
  remote_account_history_requires_reconciliation: "Existing account history must be reconciled before a replacement binding can be created.",
  remote_account_successor_invalid: "The new session cannot replace the selected binding.",
  remote_account_outcome_unknown: "The remote outcome is unknown. Keep the original request and check its receipt; do not resend it.",
  capital_exit_amount_invalid: "Enter an exact positive amount.",
  capital_exit_policy_unsupported: "This account does not support that capital-release method. Refresh the planning inputs.",
  capital_exit_deadline_invalid: "Enter a deadline in the future.",
  capital_exit_legs_required: "Select at least one existing position that can be reduced.",
  capital_exit_instrument_invalid: "Select distinct existing positions that can be reduced.",
  capital_exit_quantity_invalid: "Enter an exact positive reduction quantity.",
  capital_exit_price_invalid: "Enter an exact positive minimum sell price.",
  capital_exit_quantity_increment_invalid: "The reduction must not exceed the reducible position and must respect the native quantity increment.",
  capital_exit_price_increment_invalid: "The minimum sell price must respect the native price increment.",
  capital_exit_interval_invalid: "The batch interval must be a whole number of at least 5 seconds.",
  capital_exit_participation_invalid: "Bid participation must be greater than 0 and no more than 0.1.",
  capital_exit_cost_invalid: "Enter an exact, nonnegative economic-loss limit.",
  capital_exit_slice_invalid: "Enter an exact positive amount per batch. Preview again if it is below the minimum notional.",
  capital_exit_context_expired: "The account or quote inputs expired. Refresh them and preview again.",
  capital_exit_context_stale: "The account or quotes changed. Keep the original limits and preview again.",
  capital_exit_resume_basis_changed: "The spot reservation or account inputs changed. Reload the preview for this plan.",
  capital_exit_preview_stale: "The account control revision changed. Preview again.",
  capital_exit_revision_conflict: "The plan was updated. Refresh the original plan and check it.",
  capital_watch_read_timeout: "Reading account capital timed out. Keep the reservation while its status is verified.",
  capital_exit_status_unavailable: "Native capital status could not be read. Restore the connection and check again.",
  capital_exit_command_requires_reconciliation: "The original capital command still needs reconciliation. Wait for its result.",
  parameter_scan_axes_required: "Select at least one parameter to scan.",
  parameter_scan_values_required: "Enter at least one parameter value.",
  parameter_scan_integer_required: "The period and target validity must be exact positive integers.",
  parameter_scan_decimal_required: "Enter a valid exact decimal parameter.",
  parameter_scan_value_invalid: "A parameter is outside the supported range. Check the period, ratio and validity.",
  parameter_scan_range_invalid: "The range must be ascending with a positive step. Integer parameters need an integer step.",
  parameter_scan_step_invalid: "Enter a valid positive step.",
  parameter_scan_float_step_indistinguishable: "The threshold step is too small to distinguish at native precision. Increase it.",
  parameter_scan_duplicate_axis: "Each parameter can have only one scan range.",
  parameter_scan_axis_unsupported: "This evaluation type does not support the selected parameter.",
  parameter_scan_objective_unsupported: "This type does not support the selected validation metric.",
  parameter_scan_concurrency_invalid: "Concurrency must be a positive integer.",
  parameter_scan_source_invalid: "Select a valid source factor or research trial.",
  parameter_scan_source_not_found: "The source factor or research trial does not exist.",
  parameter_scan_computed_source_required: "Select a source trial whose calculation has completed.",
  parameter_scan_single_factor_required: "This scan supports one original model. Select a single-factor source.",
  parameter_scan_tick_unsupported: "This parameter scan currently supports BAR input only.",
  parameter_scan_native_parameters_required: "The source has no native parameters available to scan.",
  parameter_scan_not_found: "The parameter scan does not exist.",
  parameter_scan_surface_axes_invalid: "Select two different parameter axes.",
  parameter_scan_surface_fixed_required: "Specify fixed values for the remaining parameters.",
  parameter_scan_surface_fixed_invalid: "Each fixed parameter must be an exact value from this scan range.",
  review_allocation_members_required: "Select at least two different portfolios that passed review.",
  review_allocation_constraints_required: "Complete the allocation constraints, current weights and turnover.",
  review_allocation_portfolio_not_approved: "The source portfolio has no approval for its current revision. Refresh it and complete the evidence.",
  review_allocation_common_account_basis_required: "Source portfolios must share a currency, dataset and original starting capital.",
  review_allocation_native_daily_returns_required: "Complete native daily net returns are missing, so portfolio correlation and risk cannot be estimated.",
  review_allocation_native_returns_incomplete: "Source account net returns are incomplete or not aligned in time.",
  review_allocation_scientific_budget_exhausted: "The original run stopped at its scientific-call budget. Review its history and reports.",
  review_model_not_configured: "No independent review model is configured. Complete model settings first.",
  instance_settings_invalid: "The instance configuration is invalid. Check it and try again.",
  instance_settings_revision_conflict: "The configuration changed elsewhere. Load the latest version before editing.",
  instance_settings_save_failed: "The configuration could not be saved. Check it and try again.",
  nautilus_configuration_invalid: "The instance configuration is invalid.", nautilus_backtest_instance_invalid: "Enter a valid backtest instance ID and address.",
  nautilus_url_required: "Enter the instance service address.", nautilus_url_invalid: "The instance address must use HTTP or HTTPS, with no password, query or fragment.",
  nautilus_token_env_invalid: "Enter the name of the server-side environment variable containing the credential.",
  nautilus_connections_invalid: "The account connection configuration is invalid.", nautilus_connection_invalid: "Complete the account connection details.",
  nautilus_connection_id_conflict: "Instance IDs must be unique across account connections.",
  native_remote_execution_unsettled: "The remote scientific run has no confirmed terminal receipt. Restore the original connection and reconcile the same run before deleting it.",
  mandate_deleted: "The parent investment decision was deleted, so its research cannot be resumed.",
  research_delete_tasks_active: "Stop the research and wait for its original tasks to finish before deleting it.",
  research_delete_portfolio_in_use: "Portfolios still use this research. Delete those portfolios first.",
  research_delete_pm_in_use: "Investment decisions still use this research. Resolve those decisions first.",
  factor_delete_portfolio_in_use: "Portfolios still use this factor. Delete those portfolios first.",
  factor_delete_pm_in_use: "Investment decisions still use this factor. Resolve those decisions first.",
  portfolio_delete_tasks_active: "The portfolio is still being calculated. Wait for the original task to finish before deleting it.",
  portfolio_delete_observation_in_use: "The portfolio has active observations or unresolved receipts. Stop observation and complete reconciliation first.",
  pm_delete_requires_retirement: "Retire the current strategy before deleting the investment decision.",
  pm_delete_external_obligations: "The investment decision still has targets or account reconciliation obligations. Reconcile them first.",
  pm_delete_tasks_active: "The investment decision has active tasks. Wait for the original tasks to finish before deleting it.",
  research_deleted: "Research deleted.", factor_deleted: "Factor deleted.", portfolio_deleted: "Portfolio deleted.",

  research_capital_scenarios_invalid: "Capital scenarios must use exact positive amounts below 10²⁰, with at most 18 decimal places and 64 characters.", research_participation_invalid: "Volume participation must be greater than 0 and no more than 1 (0.01 = 1%).",
  research_liquidity_age_invalid: "Enter a positive integer for the maximum liquidity-data age.", research_override_unsupported: "Some research or capacity settings are not supported.",
  research_evaluation_unsupported: "The native evaluation currently supports maximizing registered metrics only.", research_policy_unregistered: "Structured research must inherit the registered dataset's original scientific policy.",
  account_reservation_unknown: "The account reservation is unknown. New targets cannot be delivered.",
  unsupported_evidence: "This research evidence is not supported.", evidence_unavailable: "Research evidence is unavailable.",
  authentication_required: "Your session expired.", request_id_conflict: "The content differs from the original submission.",
  dataset_not_ready: "The dataset is not ready.", dataset_not_found: "The dataset does not exist.",
  independent_evidence_missing: "Independent evaluation results are missing.", heldout_observations_missing: "The held-out sample has no valid observations.",
  research_criterion_insufficient: "The requested independent metric is unavailable, so the research cannot be used in a portfolio.", research_criterion_not_met: "The independent evaluation did not meet the research criterion.",
  strategy_adapter_not_configured: "The dataset has no portfolio strategy configured.", research_dataset_mismatch: "The selected studies use different data.",
  strategy_selection_mismatch: "The research evaluation windows differ.", portfolio_weights_invalid: "Weights must be positive and sum to 1.",
  strategy_policy_not_admitted_with_research: "This portfolio strategy was not admitted with the research.", target_expired: "The target expired.",
  target_delivery_requires_reconciliation: "An existing target needs reconciliation.", account_snapshot_out_of_order: "Account snapshots arrived out of order.", account_snapshot_conflict: "Snapshots for the same account sequence contain conflicting facts.",
  service_extension_required: "This extension is required.", extension_not_found: "The extension does not exist.",
  unauthorized: "Sign in again.", invalid_password: "The password is incorrect.",
  conflict: "The operation conflicts with the current state.", not_found: "The record does not exist.", unavailable: "The service is unavailable.",
  model_unavailable: "The model is unavailable. Connect the provider and refresh its catalog.", thinking_level_unsupported: "This model does not support the selected reasoning effort.", service_tier_unsupported: "This model does not support the selected service tier.",
  model_login_in_progress: "A connection attempt is already in progress for this provider. Refresh to continue.", model_login_not_found: "The connection attempt has ended. Connect again.",
  model_login_cancelled: "Connection cancelled.", model_prompt_changed: "The authorization step changed. Try again.",
  model_login_failed: "Could not connect to the model provider.", model_logout_failed: "Sign-out was not confirmed. Try again.",
  provider_status_unavailable: "The model provider's status is unavailable.", settings_field_not_mutable: "This settings field cannot be changed.",
};
export function shortError(error: unknown): string {
  const text = error instanceof Error ? error.message : typeof error === 'string' ? error : '';
  const code = error instanceof ApiError ? error.code : undefined;
  if (code && errors[code]) return i18n.t(errors[code], { ns: 'shared' });
  if (errors[text]) return i18n.t(errors[text], { ns: 'shared' });
  const scanCode = text.startsWith('parameter_scan_') ? text.split(':', 1)[0] : undefined;
  if (scanCode && errors[scanCode]) return i18n.t(errors[scanCode], { ns: 'shared' });
  if (!text || /[{}\[\]<>]/.test(text)) return i18n.t('Request failed.', { ns: 'shared' });
  if (i18n.exists(text, { ns: 'shared' })) return i18n.t(text, { ns: 'shared' });
  return text.split('\n')[0]!.slice(0, 72);
}

type Options = { method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE'; body?: unknown; rawBody?: string; signal?: AbortSignal };
export async function api<T>(path: string, options: Options = {}): Promise<T> {
  let response: Response;
  const body = options.rawBody ?? (options.body === undefined ? (options.method && options.method !== 'GET' ? '{}' : undefined) : JSON.stringify(options.body));
  try {
    response = await fetch(`${API}${path}`, {
      method: options.method ?? 'GET', credentials: 'same-origin', cache: 'no-store',
      headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
      body, signal: options.signal,
    });
  } catch (error) {
    if (options.signal?.aborted) throw error;
    throw new ApiError(i18n.t('Connection failed.', { ns: 'shared' }), 0, 'connection_failed');
  }
  if (!response.ok) {
    if (response.status === 401 && !path.startsWith('/session')) authenticationEvents.dispatchEvent(new Event('required'));
    const problem: Problem | undefined = await response.json().catch(() => undefined);
    const fallback = response.status === 401 ? "Sign in again." : response.status === 404 ? "The service or record does not exist." : `${i18n.t('Request failed.', { ns: 'shared' })} · ${response.status}`;
    const code = typeof problem?.error === 'string' && /^[a-z][a-z0-9_]+$/.test(problem.error) ? problem.error : undefined;
    throw new ApiError(shortError((code && errors[code]) ?? problem?.message ?? fallback), response.status, code);
  }
  if (response.status === 204 || response.headers.get('content-length') === '0') return undefined as T;
  try { return await response.json() as T; }
  catch { throw new ApiError(i18n.t('The response could not be read.', { ns: 'shared' }), response.status, 'response_unreadable'); }
}
