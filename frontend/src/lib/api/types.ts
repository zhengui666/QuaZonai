export type UUID = string;
export type CodexReasoningEffort = 'minimal' | 'low' | 'medium' | 'high' | 'xhigh';
export type ProgramState = 'ACTIVE' | 'COOLING' | 'APPROVAL_PENDING' | 'WAITING_FOR_FEEDBACK' | 'BLOCKED' | 'PAUSED' | 'ARCHIVED';
export type MissionState = 'PLANNED' | 'READY' | 'RUNNING' | 'SUCCEEDED' | 'FAILED' | 'INTERRUPTED' | 'CANCELLED';
export type ApprovalState = 'PENDING' | 'APPROVED' | 'REJECTED' | 'STALE' | 'EXPIRED';
export type HandoffState = 'APPROVED' | 'PUBLISHING' | 'AVAILABLE' | 'CLAIMED' | 'DOWNSTREAM_ACCEPTED' | 'DOWNSTREAM_REJECTED' | 'FEEDBACK_PENDING' | 'FEEDBACK_IN_PROGRESS' | 'FEEDBACK_PARTIAL' | 'FEEDBACK_COMPLETE' | 'FEEDBACK_STALE' | 'FEEDBACK_INCOMPLETE' | 'FEEDBACK_INVALID' | 'CONSUMER_UNREACHABLE' | 'EXPIRED' | 'REVOKED';

export interface ResearchCharter {
  id?: UUID;
  original_idea_text: string;
  research_question?: string;
  market_scope?: string | string[];
  universe_version_ids?: UUID[];
  prediction_horizon?: string;
  allowed_data_domains?: string[];
  explicit_exclusions?: string[];
  material_assumptions?: string[];
  system_assumptions?: string[];
  created_at?: string;
}

export interface ClarificationQuestion {
  key: string;
  question: string;
}

export interface IdeaDraft {
  id: UUID;
  original_idea_text: string;
  stage: string;
  outcome: string | null;
  next_action: string | null;
  blocking_reasons: string[];
  revision: number;
  clarification_questions: ClarificationQuestion[];
  charter?: ResearchCharter | null;
}

export interface CreateIdeaDraftRequest {
  original_idea_text: string;
}

export interface AnswerIdeaDraftRequest {
  answers: Record<string, string>;
  expected_revision: number;
}

export interface StartIdeaDraftRequest {
  expected_revision: number;
}

export interface ResearchProgram {
  id: UUID;
  title?: string;
  charter_id?: UUID;
  charter?: ResearchCharter;
  state: ProgramState;
  stage?: string;
  outcome?: string | null;
  next_action?: string | null;
  blocking_reasons?: string[];
  revision?: number;
  current_cycle_id?: UUID | null;
  cooling_reason?: string | null;
  blocked_reason?: string | null;
  wake_reason?: string | null;
  created_at?: string;
  updated_at?: string;
  branch_count?: number;
  mission_count?: number;
  alpha_count?: number;
}

export interface ResearchMission {
  id: UUID;
  branch_id?: UUID;
  program_id?: UUID;
  cycle_id?: UUID | null;
  mission_type: string;
  role_profile?: string | null;
  state: MissionState;
  outcome?: string | null;
  objective?: string;
  dependencies?: UUID[];
  contract_version?: string;
  max_turns?: number;
  max_tool_calls?: number;
  started_at?: string | null;
  finished_at?: string | null;
  attempt?: number;
  revision?: number;
  error_code?: string | null;
  summary?: string | null;
}

export interface ActivityEvent {
  id: number | string;
  kind: string;
  aggregate_type?: string;
  aggregate_id?: UUID | null;
  mission_id?: UUID | null;
  payload?: Record<string, unknown>;
  created_at: string;
}

export interface AlphaQualification {
  id: UUID;
  alpha_model_version_id?: UUID;
  calibration_version_id?: UUID;
  universe_version_id?: UUID;
  universe?: string;
  horizon?: string;
  role: string;
  state: string;
  name?: string;
  scope_json?: Record<string, unknown>;
  evaluation_episode_id?: UUID;
  created_at?: string;
  degradation_state?: 'HEALTHY' | 'WATCH' | 'DEGRADING' | 'INVALIDATED';
  metrics?: Record<string, unknown>;
  lineage?: Array<{ id: UUID; label: string; relationship: string }>;
}

export interface PortfolioMandate {
  id: UUID;
  key?: string;
  name: string;
  enabled: boolean;
  latest_version_id?: UUID;
  spec_json?: Record<string, unknown>;
  state?: string;
}

export interface PortfolioProgram {
  id: UUID;
  mandate_version_id: UUID;
  mandate_name?: string;
  state: string;
  created_at?: string;
  updated_at?: string;
  candidate_count?: number;
  current_candidate_id?: UUID | null;
}

export interface CandidateMember {
  alpha_qualification_id: UUID;
  alpha_name?: string;
  role: string;
  target_contribution?: number | null;
  target_weight?: number | null;
  universe?: string;
}

export interface PortfolioCandidate {
  id: UUID;
  candidate_family_id?: UUID;
  portfolio_program_id: UUID;
  mandate_version_id?: UUID;
  mandate_name?: string;
  capital_context_version_id?: UUID;
  universe_set_json?: string[] | Record<string, unknown>;
  policy_version?: string;
  risk_model_version?: string;
  cost_model_version?: string;
  capacity_model_version?: string;
  constraint_set_version?: string;
  rebalance_policy_version?: string;
  evaluation_episode_id?: UUID;
  state: string;
  created_at?: string;
  members?: CandidateMember[];
  metrics?: Record<string, unknown>;
}

export interface ApprovalSnapshot {
  id: UUID;
  candidate_id: UUID;
  candidate?: PortfolioCandidate;
  purpose: 'PAPER' | 'LIVE' | string;
  state: ApprovalState;
  downstream_system_id?: UUID | null;
  downstream_name?: string | null;
  created_at?: string;
  valid_until?: string | null;
  expires_at?: string | null;
  stale_reason?: string | null;
  recommendation_rationale?: string;
  human_report?: Record<string, unknown> | string | null;
  evidence_summary?: Record<string, unknown>;
  capital_context?: { base_currency?: string; deployable_capital?: number | string; observed_at?: string; valid_until?: string };
  risk_summary?: Record<string, unknown>;
  cost_summary?: Record<string, unknown>;
  capacity_summary?: Record<string, unknown>;
  changes_summary?: Record<string, unknown>;
}

export interface HandoffOffer {
  id: UUID;
  approval_id?: UUID;
  candidate_package_id?: UUID;
  candidate_id?: UUID;
  purpose?: string;
  downstream_system_id?: UUID;
  downstream_name?: string;
  state: HandoffState;
  claim_deadline?: string | null;
  package_contract_version?: string;
  feedback_contract_version?: string;
  created_at?: string;
  updated_at?: string;
  stale_reason?: string | null;
  feedback_state?: string | null;
}

export interface Readiness {
  SYSTEM_READY?: boolean | { ready: boolean; reasons?: string[] };
  RESEARCH_READY?: boolean | { ready: boolean; reasons?: string[] };
  PAPER_HANDOFF_READY?: boolean | { ready: boolean; reasons?: string[] };
  LIVE_HANDOFF_READY?: boolean | { ready: boolean; reasons?: string[] };
  [key: string]: unknown;
}

export interface SystemHealth {
  live?: boolean;
  ready?: boolean;
  database?: unknown;
  worker?: unknown;
  agent_worker?: unknown;
  evaluator?: unknown;
  storage?: unknown;
  codex?: unknown;
  data?: unknown;
  [key: string]: unknown;
}

export interface RuntimeConfiguration {
  revision: number;
  codex_model: string | null;
  codex_reasoning_effort: CodexReasoningEffort | null;
  codex_fast_mode: boolean;
  codex_use_default_model_settings: boolean;
  codex_base_url: string | null;
  codex_api_key_configured: boolean;
  codex_login_configured: boolean;
  max_plugin_wheel_bytes: number;
  plugin_validation_timeout_seconds: number;
  bundle_build_timeout_seconds: number;
  plugin_job_timeout_seconds: number;
  mission_job_timeout_seconds: number;
  job_poll_seconds: number;
  job_lease_seconds: number;
  updated_at?: string | null;
}

export interface RuntimeConfigurationUpdate {
  expected_revision: number;
  codex_model: string | null;
  codex_reasoning_effort: CodexReasoningEffort | null;
  codex_fast_mode: boolean;
  codex_use_default_model_settings: boolean;
  codex_base_url: string | null;
  codex_api_key?: string | null;
  clear_codex_api_key: boolean;
  max_plugin_wheel_bytes: number;
  plugin_validation_timeout_seconds: number;
  bundle_build_timeout_seconds: number;
  plugin_job_timeout_seconds: number;
  mission_job_timeout_seconds: number;
  job_poll_seconds: number;
  job_lease_seconds: number;
}

export type CodexChatgptAuthState = 'DISCONNECTED' | 'CONNECTED' | 'REAUTH_REQUIRED';
export type CodexChatgptLoginState = 'PENDING' | 'SUCCEEDED' | 'CANCELLED' | 'EXPIRED' | 'FAILED';

export interface CodexChatgptPendingLogin {
  login_id: UUID;
  expires_at: string;
  poll_after_seconds: number;
}

export interface CodexChatgptAuthStatus {
  state: CodexChatgptAuthState;
  active: boolean;
  email: string | null;
  plan_type: string | null;
  authenticated_at: string | null;
  last_refresh_at: string | null;
  reauth_required_at: string | null;
  pending_login: CodexChatgptPendingLogin | null;
  legacy_auth_file_present: boolean;
}

export interface CodexChatgptDeviceLoginStart {
  login_id: UUID;
  status: 'PENDING';
  verification_url: 'https://auth.openai.com/codex/device';
  user_code: string;
  expires_at: string;
  poll_after_seconds: number;
}

export interface CodexChatgptDeviceLoginPoll {
  status: CodexChatgptLoginState;
  expires_at: string | null;
  poll_after_seconds: number | null;
  auth: CodexChatgptAuthStatus | null;
  error_code: string | null;
}

/** Canonical fresh-install administration facts from root /api/v1 resources. */
export interface ConfigurationUniverse {
  id: UUID;
  universe_key: string;
  version_no: number;
  name: string;
  state: string;
  spec: Record<string, unknown>;
  created_at: string;
}

export interface ConfigurationDataSource {
  id: UUID;
  name: string;
  connector_key: string;
  provider: string | null;
  state: string;
  universe_scope: string[];
  field_schema: Record<string, unknown>;
  license_classification: string;
  availability_semantics: Record<string, unknown>;
  update_cadence: string | null;
  preflight_state: string;
  public_config: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface ConfigurationDataset {
  id: UUID;
  data_source_id: UUID | null;
  universe_version_id: UUID | null;
  universe_name: string | null;
  revision_no: number;
  partition: string;
  data_class: string | null;
  origin: string | null;
  promotability: string | null;
  schema_version: string | null;
  event_start: string | null;
  event_end: string | null;
  available_start: string | null;
  available_end: string | null;
  row_count: number | null;
  quality_state: string;
  point_in_time_state: string;
  created_at: string;
}

export interface ConfigurationOperation {
  id: UUID;
  kind: string;
  resource_type: string;
  resource_id: UUID;
  state: string;
  attempt: number;
  last_error: string | null;
  created_at: string;
  updated_at: string;
}

export interface ConfigurationMandateVersion {
  id: UUID;
  portfolio_mandate_id: UUID;
  version_no: number;
  policy_family: 'LONG_ONLY_MEAN_VARIANCE_V1';
  base_currency: string;
  objective: 'MAXIMIZE_NET_RETURN';
  eligible_alpha_role: 'PRIMARY_ALPHA';
  universe_version_id: UUID;
  minimum_alpha_count: number;
  minimum_weight: string;
  maximum_weight: string;
  gross_exposure_limit: string;
  net_exposure_target: string;
  cash_reserve: string;
  turnover_limit: string;
  variance_limit: string;
  risk_aversion: string;
  cost_aversion: string;
  uncertainty_aversion: string;
  commission_rate: string;
  half_spread_rate: string;
  slippage_rate: string;
  impact_rate: string;
  impact_breakpoint: string;
  state: 'ACTIVE' | 'RETIRED';
  created_at: string;
}

export interface ConfigurationMandate {
  id: UUID;
  key: string;
  name: string;
  enabled: boolean;
  state: string;
  configuration_state: 'V1_CONFIGURED' | 'LEGACY_UNAVAILABLE';
  latest_version: ConfigurationMandateVersion | null;
  created_at: string;
  updated_at: string;
}

export interface ConfigurationCapitalContext {
  id: UUID;
  configuration_contract_version: 'CAPITAL_CONTEXT_V1' | null;
  configuration_state: 'V1_CONFIGURED' | 'LEGACY_UNAVAILABLE';
  source_type: string;
  source_downstream_system_id: UUID | null;
  base_currency: string;
  deployable_capital: string;
  observed_at: string;
  valid_until: string;
  notes: string | null;
  created_at: string;
  updated_at: string;
}

export interface ConfigurationEvaluationDatasetSelection {
  id: UUID;
  universe_version_id: UUID;
  version_no: number;
  discovery_dataset_revision_id: UUID;
  validation_dataset_revision_id: UUID;
  sealed_dataset_revision_id: UUID;
  state: 'ENABLED' | 'RETIRED';
  created_at: string;
}

export interface ConfigurationEvaluationDesignVersion {
  id: UUID;
  version_no: number;
  universe_version_id: UUID;
  contract_version: string;
  allowed_model_mode: 'RELATIVE_SCORE' | 'CALIBRATED_RETURN';
  qualification_role: 'PRIMARY_ALPHA' | 'DIVERSIFIER_ALPHA' | 'HEDGE_ALPHA' | 'REGIME_SIGNAL' | 'RISK_MODULATOR' | 'SHADOW_ALPHA';
  walk_forward_folds: number;
  annualization_factor: string;
  multiple_testing_method: 'BONFERRONI' | 'BENJAMINI_HOCHBERG';
  multiple_testing_max_trials: number;
  qualification_metric_code: string;
  qualification_comparator: 'MINIMUM' | 'MAXIMUM';
  qualification_threshold: string;
  pass_disclosure_code: string;
  failure_disclosure_code: string;
  inconclusive_disclosure_code: string;
  invalid_disclosure_code: string;
  state: 'ACTIVE' | 'RETIRED';
  created_at: string;
}

export interface ConfigurationPromotionPolicyGate {
  metric_code: string;
  comparator: 'MINIMUM' | 'MAXIMUM';
  threshold: string;
  ordinal: number;
}

export interface ConfigurationPromotionPolicyVersion {
  id: UUID;
  version_no: number;
  purpose: 'ALPHA_DISCOVERY_TO_SEALED' | 'SEALED_TO_QUALIFIED' | 'PORTFOLIO_TO_PAPER' | 'PAPER_TO_LIVE';
  mode: 'MANUAL_APPROVAL' | 'AUTO_HANDOFF';
  paper_downstream_system_id: UUID | null;
  live_downstream_system_id: UUID | null;
  gates: ConfigurationPromotionPolicyGate[];
  state: 'ACTIVE' | 'RETIRED';
  created_at: string;
}

export interface ConfigurationDownstream {
  id: UUID;
  name: string;
  environment_type: 'PAPER' | 'LIVE' | string;
  enabled: boolean;
  package_contract_version: string;
  feedback_contract_version: string;
  compatibility: string[];
  preflight_state: string;
  public_config: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface ConfigurationDownstreamRegistration extends ConfigurationDownstream {
  service_token: string | null;
  token_issued: boolean;
}

export interface PluginRelease { id: UUID; plugin_id?: string; version?: string; state: string; capabilities?: string[]; created_at?: string; }
export interface OhlcPoint { time: string | number; open: number; high: number; low: number; close: number; volume?: number; }
export interface ApiErrorEnvelope { error?: { code?: string; message?: string; details?: Record<string, unknown> }; }
