export type UUID = string;
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

export interface ResearchProgram {
  id: UUID;
  title?: string;
  charter_id?: UUID;
  charter?: ResearchCharter;
  state: ProgramState;
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
  type: string;
  role?: string;
  state: MissionState;
  objective?: string;
  dependencies?: UUID[];
  started_at?: string | null;
  finished_at?: string | null;
  attempt?: number;
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

export interface MarketUniverse { id: UUID; universe_key?: string; version_no?: number; name: string; state?: string; spec_json?: Record<string, unknown>; }
export interface DatasetRevision { id: UUID; data_source_id?: UUID; universe_version_id?: UUID; universe_name?: string; revision_no?: number; schema_version?: string; event_start?: string; event_end?: string; available_start?: string; available_end?: string; row_count?: number; quality_state?: string; point_in_time_state?: string; partition?: 'DISCOVERY' | 'SEALED' | string; created_at?: string; }
export interface DataSource { id: UUID; name: string; provider?: string; state: string; universe_scope?: string[] | UUID[]; fields?: string[]; update_cadence?: string; preflight_state?: string; }
export interface DownstreamSystem { id: UUID; name: string; environment_type: 'PAPER' | 'LIVE' | 'EXTERNAL_BACKTEST' | string; enabled: boolean; package_contract_version?: string; feedback_contract_version?: string; compatibility?: string[]; preflight_state?: string; }
export interface PluginRelease { id: UUID; plugin_id?: string; version?: string; state: string; capabilities?: string[]; created_at?: string; }
export interface IdeaPreview { charter?: ResearchCharter; clarification_required?: boolean; clarification_questions?: Array<{ key: string; question: string }>; overlap?: { kind?: 'DUPLICATE' | 'BRANCH' | 'RELATED_PROGRAM' | 'NEW' | string; program_id?: UUID; program_title?: string; rationale?: string; recommendation?: string } | null; assumptions?: string[]; }
export interface OhlcPoint { time: string | number; open: number; high: number; low: number; close: number; volume?: number; }
export interface ApiErrorEnvelope { error?: { code?: string; message?: string; details?: Record<string, unknown> }; }