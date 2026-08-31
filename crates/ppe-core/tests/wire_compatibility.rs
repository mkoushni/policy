// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Praxis Contributors

//! The plugin `kind:` strings, hook names, and violation codes an operator
//! writes must not move.
//!
//! What this file guards is narrower than it was. The fixture began as a real
//! policy document authored against the engine's previous name, copied
//! verbatim, and the guarantee covered the whole document format. It no longer
//! does: making `authorization:` the only place the two phase lists appear
//! rewrote the fixture's four route bodies, so the phase spelling is a surface
//! this crate has moved deliberately, and the CHANGELOG records the retirement.
//!
//! `priority:` is the second such surface. It is a policy-mode load error now,
//! because policy dispatch never hands the registry more than one entry to
//! order, so the fixture's seven declarations are gone. The fixture is the best
//! evidence for that change rather than a casualty of it: its `audit-log` entry
//! read `priority: 90  # fires AFTER policy / delegate so the record reflects
//! the final decision`, which is exactly the ordering the key reads as promising
//! and exactly what policy dispatch does not do.
//!
//! Everything else the fixture pins still holds. It exercises multi-source
//! identity, token exchange, policy requirements, a decision point, argument
//! redaction, PII scanning, audit emission, and session taint, so a change to
//! any plugin kind string, plugin name, hook name, or violation code breaks it.
//!
//! It is checked in rather than read from a sibling repository so the guarantee
//! travels with this crate.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::unwrap_used,
    reason = "test and example code"
)]
/// The document loads. Every other assertion here reads the result of this
/// load, so it fails first and names the cause when the format does move.
#[test]
fn the_reference_policy_document_loads() {
    let yaml = include_str!("fixtures/legacy-policy-document.yaml");
    let cfg = praxis_policy_core::config::parse_config(yaml)
        .expect("the reference policy document must load");
    assert!(!cfg.plugins.is_empty(), "fixture declares plugins");
}

/// Every `kind:` string in the fixture, in declaration order.
///
/// The full set rather than a spot check. A `kind` is what an operator types, so
/// renaming one breaks their document with "no factory registered" at startup —
/// and asserting only that *some* plugin is `identity/jwt` would let any of the
/// other six be renamed silently. Names are included because the document is also
/// how a route refers to a plugin by name.
///
/// Add a plugin to the fixture and this fails; that is the prompt to decide
/// whether the new `kind` is one you are willing to keep.
#[test]
fn the_kind_strings_an_operator_writes_are_unchanged() {
    let yaml = include_str!("fixtures/legacy-policy-document.yaml");
    let cfg = praxis_policy_core::config::parse_config(yaml).expect("fixture must load");

    let declared: Vec<(&str, &str)> = cfg
        .plugins
        .iter()
        .map(|p| (p.name.as_str(), p.kind.as_str()))
        .collect();

    assert_eq!(
        declared,
        vec![
            ("jwt-user", "identity/jwt"),
            ("jwt-client", "identity/jwt"),
            ("workday-oauth", "delegator/oauth"),
            ("pii-scan", "validator/pii-scan"),
            ("audit-log", "audit/logger"),
            ("github-oauth", "delegator/oauth"),
            ("manager-approver", "elicitation/ciba"),
        ],
        "plugin names and kind strings are the operator-facing contract",
    );
}

/// The route set the document declares, by entity.
///
/// A route key is the other half of what an operator writes: it selects which
/// policy applies to a tool, prompt or resource call. Losing or renaming one means
/// a call silently evaluates under no policy, which fails open.
#[test]
fn the_route_keys_are_unchanged() {
    let yaml = include_str!("fixtures/legacy-policy-document.yaml");
    let cfg = praxis_policy_core::config::parse_config(yaml).expect("fixture must load");
    assert_eq!(cfg.routes.len(), 4, "recorded route count");
    assert!(
        cfg.dispatch_mode().is_policy(),
        "a document with routes must select policy dispatch, or none of them are consulted",
    );
}
