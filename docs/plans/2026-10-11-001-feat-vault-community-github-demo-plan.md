---
title: "demo: Praxis AI + PPE + Vault Community GitHub delegation"
type: feat
status: active
date: 2026-10-11
origin: https://github.com/praxis-proxy/policy/issues/192
blocked_by: https://github.com/praxis-proxy/policy/pull/168
---

# demo: Praxis AI + PPE + Vault Community GitHub delegation

## Summary

Add a runnable `praxis-proxy/demos` example in which Praxis AI classifies an MCP
tool call, PPE authorizes it, `delegator/vault` retrieves the caller's enrolled
GitHub credential from a local Vault Community server, and a small MCP bridge
uses that credential for GitHub `GET /user`. The agent receives the GitHub
identity result and never receives the credential.

The demo implementation belongs in `praxis-proxy/demos`. This repository owns
the tracking issue and this plan because PR #168 supplies the blocked PPE
capability.

## Requirements

- R1. Run locally with Docker, Rust, `curl`, `jq`, and `openssl`; no Kubernetes
  or external IdP is required.
- R2. Pin Vault Community and every source dependency used to build the demo.
- R3. Use local RS256 Alice and Bob JWTs. PPE and Vault validate the same token,
  issuer, audience, signature, and stable `sub` claim.
- R4. Vault policy, not path convention alone, confines each JWT identity to
  `secret/data/agents/<sub>/*`.
- R5. Enrollment reads a GitHub token from `gh auth token`, stdin, or a silent
  prompt. The token never appears in a command-line argument, tracked file,
  response, or log.
- R6. Alice's enrolled request reaches GitHub `/user`; Bob's unenrolled request
  is denied with `delegation.vault_secret_not_found` before the bridge calls
  GitHub.
- R7. A route denied before `delegate(...)` makes no Vault login or KV request.
- R8. `up.sh` starts and smoke-checks the stack; `down.sh` stops processes and
  removes ephemeral keys, JWTs, Vault data, and logs so the next start is clean.
- R9. The README labels Vault dev mode, loopback HTTP, demo root credentials,
  and locally generated signing keys as non-production.
- R10. The demo builds from PR #168 until it ships, then changes to the matching
  released PPE pin without changing the flow.

## Scope Boundaries

- The first version accepts an existing fine-grained, read-only GitHub token.
  GitHub OAuth device flow is deferred because it requires an owned OAuth app,
  enabled device flow, and a client-id lifecycle.
- Vault runs in ephemeral developer mode on loopback. Production TLS, storage,
  unseal, HA, audit retention, and operational hardening are out of scope.
- Dynamic GitHub credentials and an enrollment UI are out of scope.
- The demo does not modify the separate Praxis filter. It configures
  `scheme_prefix: "Bearer "`, and the filter preserves that prefix when it
  constructs GitHub's `Authorization` header.

## Architecture

```text
caller
  | X-User-Token: <demo JWT>
  v
Praxis AI :8090
  mcp -> policy -> router
         | identity/jwt validates demo JWT
         | APL require(...) before delegate(...)
         | delegator/vault logs in with that JWT
         v
Vault Community :8200
  JWT alias sub=alice -> policy permits secret/data/agents/alice/*
         |
         | RawDelegatedToken, Authorization: Bearer <GitHub token>
         v
GitHub MCP bridge :9100
  calls https://api.github.com/user -> returns selected identity fields
```

## Key Technical Decisions

- **A local MCP bridge keeps credential custody visible.** Praxis attaches the
  delegated header to the bridge request. The bridge forwards it to GitHub and
  returns only `login`, `id`, and profile URL; it never serializes or logs the
  header.
- **The same JWT crosses both identity boundaries.** A static public key lets
  the PPE identity plugin and Vault JWT auth verify locally without Keycloak.
  The role binds issuer `https://ppe-vault-demo.local`, audience `vault-demo`,
  and `user_claim=sub`.
- **Vault's alias template is the isolation control.** Setup reads the JWT auth
  mount accessor and installs
  `secret/data/agents/{{identity.entity.aliases.<accessor>.name}}/*`.
- **Enrollment is byte-safe.** `enroll.sh` pipes the credential through `jq`
  and `curl` on stdin; it never places the value in a command argument or writes
  it to disk.
- **The policy gate precedes Vault.** One allowed route delegates; one denied
  route calls `deny(...)` before `delegate(...)`. Vault audit/request counts and
  bridge request counts prove the dependency was not contacted on early deny.
- **The blocker pin is explicit.** The demo gateway resolves PPE from the PR
  #168 branch/commit through `PPE_DIR` or `PPE_GIT_REF`. Release migration is a
  one-line pin change after #168 ships.

## Demo Layout

```text
demos/ppe-vault-delegation/
  README.md
  up.sh
  down.sh
  docker-compose.yml
  praxis.yaml
  policy.yaml.template
  build-gateway.sh
  gateway/{Cargo.toml,src/main.rs}
  bridge/{Dockerfile,server.py}
  vault/setup.sh
  scripts/mint-jwt.sh
  scripts/enroll.sh
  scripts/run-demo.sh
  .gitignore
```

## Implementation Units

1. Add the thin Praxis AI + policy gateway and pin it to a PPE checkout carrying
   PR #168.
2. Add Vault and bridge containers with health checks, loopback-only published
   ports, and pinned images/dependencies.
3. Generate ephemeral RS256 keys/JWTs and configure Vault JWT auth, templated
   per-user policy, and short-lived login tokens.
4. Add PPE identity and Vault delegation config for `github_whoami`; put a
   deterministic deny before delegation on `github_denied`.
5. Add secure enrollment plus Alice/Bob/early-deny scenario assertions and a
   token leak scan.
6. Write the README, add the demo to the root index, and exercise clean bring-up
   and teardown twice.

## Validation

- `bash -n` and ShellCheck for every shell script.
- Parse YAML and Docker Compose configuration without starting services.
- Build the bridge image and gateway from a clean checkout.
- Run Alice success, Bob unenrolled denial, and early-policy-deny scenarios.
- Assert bridge and Vault request counts for every scenario.
- Search tracked files, generated state, logs, and HTTP responses for a planted
  canary token.
- Run `up.sh`, `down.sh`, then `up.sh` again to prove idempotent cleanup.

## Delivery

- Policy tracking: [#192](https://github.com/praxis-proxy/policy/issues/192).
- PPE blocker: [#168](https://github.com/praxis-proxy/policy/pull/168).
- Demo changes: a separate `praxis-proxy/demos` pull request referencing #192
  and declaring the temporary PR #168 pin.
