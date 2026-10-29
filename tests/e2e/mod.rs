mod accounting_attempt_identity_roots_tests;
mod accounting_attempt_identity_tests;
mod accounting_contract_tests;
mod accounting_convention_tests;
mod accounting_duration_tests;
mod accounting_extraction_diagnostic_tests;
mod accounting_line_tests;
mod accounting_presentation_tests;
mod accounting_prices_support;
mod accounting_prices_tests;
mod accounting_profile_price_composition_tests;
mod accounting_profile_price_lifecycle_tests;
mod accounting_profile_prices_tests;
mod accounting_settings_prices_tests;
mod accounting_support;
mod accounting_unrecorded_spawn_support;
mod accounting_unrecorded_spawn_tests;
mod agent_environment_tests;
mod agent_family_support;
mod agent_family_tests;
mod agent_family_transport_tests;
mod agent_grounds_settings_tests;
mod agent_grounds_support;
mod agent_grounds_templates_tests;
mod agent_owed_marker_tests;
mod agent_prompt_path_tests;
mod agent_prompt_size_tests;
mod agent_reentry_completion_tests;
mod agent_reentry_support;
mod agent_reentry_tests;
// A signal ending exists only on Unix. §FS-rhei-agents.3.2.1
#[cfg(unix)]
mod agent_retry_ending_tests;
mod agent_retry_notice_tests;
mod agent_target_edit_tests;
mod archive_refusal_tests;
mod archive_snapshot_tests;
mod archive_support;
mod archive_tamper_tests;
mod archive_tests;
mod budget_ancestry_project_tests;
mod budget_ceiling_tests;
mod budget_clamp_projects_refusal_tests;
mod budget_copied_project_support;
mod budget_copied_project_tests;
mod budget_declaration_free_tests;
mod budget_delegated_ceiling_tests;
mod budget_delegated_support;
mod budget_edge_support;
mod budget_missing_directory_tests;
mod budget_nested_plan_tests;
mod budget_path_reuse_quoting_tests;
mod budget_path_reuse_support;
mod budget_path_reuse_tests;
mod budget_program_verification_support;
mod budget_program_verification_tests;
mod budget_run_report_end_tests;
mod budget_spend_tests;
mod budget_support;
mod budget_travel_establish_tests;
mod budget_travel_halt_tests;
mod budget_unpriced_naming_tests;
mod budget_window_tests;
mod callback_timeout_support;
mod callback_timeout_tests;
mod cancelled_prior_tests;
mod changeset_review_equivalence_tests;
mod codex_config_fallback_support;
mod codex_config_fallback_tests;
mod codex_workspace_profile_tests;
mod codex_writable_roots_support;
mod codex_writable_roots_tests;
mod complete_result_input_tests;
mod completions_tests;
mod consumes_advisory_tests;
mod cost_accounting_tests;
mod cost_project_scope_support;
mod cost_project_scope_tests;
mod cost_selection_tests;
mod current_dir_target_tests;
mod dead_end_state_tests;
mod declared_exit_route_tests;
mod diagnostic_wrap_tests;
mod effort_invocation_tests;
mod effort_state_tests;
mod effort_support;
mod entry_log_restart_tests;
mod entry_log_support;
mod entry_log_tests;
mod error_guidance_tests;
mod evidence_accounting_tests;
mod evidence_callback_tests;
mod evidence_lifecycle_tests;
mod evidence_records_tests;
mod evidence_retention_tests;
mod evidence_seal_tests;
mod evidence_support;
mod evidence_version_tests;
mod examples_tests;
mod exclusions_context_tests;
mod exclusions_regressions_tests;
mod exclusions_required_tests;
mod exclusions_tests;
mod exclusions_validation_tests;
mod export_prior_migration_implementation_tests;
mod export_prior_migration_tests;
mod fence_structure_tests;
mod handoff_tests;
mod headless_dead_supervisor_tests;
mod headless_recovery_tests;
mod headless_registry_access_tests;
mod headless_stop_ownership_tests;
mod headless_support;
mod headless_tests;
mod headless_undecided_tests;
mod includes_under_tests;
mod install_skills_tests;
mod installed_state_machine_guidance_tests;
mod instantiate_existing_output_tests;
mod instantiate_output_default_tests;
mod instantiate_output_spelling_tests;
mod instantiate_project_preview_tests;
mod instantiate_project_validation_support;
mod instantiate_project_validation_tests;
mod into_flow_style_tests;
mod into_identity_tests;
mod into_placement_tests;
mod into_project_bundle_tests;
mod into_project_hand_bound_tests;
mod into_project_lay_support;
mod into_project_lay_tests;
mod into_project_rebind_tests;
mod into_project_tests;
mod into_refusal_tests;
mod into_root_machine_tests;
mod into_support;
mod into_template_gate_tests;
mod laid_output_root_tests;
mod list_ready_tests;
mod list_task_metadata_tests;
mod live_checkpoint_validation_tests;
mod live_member_admission_tests;
mod live_member_lock_tests;
mod live_member_program_tests;
mod loop_budget_refusal_tests;
mod mcp_config_file_tests;
mod mcp_surface_tests;
mod member_publication_tests;
mod member_settings_root_tests;
mod memory_map_tests;
mod memory_note_store_tests;
mod memory_prompt_tests;
mod memory_supervisor_prompt_tests;
mod new_description_marker_tests;
mod new_directory_adoption_tests;
mod new_guard_tests;
mod new_tests;
mod new_write_tests;
mod next_claim_atomicity_tests;
mod next_claim_boundary_tests;
mod next_explicit_passive_atomicity_tests;
mod next_explicit_passive_claim_tests;
mod next_node_kinds_tests;
mod next_tests;
mod note_agent_tests;
mod note_command_tests;
mod note_reset_tests;
mod note_store_support;
mod operator_attended_tests;
mod operator_basin_tests;
mod operator_consumer_tests;
mod operator_force_support;
mod operator_force_tests;
mod operator_recovery_tests;
mod operator_unwritable_state_home_tests;
mod parallel_target_override_tests;
mod poll_resume_program_tests;
mod poll_wait_outcome_tests;
mod provenance_support;
mod provenance_tests;
mod provider_limit_cleanup_tests;
mod provider_limit_codex_tests;
mod provider_limit_compatibility_tests;
mod provider_limit_parking_tests;
mod provider_limit_poll_tests;
mod provider_limit_recognition_tests;
mod provider_limit_scheduling_tests;
mod provider_limit_stream_json_tests;
mod provider_limit_support;
mod registry_location_tests;
mod remove_resolution_tests;
mod remove_retirement_tests;
mod remove_tests;
mod rendered_stderr;
mod retry_fanout_tests;
mod retry_runtime_precedence_tests;
mod roster_behavior_tests;
mod roster_json_tests;
mod roster_scope_tests;
mod roster_support;
mod run_id_conflict_tests;
mod run_lock_wait_tests;
mod run_report_driver_labels_tests;
mod run_result_directory_tests;
mod run_shell_program_tests;
mod run_shutdown_delivery_tests;
mod run_signals_tests;
mod run_target_cli_override_tests;
mod run_target_cli_timeout_tests;
mod run_tests;
mod runs_history_tests;
mod seal_command_tests;
mod self_advancing_agent_tests;
mod session_continuation_selection_tests;
mod session_continuation_support;
mod session_continuation_tests;
mod session_continuation_validation_tests;
mod shape_example_sync_tests;
mod shape_example_terminal_tests;
mod show_json_tests;
mod show_support;
mod show_tests;
mod snapshot_codex_session_tests;
mod snapshot_continue_root_tests;
mod snapshot_prior_authoring_tests;
mod snapshot_prior_boundaries_tests;
mod snapshot_prior_fix_tests;
mod snapshot_prior_inheritance_tests;
mod snapshot_prior_selection_tests;
mod snapshot_tests;
mod spawn_record_malformed_tests;
mod state_machine_placement_tests;
mod state_machine_resolution_support;
mod state_machine_resolution_tests;
mod states_line_refused_tests;
mod strict_inputs_admission_tests;
mod strict_inputs_exit_tests;
mod strict_inputs_support;
mod strict_inputs_tests;
mod suite_isolation_tests;
mod summary_repricing_lock_tests;
mod summary_repricing_support;
mod summary_repricing_tests;
mod summary_tests;
mod supervised_delivery_tests;
mod supervision_appended_child_tests;
mod supervision_barrier_tests;
mod supervision_empty_visit_release_tests;
mod supervision_empty_visit_tests;
mod supervision_gated_finish_tests;
mod supervision_manual_release_guard_tests;
mod supervision_next_tests;
mod supervision_no_spawn_release_tests;
mod supervision_release_rule_tests;
mod supervision_retry_refusal_tests;
mod supervision_retry_support;
mod supervision_spent_visit_retry_tests;
mod supervision_surfaces_tests;
mod supervision_tests;
mod supervisor_validation_scope_tests;
mod supervisor_validation_tests;
mod task_export_consumer_tests;
mod task_export_producer_tests;
mod task_export_validation_tests;
mod task_tooling_surface_tests;
mod task_tooling_tests;
mod template_ancestor_discovery_tests;
mod template_example_sync_tests;
mod template_sandbox_sweep_support;
mod template_sandbox_sweep_tests;
mod template_timeout_tests;
mod template_transition_guard_tests;
mod templates_render_tests;
mod templates_tests;
mod terminal_result_attempt_tests;
mod terminal_result_fanout_tests;
mod terminal_result_redirect_tests;
mod terminal_result_stall_tests;
mod terminal_result_tests;
mod terminal_watch_support;
mod terminal_watch_tests;
mod timeout_charge_halt_tests;
mod timeout_preflight_tests;
mod transition_tests;
mod transition_writer_lock_tests;
mod union_artifact_coalescing_tests;
mod union_artifact_paths_support;
mod union_artifact_paths_tests;
mod union_artifact_terminal_coalescing_tests;
mod union_shared_input_warning_tests;
mod unrecognized_input_directory_tests;
mod until_idle_deadline_tests;
mod until_idle_drain_tests;
mod until_idle_frontend_tests;
mod until_idle_outcome_tests;
mod until_idle_support;
mod validate_parallel_layout_tests;
mod validate_retry_cache_tests;
mod validation_source_tests;
mod verify_findings_tests;
mod verify_reset_tests;
mod verify_tests;
mod waiting_on_person_tests;
mod worker_capture_precondition_tests;
mod worker_edit_inflight_stop_tests;
mod worker_edit_one_file_support;
mod worker_edit_poll_deadline_tests;
// The controlled read observer uses Linux /proc and ELF interposition only.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod worker_edit_publication_tests;
mod worker_edit_revert_prompt_tests;
mod worker_edit_revert_race_tests;
mod worker_edit_revert_support;
mod worker_edit_revert_tests;
mod worker_edit_stop_help_tests;
mod worker_edit_stop_report_tests;
mod workspace_task_metadata_tests;

/// The plan and machine text most of them start from.
mod shared_fixtures;
pub use shared_fixtures::*;

/// What every harness calls: run the CLI, build a workspace, read and assert
/// on what came back.
mod helpers;
pub use helpers::*;

// Shared with the `integration_markdown_plans` harness, which cannot see this
// module tree and `include!`s the same file.
#[path = "../support/binaries.rs"]
mod binaries;
#[path = "../support/git_env.rs"]
mod git_env;
#[path = "../support/python_fixture.rs"]
mod python_fixture;
#[path = "../support/test_dir.rs"]
mod test_dir;
#[path = "../support/unique_dir_name.rs"]
mod unique_dir_name;

pub use git_env::{
    git_command, git_command_as_if_inherited, git_command_with_staged_env, git_init_or_absent,
    repository_env_as_a_hook_leaves_it, repository_env_names_no_report_mentioned,
};
pub use python_fixture::{
    fixture_command, fixture_command_line, fixture_command_with_args, python_command,
    write_python_agent,
};

/// A `cli:` callback that runs one line of Python, spelled as a YAML scalar.
///
/// A callback is a command line for the platform's own shell, and the two
/// shells share almost no vocabulary — `printf` is `sh`, not `cmd`. The code
/// goes inside one pair of double quotes and quotes its own strings with
/// `'…'`, which both shells hand through unchanged, and `serde_json` spells
/// the result as a YAML double-quoted scalar.

// §FS-rhei-programs.1.1
pub fn python_callback_yaml(code: &str) -> String {
    serde_json::to_string(&format!("cli:{} -c \"{code}\"", python_command()))
        .expect("callback should serialize")
}
pub use binaries::rhei_binary;
pub use test_dir::TestDir;

/// The product's own quoting, so a test builds an expected command line the way
/// the product built it — POSIX quotes on Unix, `cmd`'s on Windows — instead of
/// dropping the assertion to a substring because the two platforms disagree.
// §FS-rhei-errors.2
pub use rhei_core::platform::shell_quote;

// Every scenario that writes `use super::*` reads these through it.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
