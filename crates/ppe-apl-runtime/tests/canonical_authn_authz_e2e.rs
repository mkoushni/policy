// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Praxis Contributors

// End-to-end reference for the canonical APL config shape: a route that
// declares `authentication:` and `authorization:` as sibling blocks. This is
// the form the docs teach; this test proves it loads and runs both phases:
//
//   routes:
//     - tool: get_compensation
//       authentication:            # praxis-policy-core identity dispatch (identity.resolve)
//         - corp-jwt
//       authorization:             # praxis-policy-apl-core authorization phases
//         pre_invocation:
//           - "run(audit-log)"
//
// The two blocks are handled by different layers — `authentication:` binds
// identity plugins for the `identity.resolve` hook (praxis-policy-core), while
// `authorization:` compiles into the APL route handler the visitor installs
// on `cmf.tool_pre_invoke` (praxis-policy-apl-runtime). This test exercises
// both from one loaded config.

#![allow(
    missing_docs,
    clippy::field_reassign_with_default,
    trivial_casts,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::unwrap_used,
    reason = "test and example code"
)]
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use praxis_policy_core::cmf::enums::Role;
use praxis_policy_core::cmf::{CmfHook, Message, MessagePayload};
use praxis_policy_core::context::PluginContext;
use praxis_policy_core::engine::PolicyEngine;
use praxis_policy_core::error::PluginError as CoreError;
use praxis_policy_core::extensions::MetaExtension;
use praxis_policy_core::factory::{PluginFactory, PluginInstance};
use praxis_policy_core::hooks::adapter::TypedHandlerAdapter;
use praxis_policy_core::hooks::payload::Extensions;
use praxis_policy_core::hooks::trait_def::{HookHandler, PluginResult};
use praxis_policy_core::identity::{
    HOOK_IDENTITY_RESOLVE, IdentityHook, IdentityPayload, TokenSource,
};
use praxis_policy_core::plugin::{Plugin, PluginConfig};
use praxis_policy_core::registry::AnyHookHandler;

use praxis_policy_apl_runtime::{AplOptions, DispatchCache, MemorySessionStore, register_apl};

// ---------------------------------------------------------------------
// `authentication:` side — a minimal identity resolver that records it
// fired, registered under the `identity.resolve` hook.
// ---------------------------------------------------------------------

struct RecordingIdentity {
    cfg: PluginConfig,
    name: String,
    ledger: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Plugin for RecordingIdentity {
    fn config(&self) -> &PluginConfig {
        &self.cfg
    }
}

impl HookHandler<IdentityHook> for RecordingIdentity {
    async fn handle(
        &self,
        _payload: &IdentityPayload,
        _ext: &Extensions,
        _ctx: &mut PluginContext,
    ) -> PluginResult<IdentityPayload> {
        self.ledger.lock().unwrap().push(self.name.clone());
        PluginResult::allow()
    }
}

struct RecordingIdentityFactory {
    ledger: Arc<Mutex<Vec<String>>>,
}

impl PluginFactory for RecordingIdentityFactory {
    fn create(&self, config: &PluginConfig) -> Result<PluginInstance, Box<CoreError>> {
        let plugin = Arc::new(RecordingIdentity {
            cfg: config.clone(),
            name: config.name.clone(),
            ledger: Arc::clone(&self.ledger),
        });
        let adapter: Arc<dyn AnyHookHandler> = Arc::new(
            TypedHandlerAdapter::<IdentityHook, _>::new(Arc::clone(&plugin)),
        );
        Ok(PluginInstance {
            plugin: plugin as Arc<dyn Plugin>,
            handlers: vec![(HOOK_IDENTITY_RESOLVE, adapter)],
        })
    }
}

// ---------------------------------------------------------------------
// `authorization:` side — an allow-through CMF plugin the APL
// `pre_invocation` step invokes via `run(audit-log)`.
// ---------------------------------------------------------------------

struct AllowGate {
    cfg: PluginConfig,
}

#[async_trait]
impl Plugin for AllowGate {
    fn config(&self) -> &PluginConfig {
        &self.cfg
    }
}

impl HookHandler<CmfHook> for AllowGate {
    async fn handle(
        &self,
        _payload: &MessagePayload,
        _extensions: &Extensions,
        _ctx: &mut PluginContext,
    ) -> PluginResult<MessagePayload> {
        PluginResult::allow()
    }
}

struct AllowGateFactory;
impl PluginFactory for AllowGateFactory {
    fn create(&self, config: &PluginConfig) -> Result<PluginInstance, Box<CoreError>> {
        let plugin = Arc::new(AllowGate {
            cfg: config.clone(),
        });
        let adapter: Arc<dyn AnyHookHandler> =
            Arc::new(TypedHandlerAdapter::<CmfHook, _>::new(Arc::clone(&plugin)));
        Ok(PluginInstance {
            plugin: plugin as Arc<dyn Plugin>,
            handlers: vec![("cmf.tool_pre_invoke", adapter)],
        })
    }
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn meta_for_tool(name: &str) -> MetaExtension {
    let mut meta = MetaExtension::default();
    meta.entity_type = Some("tool".to_owned());
    meta.entity_name = Some(name.to_owned());
    meta
}

fn ext_for_tool(name: &str) -> Extensions {
    Extensions {
        meta: Some(Arc::new(meta_for_tool(name))),
        ..Default::default()
    }
}

// The canonical shape under test: `authentication:` and `authorization:`
// as siblings on the route.
const CANONICAL_YAML: &str = r#"
engine_settings:
  dispatch: policy
plugins:
  - name: corp-jwt
    kind: identity-recorder
    hooks: [identity.resolve]
  - name: audit-log
    kind: allow-gate
    hooks: [cmf.tool_pre_invoke]
routes:
  - tool: get_compensation
    authentication:
      - corp-jwt
    authorization:
      pre_invocation:
        - "run(audit-log)"
"#;

async fn build_manager(ledger: Arc<Mutex<Vec<String>>>) -> Arc<PolicyEngine> {
    let mgr = Arc::new(PolicyEngine::default());
    mgr.register_factory(
        "identity-recorder",
        Box::new(RecordingIdentityFactory { ledger }),
    );
    mgr.register_factory("allow-gate", Box::new(AllowGateFactory));

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

    mgr.load_config_yaml(CANONICAL_YAML)
        .expect("canonical authentication:+authorization: config must load");
    mgr.initialize().await.expect("initialize");
    mgr
}

// ---------------------------------------------------------------------
// Scenario
// ---------------------------------------------------------------------

/// The canonical config loads, and BOTH sibling blocks take effect:
/// `authentication:` dispatches the `corp-jwt` identity plugin on
/// `identity.resolve`, and `authorization.pre_invocation` runs the
/// `audit-log` plugin on `cmf.tool_pre_invoke` and allows the request.
#[tokio::test]
async fn canonical_authn_and_authz_blocks_both_run() {
    let ledger = Arc::new(Mutex::new(Vec::new()));
    let mgr = build_manager(Arc::clone(&ledger)).await;

    // authentication: — the route's identity block dispatches corp-jwt.
    let (id_result, _bg) = mgr
        .invoke_named::<IdentityHook>(
            HOOK_IDENTITY_RESOLVE,
            IdentityPayload::new("eyJ.fake.jwt", TokenSource::Bearer),
            ext_for_tool("get_compensation"),
            None,
        )
        .await;
    assert!(
        id_result.continue_processing,
        "identity resolve should continue; violation = {:?}",
        id_result.violation
    );
    assert_eq!(
        *ledger.lock().unwrap(),
        vec!["corp-jwt".to_owned()],
        "the route's `authentication:` block must dispatch corp-jwt",
    );

    // authorization: — the route's pre_invocation runs audit-log → allow.
    let (authz_result, _bg) = mgr
        .invoke_named::<CmfHook>(
            "cmf.tool_pre_invoke",
            MessagePayload {
                message: Message::text(Role::User, "read my compensation"),
            },
            ext_for_tool("get_compensation"),
            None,
        )
        .await;
    assert!(
        authz_result.continue_processing,
        "authorization.pre_invocation should allow; violation = {:?}",
        authz_result.violation
    );
}
