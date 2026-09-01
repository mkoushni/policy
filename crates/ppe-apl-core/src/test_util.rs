// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Praxis Contributors

//! Test scaffolding for compiling one policy block beside the plugin
//! declarations its steps name.
//!
//! Behind the `test-util` feature, so it stays out of the semver-bound
//! published surface. A test in another crate sees only the public API, which is
//! why this is public rather than a `#[cfg(test)]` helper.
//!
//! # Why this exists
//!
//! The parser used to carry a second whole-document shape, `compile_config`,
//! whose `routes:` was a map keyed by route name. Nothing in production called
//! it: the runtime compiles a section's policy block through
//! [`crate::compile_policy_block_value`], and a real document writes `routes:`
//! as a list of selectors. Two `routes:` shapes in one project is one too many,
//! and the one no host used was the one tests read, so a test could pass against
//! a document no deployment could load.
//!
//! What the tests actually wanted was narrower than a config: one compiled route
//! plus the registry a dispatcher resolves plugin names against. That is what
//! this returns.

use crate::parser::ParseError;
use crate::plugin_decl::{PluginDeclaration, PluginRegistry};
use crate::rules::CompiledRoute;

/// A compiled policy block and the plugin registry its steps name.
///
/// The two travel together because a dispatcher needs both: the route supplies
/// the steps, the registry supplies each named plugin's hook and kind.
#[derive(Debug, Default)]
pub struct TestPolicy {
    /// The compiled block.
    pub route: CompiledRoute,
    /// Declarations the block's steps refer to by name.
    pub plugins: PluginRegistry,
}

/// A test document: a root `plugins:` list beside one policy block under
/// `route:`.
///
/// `route:` rather than `routes:` is deliberate. A document here carries exactly
/// one block, so there is no map to key, and nothing defines a second shape for
/// the `routes:` spelling a real config owns.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TestDocument {
    /// Root `plugins:` block, full declarations.
    #[serde(default)]
    plugins: Vec<PluginDeclaration>,

    /// The policy block: `args:` / `authorization:` / `result:` / `plugins:`.
    #[serde(default)]
    route: Option<serde_yaml::Value>,
}

/// Compile a test document into one route plus its plugin registry.
///
/// `source` is the diagnostic path prefix baked into each rule's and pipeline's
/// `source`, the same way a section name is for a real load.
///
/// Unlike the deleted `compile_config`, this does **not** drop a block that
/// declares no APL term. It returns an empty [`CompiledRoute`], matching
/// [`crate::compile_policy_block_value`], so a test asserting that a block
/// carries no policy checks `route.declared_phases().is_empty()` rather than a
/// route's absence from a map.
///
/// A duplicate plugin name is last-one-wins, as the deleted function had it.
///
/// # Errors
///
/// Returns `ParseError::Yaml` when the document does not deserialize, or the
/// per-rule error from a rule or pipeline that fails to compile.
pub fn compile_test_policy(source: &str, yaml: &str) -> Result<TestPolicy, ParseError> {
    let doc: TestDocument = serde_yaml::from_str(yaml)?;
    let route = match doc.route {
        Some(block) => crate::parser::compile_policy_block_value(source, &block)?,
        None => CompiledRoute::new(source),
    };
    let mut plugins = PluginRegistry::with_capacity(doc.plugins.len());
    for decl in doc.plugins {
        plugins.insert(decl.name.clone(), decl);
    }
    Ok(TestPolicy { route, plugins })
}

/// Compile just the policy block, for a test with no plugin declarations to
/// make.
///
/// # Errors
///
/// As [`compile_test_policy`].
pub fn compile_test_route(source: &str, yaml: &str) -> Result<CompiledRoute, ParseError> {
    Ok(compile_test_policy(source, yaml)?.route)
}
