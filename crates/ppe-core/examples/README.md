# PPE Core Examples

Both examples run under `engine_settings.dispatch: hooks`. Neither registers an APL config visitor, so neither has a `run(name)` policy step to invoke a plugin from, and hook dispatch is what activates plugins here. `routes:`, `groups:`, and `global:` are load errors in that mode; an example demonstrating policy dispatch would register a visitor and write a policy instead.

## plugin_demo

A complete end-to-end example showing how to build plugins, load config, and invoke hooks with the PPE runtime.

### What it demonstrates

- **Defining hook types and payloads** — `ToolPreInvoke` and `ToolPostInvoke` hooks with a shared `ToolInvokePayload`
- **Building plugins** — four plugins (`IdentityResolver`, `PiiGuard`, `RemoteAuthz`, `AuditLogger`) implementing `Plugin` + `HookHandler<H>` for different hook types
- **Multi-hook registration** — a single plugin instance (e.g., `IdentityResolver`) registered for multiple hooks (`demo.tool_pre_invoke` and `demo.tool_post_invoke`) via the factory pattern
- **Host-owned hooks** — the demo declares its own hook names with `define_hooks!` and registers their metadata before loading the config that names them, which is the ordering the loader requires
- **Plugin factories** — `PluginFactory` implementations that create plugin instances and wire up typed handler adapters
- **YAML config loading** — `plugin_demo.yaml` declares four plugins and nothing else; hook dispatch needs no routes
- **Per-plugin conditions** — `pii-guard` and `remote-authz` each carry a `conditions: [{tools: [...]}]` that narrows them to one tool, while `identity-resolver` and `audit-logger` declare none and fire on every request
- **Priority ordering** — `priority:` orders the chain in both dispatch modes, so the four plugins run 10, 20, 30, 100
- **PluginContext** — `global_state` used to pass PII clearance between hooks, `local_state` for per-plugin scratch data
- **BackgroundTasks** — fire-and-forget plugins (`AuditLogger`) spawn background tasks; `wait_for_background_tasks()` awaits them
- **PluginContextTable** — context table threaded from pre-invoke to post-invoke to preserve plugin state

### Running

From the workspace root:

```
cargo run --example plugin_demo
```

### Scenarios

The demo runs seven scenarios against four registered plugins:

| Scenario | Tool | User | Outcome |
|----------|------|------|---------|
| 1 | get_compensation | alice (no clearance) | DENIED by pii-guard |
| 2 | get_compensation | alice (with clearance) | ALLOWED, then post-invoke fires |
| 3 | list_departments | bob | ALLOWED (pii-guard's `conditions:` exclude the tool) |
| 4 | some_other_tool | charlie | ALLOWED (only the unconditional plugins fire) |
| 5 | query_external_data | alice (in ACL) | ALLOWED by remote-authz, cache hit |
| 6 | query_external_data | charlie (not in ACL) | DENIED by remote-authz, cache miss |
| 7 | list_departments | (empty) | DENIED by identity-resolver |

### What hook dispatch does not express

The scenario outcomes above are what they were under the earlier policy-dispatch config, but the mechanism is not the same one and does not generalize. Hook mode has no route table and no tags: `conditions:` matches an entity name, so a plugin scoped to a *set* of tools by a shared tag now names each tool, and a tag the host injects on the request at runtime activates nothing. Tag- and route-scoped activation is a policy-dispatch feature, and reaching it means registering an APL visitor and writing `run(name)` steps.

### Files

- `plugin_demo.rs` — Rust source with plugins, factories, and main
- `plugin_demo.yaml` — YAML config with four plugin declarations and their conditions

---

## cmf_capabilities_demo

Demonstrates CMF messages with capability-gated extension access. Shows how different plugins see different views of the same extensions based on their declared capabilities.

### What it demonstrates

- **CMF Message** — typed content parts (`Text`, `ToolCall`) with the standard CMF format
- **Capability gating** — plugins declare capabilities in YAML config; the executor filters extensions per plugin
- **Security labels** — `MonotonicSet` (add-only, no remove at compile time)
- **Guarded HTTP headers** — `.read()` is free, `.write(token)` requires a `WriteToken`
- **COW copy** — `extensions.cow_copy()` for plugins that need to modify; zero-cost for read-only plugins
- **Write tokens** — executor sets tokens based on capabilities; propagated through `cow_copy()`
- **Three capability levels** — identity-checker (security), header-injector (http + labels), audit-logger (http + labels read-only)
- **Unconditional hook dispatch** — none of the three declares `conditions:`, so all three fire on every request; the config is a `plugins:` block and an `engine_settings:` block, with no route table at all

### Running

From the workspace root:

```
cargo run --example cmf_capabilities_demo
```

### What each plugin sees

| Plugin | Capabilities | Security Labels | Subject | HTTP Headers | Can Write |
|--------|-------------|-----------------|---------|--------------|-----------|
| identity-checker | read_labels, read_subject, read_roles | visible | visible (id + roles) | hidden | no |
| header-injector | read_headers, write_headers, append_labels | visible | hidden | visible | yes (headers + labels) |
| audit-logger | read_headers, read_labels | visible | hidden | visible | no (audit mode) |

### Files

- `cmf_capabilities_demo.rs` — Rust source with CMF plugins and capability-gated access
- `cmf_capabilities_demo.yaml` — YAML config with per-plugin capabilities
