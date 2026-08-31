// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Praxis Contributors

// praxis-policy-plugin-pii-scanner — CMF `HookHandler` that walks the message's
// ToolCall / PromptRequest argument map and tests each string value
// against configured PII patterns. Modes:
//
//   * `deny`   — return `pii.detected` violation; gateway 403s
//   * `taint`  — emit a session taint label (downstream policy can
//                gate via `session.labels contains 'PII'`)
//   * `redact` — replace matching values with `[PII]` and continue
//
// Operators wire it as a `policy:` step:
//
//   policy:
//     - "require(perm.email_send)"
//     - "run(pii-scan)"
//
// The plugin registers on whichever CMF pre-invoke hooks the
// operator declares in YAML (tool / prompt / llm / resource).

//! Scans tool and prompt arguments for configured PII patterns.
//!
//! Three modes decide what a match means: `deny` rejects the request, `taint`
//! labels the session so later policy can gate on it, and `redact` replaces the
//! value and continues.

/// Plugin configuration, including the patterns and the mode.
pub mod config;
/// Constructs the scanner from configuration.
pub mod factory;
/// The CMF hook handler that walks arguments and applies the mode.
pub mod scanner;

pub use config::{PiiPattern, PiiScanMode, PiiScannerConfig};
pub use factory::{KIND, PiiScannerFactory};
pub use scanner::PiiScanner;
