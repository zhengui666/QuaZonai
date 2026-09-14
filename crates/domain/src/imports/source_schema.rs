//! Identity-column schema facts from pre-removal Alembic 0029, not executable legacy code.
//! This does not prove composite context-column equality or project/lineage consistency.
//! The exporter separately requires a per-field nonsecret projection.
use contracts::imports::HistoricalKindV1;
pub type ForeignKey = (&'static str, &'static str, bool);
pub fn table(name: &str) -> Option<(HistoricalKindV1, &'static [ForeignKey])> {
    use HistoricalKindV1::*;
    Some(match name {
        "alpha_calibration_versions" => (
            Evidence,
            &[
                ("alpha_model_version_id", "alpha_model_versions", true),
                (
                    "source_discovery_evaluation_id",
                    "alpha_discovery_evaluations",
                    false,
                ),
                ("training_dataset_revision_id", "dataset_revisions", false),
            ],
        ),
        "alpha_discovery_evaluation_gates" => (
            Evaluation,
            &[(
                "discovery_evaluation_id",
                "alpha_discovery_evaluations",
                true,
            )],
        ),
        "alpha_discovery_evaluation_metrics" => (
            Evaluation,
            &[(
                "discovery_evaluation_id",
                "alpha_discovery_evaluations",
                true,
            )],
        ),
        "alpha_discovery_evaluations" => (
            Evaluation,
            &[
                ("alpha_model_version_id", "alpha_model_versions", true),
                ("branch_id", "research_branches", true),
                ("cause_event_id", "events", true),
                ("cycle_id", "research_cycles", true),
                ("discovery_dataset_revision_id", "dataset_revisions", true),
                (
                    "evaluation_dataset_selection_id",
                    "evaluation_dataset_selections",
                    true,
                ),
                (
                    "evaluation_design_version_id",
                    "evaluation_design_versions",
                    true,
                ),
                ("mission_id", "research_missions", true),
                ("program_id", "research_programs", true),
                ("source_mission_artifact_id", "mission_artifacts", true),
            ],
        ),
        "alpha_evaluation_assignment_dataset_revisions" => (
            Evaluation,
            &[
                ("assignment_id", "alpha_evaluation_assignments", true),
                ("dataset_revision_id", "dataset_revisions", true),
            ],
        ),
        "alpha_evaluation_assignments" => (
            Evaluation,
            &[
                (
                    "alpha_calibration_version_id",
                    "alpha_calibration_versions",
                    false,
                ),
                ("alpha_model_version_id", "alpha_model_versions", true),
                ("branch_id", "research_branches", true),
                ("cause_event_id", "events", true),
                ("cycle_id", "research_cycles", true),
                (
                    "discovery_evaluation_id",
                    "alpha_discovery_evaluations",
                    true,
                ),
                (
                    "evaluation_design_version_id",
                    "evaluation_design_versions",
                    true,
                ),
                ("mission_id", "research_missions", true),
                ("program_id", "research_programs", true),
                (
                    "promotion_policy_version_id",
                    "promotion_policy_versions",
                    true,
                ),
                ("sealed_dataset_revision_id", "dataset_revisions", true),
                ("source_mission_artifact_id", "mission_artifacts", true),
                ("universe_version_id", "market_universe_versions", true),
            ],
        ),
        "alpha_evaluation_episodes" => (
            Evaluation,
            &[
                ("alpha_model_version_id", "alpha_model_versions", true),
                ("assignment_id", "alpha_evaluation_assignments", false),
                ("branch_id", "research_branches", true),
                ("program_id", "research_programs", true),
                ("sealed_dataset_revision_id", "dataset_revisions", true),
                ("sealed_run_id", "quant_runtime_runs", false),
            ],
        ),
        "alpha_evaluation_forecasts" => (
            Evaluation,
            &[
                ("result_id", "alpha_evaluation_results", true),
                ("signal_artifact_id", "alpha_signal_artifacts", true),
            ],
        ),
        "alpha_evaluation_gates" => (
            Evaluation,
            &[("result_id", "alpha_evaluation_results", true)],
        ),
        "alpha_evaluation_metrics" => (
            Evaluation,
            &[("result_id", "alpha_evaluation_results", true)],
        ),
        "alpha_evaluation_results" => (
            Evaluation,
            &[("episode_id", "alpha_evaluation_episodes", true)],
        ),
        "alpha_model_versions" => (
            Strategy,
            &[
                ("alpha_model_id", "alpha_models", true),
                (
                    "feature_pipeline_version_id",
                    "feature_pipeline_versions",
                    false,
                ),
                ("source_mission_artifact_id", "mission_artifacts", false),
                ("source_mission_id", "research_missions", true),
                ("universe_version_id", "market_universe_versions", true),
            ],
        ),
        "alpha_models" => (Strategy, &[("owner_program_id", "research_programs", true)]),
        "alpha_qualifications" => (
            Evidence,
            &[
                ("alpha_model_id", "alpha_models", false),
                ("evaluation_result_id", "alpha_evaluation_results", false),
                ("program_id", "research_programs", false),
                ("universe_version_id", "market_universe_versions", false),
            ],
        ),
        "alpha_signal_artifacts" => (
            Artifact,
            &[
                ("alpha_model_version_id", "alpha_model_versions", true),
                ("dataset_revision_id", "dataset_revisions", true),
                ("evaluation_result_id", "alpha_evaluation_results", false),
                ("run_id", "quant_runtime_runs", false),
            ],
        ),
        "approval_snapshots" => (
            Approval,
            &[
                ("candidate_id", "portfolio_candidates", true),
                ("candidate_package_id", "candidate_packages", false),
                (
                    "downstream_connection_version_id",
                    "downstream_connection_versions",
                    false,
                ),
                ("downstream_system_id", "downstream_systems", false),
                (
                    "feedback_contract_version_id",
                    "feedback_contract_versions",
                    false,
                ),
                (
                    "paper_to_live_policy_version_id",
                    "promotion_policy_versions",
                    false,
                ),
                ("preflight_receipt_id", "preflight_receipts", false),
                ("promotion_evaluation_id", "promotion_evaluations", false),
            ],
        ),
        "archive_manifest_shards" => (Evidence, &[("manifest_id", "archive_manifests", true)]),
        "archive_manifests" => (
            Evidence,
            &[
                ("data_source_id", "governed_data_sources", true),
                ("universe_version_id", "market_universe_versions", true),
            ],
        ),
        "candidate_packages" => (Evidence, &[("candidate_id", "portfolio_candidates", true)]),
        "capital_context_versions" => (
            Evidence,
            &[("source_downstream_system_id", "downstream_systems", false)],
        ),
        "clarification_answers" => (
            Research,
            &[("question_id", "clarification_questions", true)],
        ),
        "clarification_questions" => (Research, &[("idea_draft_id", "idea_drafts", true)]),
        "data_quality_results" => (
            Evidence,
            &[("dataset_revision_id", "dataset_revisions", true)],
        ),
        "dataset_revisions" => (
            Evidence,
            &[
                ("data_source_id", "governed_data_sources", false),
                ("point_in_time_result_id", "data_quality_results", false),
                ("quality_result_id", "data_quality_results", false),
                ("universe_version_id", "market_universe_versions", false),
            ],
        ),
        "degradation_observations" => (
            Evidence,
            &[
                (
                    "forward_evidence_episode_id",
                    "forward_evidence_episodes",
                    true,
                ),
                ("program_id", "research_programs", true),
            ],
        ),
        "disclosures" => (
            Evidence,
            &[("episode_id", "alpha_evaluation_episodes", true)],
        ),
        "downstream_connection_versions" => (
            Evidence,
            &[
                ("credential_set_id", "credential_sets", false),
                ("downstream_system_id", "downstream_systems", true),
                (
                    "feedback_contract_version_id",
                    "feedback_contract_versions",
                    true,
                ),
                ("plugin_release_id", "plugin_releases", false),
            ],
        ),
        "downstream_systems" => (Evidence, &[]),
        "evaluation_dataset_selections" => (
            Evaluation,
            &[
                ("discovery_dataset_revision_id", "dataset_revisions", true),
                ("sealed_dataset_revision_id", "dataset_revisions", true),
                ("universe_version_id", "market_universe_versions", true),
                ("validation_dataset_revision_id", "dataset_revisions", true),
            ],
        ),
        "evaluation_design_versions" => (
            Evaluation,
            &[("universe_version_id", "market_universe_versions", true)],
        ),
        "evaluation_episodes" => (
            Evaluation,
            &[
                ("branch_id", "research_branches", true),
                ("discovery_run_id", "quant_runtime_runs", true),
                ("program_id", "research_programs", true),
                ("sealed_dataset_revision_id", "dataset_revisions", false),
                ("sealed_run_id", "quant_runtime_runs", false),
            ],
        ),
        "events" => (Evidence, &[]),
        "evidence_exposures" => (
            Evidence,
            &[("episode_id", "alpha_evaluation_episodes", true)],
        ),
        "feature_pipeline_versions" => (
            Evidence,
            &[("universe_version_id", "market_universe_versions", true)],
        ),
        "feedback_contract_accepted_arrow_contracts" => (
            Evidence,
            &[(
                "feedback_contract_version_id",
                "feedback_contract_versions",
                true,
            )],
        ),
        "feedback_contract_accepted_package_contracts" => (
            Evidence,
            &[(
                "feedback_contract_version_id",
                "feedback_contract_versions",
                true,
            )],
        ),
        "feedback_contract_metric_requirements" => (
            Evidence,
            &[(
                "feedback_contract_version_id",
                "feedback_contract_versions",
                true,
            )],
        ),
        "feedback_contract_versions" => (
            Evidence,
            &[("downstream_system_id", "downstream_systems", true)],
        ),
        "feedback_packages" => (Evidence, &[("handoff_offer_id", "handoff_offers", true)]),
        "forward_evidence_episodes" => (
            Evidence,
            &[
                ("feedback_package_id", "feedback_packages", false),
                ("handoff_id", "handoff_offers", true),
            ],
        ),
        "forward_evidence_metrics" => (
            Evidence,
            &[("episode_id", "forward_evidence_episodes", true)],
        ),
        "governed_data_sources" => (Evidence, &[]),
        "handoff_offers" => (
            Handoff,
            &[
                ("approval_id", "approval_snapshots", true),
                ("candidate_id", "portfolio_candidates", true),
                ("candidate_package_id", "candidate_packages", true),
                (
                    "downstream_connection_version_id",
                    "downstream_connection_versions",
                    false,
                ),
                ("downstream_system_id", "downstream_systems", true),
                (
                    "feedback_contract_version_id",
                    "feedback_contract_versions",
                    false,
                ),
                (
                    "paper_to_live_policy_version_id",
                    "promotion_policy_versions",
                    false,
                ),
                ("preflight_receipt_id", "preflight_receipts", false),
            ],
        ),
        "idea_contributions" => (Research, &[("program_id", "research_programs", true)]),
        "idea_drafts" => (Research, &[]),
        "jobs" => (Run, &[]),
        "market_universe_versions" => (Evidence, &[]),
        "mission_artifacts" => (
            Artifact,
            &[
                ("mission_id", "research_missions", true),
                ("turn_id", "agent_turns", false),
            ],
        ),
        "mission_dependencies" => (
            Evidence,
            &[
                ("depends_on_mission_id", "research_missions", true),
                ("mission_id", "research_missions", true),
            ],
        ),
        "nautilus_catalog_bindings" => (
            Evidence,
            &[("dataset_revision_id", "dataset_revisions", true)],
        ),
        "portfolio_assembly_input_covariances" => (Evidence, &[]),
        "portfolio_assembly_input_members" => (
            Evidence,
            &[
                ("alpha_qualification_id", "alpha_qualifications", true),
                ("alpha_signal_artifact_id", "alpha_signal_artifacts", true),
                ("input_id", "portfolio_assembly_inputs", true),
            ],
        ),
        "portfolio_assembly_inputs" => (
            Evidence,
            &[
                (
                    "capital_context_version_id",
                    "capital_context_versions",
                    true,
                ),
                ("cause_event_id", "events", true),
                ("mandate_version_id", "portfolio_mandate_versions", true),
                (
                    "portfolio_input_evaluation_assignment_id",
                    "portfolio_input_evaluation_assignments",
                    true,
                ),
                ("portfolio_program_id", "portfolio_programs", true),
                ("previous_candidate_id", "portfolio_candidates", false),
                (
                    "promotion_policy_version_id",
                    "promotion_policy_versions",
                    true,
                ),
                ("universe_version_id", "market_universe_versions", true),
            ],
        ),
        "portfolio_candidate_families" => (
            Evidence,
            &[
                ("mandate_version_id", "portfolio_mandate_versions", true),
                ("portfolio_program_id", "portfolio_programs", true),
            ],
        ),
        "portfolio_candidate_members" => (
            Evidence,
            &[
                ("alpha_qualification_id", "alpha_qualifications", true),
                ("candidate_id", "portfolio_candidates", true),
            ],
        ),
        "portfolio_candidates" => (
            Evidence,
            &[
                ("assembly_input_id", "portfolio_assembly_inputs", false),
                ("candidate_family_id", "portfolio_candidate_families", false),
                ("portfolio_program_id", "portfolio_programs", true),
                ("universe_version_id", "market_universe_versions", false),
            ],
        ),
        "portfolio_evaluation_assignments" => (
            Evaluation,
            &[
                ("candidate_id", "portfolio_candidates", true),
                ("cause_event_id", "events", true),
                (
                    "evaluation_dataset_selection_id",
                    "evaluation_dataset_selections",
                    true,
                ),
                ("previous_candidate_id", "portfolio_candidates", false),
                (
                    "promotion_policy_version_id",
                    "promotion_policy_versions",
                    true,
                ),
            ],
        ),
        "portfolio_evaluation_disclosures" => (
            Evaluation,
            &[("episode_id", "portfolio_evaluation_episodes", true)],
        ),
        "portfolio_evaluation_episodes" => (
            Evaluation,
            &[("assignment_id", "portfolio_evaluation_assignments", true)],
        ),
        "portfolio_evaluation_gates" => (
            Evaluation,
            &[("episode_id", "portfolio_evaluation_episodes", true)],
        ),
        "portfolio_evaluation_metrics" => (
            Evaluation,
            &[("episode_id", "portfolio_evaluation_episodes", true)],
        ),
        "portfolio_input_evaluation_assignment_members" => (
            Evaluation,
            &[
                ("alpha_qualification_id", "alpha_qualifications", true),
                ("alpha_signal_artifact_id", "alpha_signal_artifacts", true),
                (
                    "assignment_id",
                    "portfolio_input_evaluation_assignments",
                    true,
                ),
            ],
        ),
        "portfolio_input_evaluation_assignments" => (
            Evaluation,
            &[
                (
                    "capital_context_version_id",
                    "capital_context_versions",
                    true,
                ),
                ("cause_event_id", "events", true),
                (
                    "evaluation_dataset_selection_id",
                    "evaluation_dataset_selections",
                    true,
                ),
                ("mandate_version_id", "portfolio_mandate_versions", true),
                ("portfolio_program_id", "portfolio_programs", true),
                ("previous_candidate_id", "portfolio_candidates", false),
                (
                    "promotion_policy_version_id",
                    "promotion_policy_versions",
                    true,
                ),
            ],
        ),
        "portfolio_mandate_versions" => (
            Evidence,
            &[
                ("portfolio_mandate_id", "portfolio_mandates", true),
                ("universe_version_id", "market_universe_versions", false),
            ],
        ),
        "portfolio_mandates" => (Evidence, &[]),
        "portfolio_programs" => (Evidence, &[]),
        "portfolio_search_ledger_entries" => (
            Evidence,
            &[
                ("cause_event_id", "events", true),
                (
                    "portfolio_assembly_input_id",
                    "portfolio_assembly_inputs",
                    false,
                ),
                ("portfolio_program_id", "portfolio_programs", true),
            ],
        ),
        "preflight_receipts" => (Evidence, &[]),
        "program_relationships" => (
            Research,
            &[
                ("from_program_id", "research_programs", true),
                ("to_program_id", "research_programs", true),
            ],
        ),
        "promotion_evaluations" => (
            Evaluation,
            &[
                ("candidate_package_id", "candidate_packages", true),
                (
                    "downstream_connection_version_id",
                    "downstream_connection_versions",
                    true,
                ),
                (
                    "forward_evidence_episode_id",
                    "forward_evidence_episodes",
                    false,
                ),
                (
                    "paper_to_live_policy_version_id",
                    "promotion_policy_versions",
                    false,
                ),
                ("policy_version_id", "promotion_policy_versions", true),
                (
                    "portfolio_evaluation_episode_id",
                    "portfolio_evaluation_episodes",
                    false,
                ),
                ("preflight_receipt_id", "preflight_receipts", true),
            ],
        ),
        "promotion_gate_results" => (
            Evidence,
            &[("evaluation_id", "promotion_evaluations", true)],
        ),
        "promotion_policy_gates" => (
            Evidence,
            &[("policy_version_id", "promotion_policy_versions", true)],
        ),
        "promotion_policy_versions" => (
            Evidence,
            &[
                (
                    "live_connection_version_id",
                    "downstream_connection_versions",
                    false,
                ),
                ("live_downstream_system_id", "downstream_systems", false),
                ("live_preflight_receipt_id", "preflight_receipts", false),
                (
                    "paper_connection_version_id",
                    "downstream_connection_versions",
                    false,
                ),
                ("paper_downstream_system_id", "downstream_systems", false),
                ("paper_preflight_receipt_id", "preflight_receipts", false),
                (
                    "paper_to_live_policy_version_id",
                    "promotion_policy_versions",
                    false,
                ),
            ],
        ),
        "public_mutation_receipts" => (Evidence, &[]),
        "quant_runtime_runs" => (
            Run,
            &[
                ("branch_id", "research_branches", true),
                ("mission_id", "research_missions", false),
                ("parent_run_id", "quant_runtime_runs", false),
                ("program_id", "research_programs", true),
            ],
        ),
        "research_branches" => (
            Research,
            &[
                ("cycle_id", "research_cycles", false),
                ("parent_branch_id", "research_branches", false),
                ("program_id", "research_programs", true),
            ],
        ),
        "research_charters" => (Research, &[("idea_draft_id", "idea_drafts", false)]),
        "research_cycles" => (Research, &[("program_id", "research_programs", true)]),
        "research_missions" => (
            Run,
            &[
                ("branch_id", "research_branches", true),
                ("cycle_id", "research_cycles", false),
                ("program_id", "research_programs", true),
            ],
        ),
        "research_programs" => (
            Research,
            &[
                ("charter_id", "research_charters", true),
                ("current_cycle_id", "research_cycles", false),
                (
                    "evidence_inherited_from_program_id",
                    "research_programs",
                    false,
                ),
                ("source_program_id", "research_programs", false),
            ],
        ),
        "research_wake_events" => (
            Evidence,
            &[
                ("cycle_id", "research_cycles", false),
                (
                    "degradation_observation_id",
                    "degradation_observations",
                    true,
                ),
                (
                    "forward_evidence_episode_id",
                    "forward_evidence_episodes",
                    true,
                ),
                ("program_id", "research_programs", true),
            ],
        ),
        "search_ledger_entries" => (
            Evidence,
            &[
                ("branch_id", "research_branches", true),
                ("mission_id", "research_missions", false),
                ("program_id", "research_programs", true),
                ("run_id", "quant_runtime_runs", true),
            ],
        ),
        _ => return None,
    })
}
