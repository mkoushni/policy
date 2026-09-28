// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Praxis Contributors

//! Live Vault KV v2 coverage. See `docs/content/testing.md` for provisioning.

#![cfg(all(feature = "secrets-vault", feature = "http-hyper"))]
#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code"
)]

use std::env;
use std::sync::Arc;

use praxis_policy::{HyperTransport, SecretProviderFactory as _, VaultSecretProviderFactory};
use praxis_policy_core::http::HttpTransport;
use praxis_policy_core::secrets::SecretProviderConfig;

#[tokio::test]
#[ignore = "requires a provisioned Vault server; see docs/content/testing.md"]
async fn vault_kv_v2_reads_live_and_maps_soft_deleted_versions() {
    let (Ok(address), Ok(role_id), Ok(secret_id)) = (
        env::var("VAULT_ADDR"),
        env::var("VAULT_ROLE_ID"),
        env::var("VAULT_SECRET_ID"),
    ) else {
        return;
    };
    let transport: Arc<dyn HttpTransport> =
        Arc::new(HyperTransport::new().with_allow_private_destinations());
    let factory = VaultSecretProviderFactory::new(transport);
    let settings = serde_yaml::from_str(&format!(
        "address: {address}\nauth:\n  method: approle\n  role_id: {role_id}\n  secret_id:\n    literal: {secret_id}\nallow_insecure_literal: true\n"
    ))
    .expect("settings");
    let provider = factory
        .build(&SecretProviderConfig {
            kind: "vault".to_owned(),
            settings,
        })
        .expect("provider");

    assert_eq!(
        provider
            .get_secret("secret/live#password")
            .await
            .expect("live read")
            .as_str(),
        "live-value"
    );
    let deleted = provider
        .get_secret("secret/live-deleted#password")
        .await
        .expect_err("soft-deleted version must be absent");
    assert!(
        matches!(deleted, praxis_policy::SecretError::NotFound { .. }),
        "{deleted}"
    );
}
