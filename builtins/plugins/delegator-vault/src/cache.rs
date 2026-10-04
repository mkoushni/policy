// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Praxis Contributors

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};

use praxis_policy_core::delegation::DelegationSubject;
use praxis_policy_core::error::PluginViolation;
use praxis_policy_core::extensions::raw_credentials::RawDelegatedToken;

use crate::config::CacheConfig;

/// What one Vault KV read produced.
#[derive(Debug)]
pub(crate) struct Mint {
    pub token: RawDelegatedToken,
    pub secret_version: u64,
}

/// Whether a `Served` came from the cache or a fresh Vault read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Cache,
    Mint,
}

/// The credential delivered to the handler, with provenance.
#[derive(Debug)]
pub(crate) struct Served {
    pub mint: Mint,
    pub source: Source,
    pub minted_at: DateTime<Utc>,
}

/// Bounded, coalescing credential cache keyed by principal identity.
///
/// `try_get_with` coalesces concurrent requests for the same key:
/// one caller fetches from Vault while the others wait, and all
/// receive the same result. A failed fetch is not cached.
pub(crate) struct CredentialCache {
    inner: moka::future::Cache<String, CachedMint>,
    config: CacheConfig,
}

#[derive(Clone)]
struct CachedMint {
    token: RawDelegatedToken,
    secret_version: u64,
    minted_at: DateTime<Utc>,
}

impl CredentialCache {
    /// Build a cache from config, or `None` if caching is disabled.
    pub(crate) fn new(config: &CacheConfig) -> Result<Option<Self>, String> {
        if !config.enabled {
            return Ok(None);
        }
        config.validate()?;

        let inner = moka::future::Cache::builder()
            .max_capacity(config.max_entries)
            .time_to_live(Duration::from_secs(config.ttl_seconds))
            .build();

        Ok(Some(Self {
            inner,
            config: config.clone(),
        }))
    }

    /// Cache key combining subject variant and identity claim value.
    pub(crate) fn cache_key(subject: &DelegationSubject, identity: &str) -> String {
        let tag = match subject {
            DelegationSubject::User => "u",
            DelegationSubject::Client => "c",
            DelegationSubject::CallerWorkload => "w",
            DelegationSubject::ThisWorkload => "t",
            _ => "?",
        };
        format!("{tag}:{identity}")
    }

    /// Get a cached credential or mint a new one.
    ///
    /// On cache hit, returns `Source::Cache`. On miss, evaluates
    /// `mint_fn` (coalesced with concurrent callers for the same key),
    /// stores the result, and returns `Source::Mint`.
    ///
    /// A failed mint propagates the error to all waiting callers and
    /// is not cached.
    pub(crate) async fn get_or_mint<F>(
        &self,
        key: String,
        mint_fn: F,
    ) -> Result<Served, Arc<PluginViolation>>
    where
        F: std::future::Future<Output = Result<Mint, PluginViolation>>,
    {
        let entry = self
            .inner
            .entry(key)
            .or_try_insert_with(async {
                mint_fn.await.map(|mint| CachedMint {
                    token: mint.token,
                    secret_version: mint.secret_version,
                    minted_at: Utc::now(),
                })
            })
            .await
            .map_err(|e| Arc::new((*e).clone()))?;

        let fresh = entry.is_fresh();
        let value = entry.into_value();

        Ok(Served {
            mint: Mint {
                token: value.token,
                secret_version: value.secret_version,
            },
            source: if fresh { Source::Mint } else { Source::Cache },
            minted_at: value.minted_at,
        })
    }

    /// The configured TTL, used for `expires_at` computation.
    pub(crate) fn ttl_seconds(&self) -> u64 {
        self.config.ttl_seconds
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, reason = "tests")]
mod tests {
    use super::*;

    fn test_config() -> CacheConfig {
        CacheConfig {
            enabled: true,
            ttl_seconds: 60,
            max_entries: 100,
        }
    }

    fn make_mint(token_value: &str) -> Mint {
        Mint {
            token: RawDelegatedToken::new(
                token_value,
                "Authorization",
                "https://api.test",
                Vec::new(),
                Utc::now() + chrono::Duration::seconds(60),
            ),
            secret_version: 1,
        }
    }

    #[test]
    fn disabled_returns_none() {
        let cfg = CacheConfig::default();
        assert!(CredentialCache::new(&cfg).unwrap().is_none());
    }

    #[test]
    fn enabled_returns_some() {
        assert!(CredentialCache::new(&test_config()).unwrap().is_some());
    }

    #[test]
    fn cache_key_includes_subject_tag() {
        let k1 = CredentialCache::cache_key(&DelegationSubject::User, "alice");
        let k2 = CredentialCache::cache_key(&DelegationSubject::Client, "alice");
        assert_ne!(k1, k2);
        assert!(k1.starts_with("u:"));
        assert!(k2.starts_with("c:"));
    }

    #[tokio::test]
    async fn second_call_hits_cache() {
        let cache = CredentialCache::new(&test_config()).unwrap().unwrap();
        let key = "u:alice".to_owned();

        let call_count = std::sync::atomic::AtomicU32::new(0);

        let r1 = cache
            .get_or_mint(key.clone(), async {
                call_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok(make_mint("ghp_first"))
            })
            .await
            .unwrap();
        assert_eq!(r1.source, Source::Mint);
        assert_eq!(&*r1.mint.token.token, "ghp_first");

        let r2 = cache
            .get_or_mint(key, async {
                call_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(make_mint("ghp_second"))
            })
            .await
            .unwrap();
        assert_eq!(r2.source, Source::Cache);
        assert_eq!(&*r2.mint.token.token, "ghp_first");

        assert_eq!(call_count.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn failed_mint_not_cached() {
        let cache = CredentialCache::new(&test_config()).unwrap().unwrap();
        let key = "u:bob".to_owned();

        let err = cache
            .get_or_mint(key.clone(), async {
                Err(PluginViolation::new("test.fail", "boom"))
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, "test.fail");

        let ok = cache
            .get_or_mint(key, async { Ok(make_mint("ghp_recovered")) })
            .await
            .unwrap();
        assert_eq!(&*ok.mint.token.token, "ghp_recovered");
    }

    #[tokio::test]
    async fn different_keys_isolated() {
        let cache = CredentialCache::new(&test_config()).unwrap().unwrap();

        cache
            .get_or_mint("u:alice".into(), async { Ok(make_mint("alice-token")) })
            .await
            .unwrap();

        cache
            .get_or_mint("u:bob".into(), async { Ok(make_mint("bob-token")) })
            .await
            .unwrap();

        let alice = cache
            .get_or_mint("u:alice".into(), async { panic!("should not be called") })
            .await
            .unwrap();
        let bob = cache
            .get_or_mint("u:bob".into(), async { panic!("should not be called") })
            .await
            .unwrap();

        assert_eq!(&*alice.mint.token.token, "alice-token");
        assert_eq!(&*bob.mint.token.token, "bob-token");
    }
}
