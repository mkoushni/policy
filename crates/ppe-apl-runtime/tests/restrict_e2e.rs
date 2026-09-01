// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Praxis Contributors

// End-to-end: an APL route with `restrict` effects, driven through the
// real PolicyEngine + APL visitor, must fold the emitted constraints
// and surface them on the typed `candidate_constraint` extension slot
// that the host router reads off `PipelineResult.modified_extensions`.
// A `custom`-label contradiction must fail closed.

#![allow(
    missing_docs,
    clippy::needless_raw_string_hashes,
    clippy::field_reassign_with_default,
    clippy::needless_raw_strings,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::unwrap_used,
    reason = "test and example code"
)]
use std::sync::Arc;

use praxis_policy_core::cmf::enums::Role;
use praxis_policy_core::cmf::{CmfHook, Message, MessagePayload};
use praxis_policy_core::engine::PolicyEngine;
use praxis_policy_core::extensions::{
    CandidateConstraintExtension, Extensions, MetaExtension, OnEmpty,
};

use praxis_policy_apl_runtime::{AplOptions, DispatchCache, MemorySessionStore, register_apl};

fn cmf_payload(text: &str) -> MessagePayload {
    MessagePayload {
        message: Message::text(Role::User, text),
    }
}

fn meta_for_tool(name: &str) -> MetaExtension {
    let mut meta = MetaExtension::default();
    meta.entity_type = Some("tool".to_owned());
    meta.entity_name = Some(name.to_owned());
    meta
}

/// Build a engine wired with the APL visitor from `yaml`. `restrict`
/// needs no plugins of its own, so no factories are registered.
async fn build_manager(yaml: &str) -> Arc<PolicyEngine> {
    let mgr = Arc::new(PolicyEngine::default());
    register_apl(
        &mgr,
        AplOptions {
            dispatch_cache: Arc::new(DispatchCache::new()),
            session_store: Arc::new(MemorySessionStore::new()),
            pdps: Vec::new(),
            pdp_factories: Vec::new(),
            session_store_factories: Vec::new(),
            base_capabilities: None,
        },
    );
    mgr.load_config_yaml(yaml).expect("load_config_yaml");
    mgr.initialize().await.expect("initialize");
    mgr
}

/// Read the folded constraint off the merged extensions' typed slot.
fn constraint(ext: &Extensions) -> Option<CandidateConstraintExtension> {
    ext.candidate_constraint.as_ref().map(|arc| (**arc).clone())
}

/// A single unconditional `restrict` emits its constraint on
/// `candidate_constraint`, and the request still continues (restrict
/// never denies).
#[tokio::test]
async fn restrict_emits_constraint_on_side_channel() {
    const YAML: &str = r#"
engine_settings:
  dispatch: policy
routes:
  - tool: infer
    authorization:
      pre_invocation:
        - restrict: { allow_regions: [eu] }
"#;
    let mgr = build_manager(YAML).await;

    let ext = Extensions {
        meta: Some(Arc::new(meta_for_tool("infer"))),
        ..Default::default()
    };
    let (result, _bg) = mgr
        .invoke_named::<CmfHook>("cmf.tool_pre_invoke", cmf_payload("hi"), ext, None)
        .await;

    assert!(
        result.continue_processing,
        "restrict never denies: violation = {:?}",
        result.violation
    );
    let merged = result
        .modified_extensions
        .expect("restrict must surface a constraint via modified_extensions");
    let c = constraint(&merged).expect("candidate_constraint slot must be set");
    assert_eq!(c.allow_regions.as_deref(), Some(&["eu".to_owned()][..]));
    assert_eq!(c.on_empty, OnEmpty::Deny);
}

/// Two restricts in the same phase fold: allow-sets intersect, deny-sets
/// union, and the blob is the single folded result.
#[tokio::test]
async fn two_restricts_fold_into_one_blob() {
    const YAML: &str = r#"
engine_settings:
  dispatch: policy
routes:
  - tool: infer
    authorization:
      pre_invocation:
        - restrict: { allow_models: ["vllm/*", "anthropic/*"], deny_models: ["openai/*"] }
        - restrict: { allow_models: ["anthropic/*", "cohere/*"] }
"#;
    let mgr = build_manager(YAML).await;

    let ext = Extensions {
        meta: Some(Arc::new(meta_for_tool("infer"))),
        ..Default::default()
    };
    let (result, _bg) = mgr
        .invoke_named::<CmfHook>("cmf.tool_pre_invoke", cmf_payload("hi"), ext, None)
        .await;

    assert!(result.continue_processing);
    let merged = result.modified_extensions.expect("modified_extensions");
    let c = constraint(&merged).expect("candidate_constraint slot");
    assert_eq!(
        c.allow_models.as_deref(),
        Some(&["anthropic/*".to_owned()][..])
    ); // intersection
    assert_eq!(c.deny_models, vec!["openai/*".to_owned()]); // union
    assert_eq!(c.on_empty, OnEmpty::Deny);
}

/// A `when`-gated restrict that does NOT fire (gate false) emits no blob.
#[tokio::test]
async fn gated_restrict_absent_when_gate_false() {
    const YAML: &str = r#"
engine_settings:
  dispatch: policy
routes:
  - tool: infer
    authorization:
      pre_invocation:
        - when: "session.labels contains 'eu_resident'"
          do:
            - restrict: { allow_regions: [eu] }
"#;
    let mgr = build_manager(YAML).await;

    // No `eu_resident` label on the session → gate is false.
    let ext = Extensions {
        meta: Some(Arc::new(meta_for_tool("infer"))),
        ..Default::default()
    };
    let (result, _bg) = mgr
        .invoke_named::<CmfHook>("cmf.tool_pre_invoke", cmf_payload("hi"), ext, None)
        .await;

    assert!(result.continue_processing);
    // Either no modified_extensions at all, or one with an empty slot.
    let has_constraint = result
        .modified_extensions
        .as_ref()
        .and_then(constraint)
        .is_some();
    assert!(
        !has_constraint,
        "gate was false — no constraint should be emitted"
    );
}

/// Two restricts requiring the same `custom` label to differ is an
/// unsatisfiable contradiction — the request fails closed with a
/// `policy.restrict_conflict` violation.
#[tokio::test]
async fn conflicting_custom_labels_fail_closed() {
    const YAML: &str = r#"
engine_settings:
  dispatch: policy
routes:
  - tool: infer
    authorization:
      pre_invocation:
        - restrict: { custom: { gpu: h100 } }
        - restrict: { custom: { gpu: a100 } }
"#;
    let mgr = build_manager(YAML).await;

    let ext = Extensions {
        meta: Some(Arc::new(meta_for_tool("infer"))),
        ..Default::default()
    };
    let (result, _bg) = mgr
        .invoke_named::<CmfHook>("cmf.tool_pre_invoke", cmf_payload("hi"), ext, None)
        .await;

    assert!(
        !result.continue_processing,
        "contradictory custom labels must fail closed"
    );
    let violation = result.violation.expect("conflict must surface a violation");
    assert_eq!(violation.code, "policy.restrict_conflict");
}

/// Restricts in separate `parallel:` branches both merge back into the
/// single folded constraint — accumulation survives the fan-out, driven
/// end-to-end through the pipeline (not just the evaluator unit path).
#[tokio::test]
async fn parallel_branch_restricts_accumulate() {
    const YAML: &str = r#"
engine_settings:
  dispatch: policy
routes:
  - tool: infer
    authorization:
      pre_invocation:
        - parallel:
            - restrict: { allow_regions: [eu, us] }
            - restrict: { deny_models: ["openai/*"] }
"#;
    let mgr = build_manager(YAML).await;

    let ext = Extensions {
        meta: Some(Arc::new(meta_for_tool("infer"))),
        ..Default::default()
    };
    let (result, _bg) = mgr
        .invoke_named::<CmfHook>("cmf.tool_pre_invoke", cmf_payload("hi"), ext, None)
        .await;

    assert!(result.continue_processing);
    let merged = result.modified_extensions.expect("modified_extensions");
    let c = constraint(&merged).expect("candidate_constraint slot");
    // Branch A's allow_regions and branch B's deny_models both survive the
    // fold (order-agnostic — the fold may reorder).
    let mut regions = c
        .allow_regions
        .clone()
        .expect("allow_regions from branch A");
    regions.sort();
    assert_eq!(regions, vec!["eu".to_owned(), "us".to_owned()]);
    assert_eq!(c.deny_models, vec!["openai/*".to_owned()]);
}
