// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Praxis Contributors

//! Safety invariant catalog for plugin dispatch.
//!
//! One table: every dispatch phase × {none, error, hang, panic}, with
//! `on_error: fail`. Each cell asserts the decision, not merely that no
//! allow was returned. A new [`PluginMode`] variant fails to compile
//! until [`expected_plugin_verdict`] gains an arm.
//!
//! `docs/safety-invariants.md` is the prose form of this table.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test and example code"
)]

use praxis_policy_core::config::parse_config;
use praxis_policy_core::executor::{Executor, ExecutorConfig};
use praxis_policy_core::fault_testing::{
    ExpectedVerdict, InjectedFailure, dispatch_modes, expected_plugin_verdict, fault_entry,
};
use praxis_policy_core::hooks::payload::{Extensions, PluginPayload};
use praxis_policy_core::plugin::OnError;

#[derive(Debug, Clone)]
#[allow(
    dead_code,
    reason = "test fixture — typed shape is the point, not field reads"
)]
struct TestPayload {
    value: String,
}
praxis_policy_core::impl_plugin_payload!(TestPayload);

#[test]
fn plugin_fault_catalog_covers_every_dispatch_mode() {
    let modes = dispatch_modes();
    assert_eq!(
        modes.len(),
        5,
        "five dispatch phases; Disabled is not a phase"
    );
    for mode in modes {
        assert!(mode.is_dispatch_phase(), "{mode} must be a dispatch phase");
    }
}

#[tokio::test(start_paused = true)]
async fn plugin_fault_catalog_asserts_the_safe_verdict() {
    let tracker = tokio_util::task::TaskTracker::new();
    for mode in dispatch_modes() {
        for failure in InjectedFailure::all() {
            let expected = expected_plugin_verdict(mode, failure);
            let executor = Executor::new(ExecutorConfig {
                timeout_seconds: 1,
                short_circuit_on_deny: true,
            });
            let entry = fault_entry("fault", mode, OnError::Fail, failure);
            let payload: Box<dyn PluginPayload> = Box::new(TestPayload { value: "x".into() });
            let (result, bg) = executor
                .execute(
                    std::slice::from_ref(&entry),
                    payload,
                    Extensions::default(),
                    None,
                    &tracker,
                )
                .await;

            match expected {
                ExpectedVerdict::Allow => {
                    assert!(
                        result.continue_processing,
                        "{mode:?} × {failure:?}: expected allow"
                    );
                    assert!(result.violation.is_none(), "{mode:?} × {failure:?}");
                    let bg_errors = bg.wait_for_background_tasks().await;
                    assert!(
                        bg_errors.is_empty(),
                        "{mode:?} × {failure:?}: background errors {bg_errors:?}"
                    );
                },
                ExpectedVerdict::Halt { code } => {
                    assert!(
                        !result.continue_processing,
                        "{mode:?} × {failure:?}: expected deny, got allow"
                    );
                    let v = result
                        .violation
                        .as_ref()
                        .expect("a halt must carry a violation");
                    assert_eq!(v.code, code, "{mode:?} × {failure:?}");
                    assert_eq!(v.plugin_name.as_deref(), Some("fault"));
                    let _ = bg.wait_for_background_tasks().await;
                },
                ExpectedVerdict::Continue { record_code } => {
                    assert!(
                        result.continue_processing,
                        "{mode:?} × {failure:?}: non-blocking phase must not halt"
                    );
                    assert!(
                        result.violation.is_none(),
                        "{mode:?} × {failure:?}: continue is not a deny"
                    );
                    assert_eq!(
                        result.errors.len(),
                        1,
                        "{mode:?} × {failure:?}: the failure must be recorded"
                    );
                    assert_eq!(
                        result.errors[0].code.as_deref(),
                        record_code,
                        "{mode:?} × {failure:?}"
                    );
                    let _ = bg.wait_for_background_tasks().await;
                },
                ExpectedVerdict::AllowThenBackgroundPanic => {
                    assert!(
                        result.continue_processing,
                        "{mode:?} × {failure:?}: fire-and-forget cannot change the verdict"
                    );
                    let bg_errors = bg.wait_for_background_tasks().await;
                    assert_eq!(
                        bg_errors.len(),
                        1,
                        "{mode:?} × {failure:?}: panic must surface on wait, got {bg_errors:?}"
                    );
                },
            }
        }
    }
}

#[tokio::test]
async fn empty_plugin_list_allows() {
    let executor = Executor::default();
    let tracker = tokio_util::task::TaskTracker::new();
    let payload: Box<dyn PluginPayload> = Box::new(TestPayload { value: "x".into() });
    let (result, bg) = executor
        .execute(&[], payload, Extensions::default(), None, &tracker)
        .await;
    assert!(result.continue_processing);
    assert!(result.violation.is_none());
    assert!(bg.wait_for_background_tasks().await.is_empty());
}

#[test]
fn malformed_config_fails_the_load() {
    let err = parse_config("global:\n  not_a_real_key: true\n")
        .expect_err("an unknown key must fail the load");
    let text = err.to_string();
    assert!(
        text.contains("not_a_real_key") || text.contains("unknown"),
        "the load error must name the bad key or that it is unknown: {text}"
    );
}
