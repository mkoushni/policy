---
date: 2026-08-27
topic: apl-canonical-form
---

# APL canonical form: every allowed key and form

What APL accepts in this engine, after this work. This is the statement, not a
summary of one kept elsewhere: PPE defines its own language, and where a decision
below departs from how APL was first documented, the departure is named at the
point it happens rather than treated as a violation.

Every claim about this build's current behavior was measured against the parser,
not read from its comments.

Removed from the accept set by this work, and absent from every example here:
`apl:`, `policy:`, `post_policy:`, `identity:`, `global.policies:`.

`plugin(name)` is removed too, in favor of `run(name)` as the only form that
invokes a registered plugin.

Two further removals narrow the surface rather than rename it. The flat
`pre_invocation:` / `post_invocation:` spelling is gone, so `authorization:` is
the only place the two phase lists appear. And `args:` / `result:` are not
accepted under `global:`. `attribute_files:` moves out of the wrapper to
`global.attribute_files:`, joining `pdp:` and `session_store:` as a global-only
engine block.

---

## 1. The complete document

This document describes **policy mode**, `dispatch: policy`, which is the
default. `dispatch: hooks` is the other model: the top-level `plugins:`
declarations are the whole configuration, each plugin fires at the hooks it
declares, filtered by its own `conditions:` and ordered by its `priority`, and
`routes:` / `groups:` / `global:` are load errors. The two are mutually exclusive
and each rejects the other's keys. An unrecognized `dispatch:` value is a load
error naming both.

```yaml
# ─── Engine settings ────────────────────────────────────────────────
engine_settings:
  dispatch: policy               # the default. `hooks` is the pre-APL model
  plugin_timeout: 30             # seconds, per plugin
  short_circuit_on_deny: true    # default true
  route_cache_max_entries: 10000 # policy mode only

# ─── Plugin declarations ────────────────────────────────────────────
plugins:
  - name: corp-jwt
    kind: identity-jwt           # name + kind required; rest optional
    description: "Corporate JWT validation"
    author: platform
    version: "1.0.0"
    hooks: [identity.resolve]
    mode: sequential               # sequential|transform|audit|concurrent|
                                  #   fire_and_forget|disabled
    on_error: fail                 # fail|ignore|disable
    capabilities: [read_claims, read_meta]
    tags: [identity]
    config:
      issuer: https://idp.example.com
    # `priority:` and `conditions:` are hook-mode only. In policy mode a policy
    # step decides when a plugin runs, so declaring either is a load error.

# ─── Always-on policy ───────────────────────────────────────────────
global:
  authentication:                # list form, additive
    - corp-jwt
    - spiffe-attestor
  authorization:
    pre_invocation:
      - "http.method != 'GET': deny"
  response:
    status: 405
    headers:
      Allow: "GET"
  defaults:                      # keys: tool | resource | prompt | llm | http
    tool:
      description: "Default policy for every tool"
      authorization:
        pre_invocation:
          - "require(authenticated)"
  pdp:                           # each entry needs kind:, rest is factory-specific
    - kind: cedar-direct
      policy_dir: ./cedar
  session_store:
    kind: valkey
    url: redis://localhost:6379
  attribute_files:               # static data.* tree; global only
    - ./attributes/tenants.yaml

# ─── Named policy bundles a route joins by name or tag ──────────────
groups:
  hr-sensitive:
    description: "Controls for HR data"
    metadata:
      owner: hr-platform
    authentication:
      replace_inherited: true    # map form
      steps:
        - legacy-basic-auth
    authorization:
      pre_invocation:
        - "require(role.hr)"

# ─── Per-entity policy ──────────────────────────────────────────────
routes:
  - tool: get_employee           # exactly one selector per route
    meta:
      tags: [hr-sensitive]
      scope: employees
      properties:
        owner: hr-platform
    groups: [hr-sensitive]
    authentication: [corp-jwt]
    plugins:                     # MAP only: overrides, never activation
      pii_scanner:
        config: {sensitivity: high}
    args:
      employee_id: "str"
    authorization:
      pre_invocation:
        - "require(authenticated)"
        - "require(!delegated)"
        - "delegation.depth > 2: deny"
      post_invocation:
        - "run(audit-log)"
    result:
      ssn: "str | redact(!perm.view_ssn)"
      employee_id: "str | mask(4)"
    response:
      status: 403
      body: "{\"error\":\"forbidden\"}"
      headers:
        WWW-Authenticate: "Bearer"
```

### Selector forms

Exactly one selector per route.

```yaml
- tool: get_employee                       # exact
- tool: [get_employee, list_employees]     # list
- tool: "hr-*"                             # glob
- resource: "file:///data/*"
- prompt: summarize
- llm: gpt-4
- http: /healthz                           # exact path
- http: [/livez, /readyz]                  # list of exact paths
- http: {path_prefix: /v1/files, method: [GET, HEAD]}
- http: {path: /v1/files/manifest}         # keys: path | path_prefix | method
```

There is no `when:` on a route. It was parsed, never evaluated, and granted a
specificity bonus, so a narrowing condition acted as a broadening one. Removed;
express the condition as a `when:` / `do:` step inside `authorization:`, where it
is evaluated against the full attribute bag.

### The plugins: key has two shapes, and they belong to different models

```yaml
plugins:                         # MAP: per-plugin override. Adjusts a plugin a
  audit-log:                     #   run(name) step invokes. Never activates.
    config: {sink: stdout}
    capabilities: [read_meta]
    on_error: ignore
```

In policy mode the map is the only `plugins:` a route or bundle may carry, and it
overrides three values: `config`, `capabilities`, `on_error`. `hooks`, `kind`, and
`source` always come from the top-level declaration.

The **list** shape belongs to hook mode, which is the whole of the difference
between the two models:

| | hook mode, `dispatch: hooks` | policy mode, `dispatch: policy` |
|---|---|---|
| what activates a plugin | its own `hooks:` declaration | a `run(name)` step |
| where it runs | every hook it declared | where the step sits |
| ordering | plugin `priority`, bands | the order written, `sequential:` / `parallel:` |
| deny handling | `short_circuit_on_deny` | halt on deny, plus `on_deny:` reactions |
| gating | per-plugin `conditions:` | `when:` / `do:`, any predicate |
| `routes:` `groups:` `global:` | load error | the configuration |

In policy mode a `plugins:` **list** is valid only at top level, as the
declaration block. Activation lists are removed from every scope that carried
one: `routes[]`, `groups.<name>`, `global.defaults.<entity>`, and the reserved
`all` group. A plugin runs because a policy step names it with `run(name)`.

Activation lists were hook mode's mechanism arriving in policy mode's config.
They are how the engine worked before APL, and in policy mode they are worse than
redundant: the annotation short-circuit makes a list inert for the phases a block
declares and live for the phases it does not. Hook mode keeps them by keeping the
declaration block and each plugin's own `hooks:`, which is all it ever needed.

Under `global:` a list never worked at all: `GlobalConfig` models no such field
and the APL path copies `plugins` only when it is a mapping, so a list there was
dropped twice over and activated nothing.

### The authentication: key has two shapes

Both parse to the same IR. It binds plugins for the `identity.resolve` hook only,
independent of `plugins:`.

```yaml
authentication: [corp-jwt, spiffe-attestor]      # list form, always additive

authentication:                                   # map form, can replace
  replace_inherited: true                         #   drops inherited layers
  steps:
    - corp-jwt                                    # a step: bare plugin name
    - name: spiffe-attestor                       #   or a map
      config:
        verify_attestation: strict
```

A step is a bare plugin name, or a map with `name:` and an optional `config:`.
Layers stack `global` then tag bundles then the route, and each appends unless
`replace_inherited: true`. `replace_inherited: true` with `steps: []` is how a
route opts out of inherited identity entirely.

`replace_inherited:` works at bundle scope as well as on a route. A bundle that
sets it drops everything accumulated before it, the global layer and any earlier
bundle, and layers after it still append; a route that sets it drops every
inherited layer. Bundle order is `meta.tags` in declaration order then `groups:`
in declaration order, so which bundle replaces is reproducible from the file.

Today only the route layer honors the flag, and a bundle's is "parsed but not
honored" by the resolver's own account. Fixed as part of this work, which is the
one exception to §5b.

A step carries `name:` and `config:` and nothing else. Its `on_error:` was parsed
and dropped, so it is removed with the other inert keys, and the `flatten`
catch-all that let any unknown field into a step closes with the rest of the key
sets.

---

## 2. Phases

Four, in this order. The first `deny` in any phase stops that phase and every
later one.

| Key | When |
|---|---|
| `args:` | validate and transform request inputs, before the call |
| `authorization.pre_invocation:` | authorize, and carry obligations |
| `result:` | reshape the response |
| `authorization.post_invocation:` | checks once the result is known |

`authorization:` is the only place `pre_invocation:` and `post_invocation:`
appear, and it must declare at least one of them. The flat spelling on a section
is removed, in favor of one structure. APL as first documented called the two
forms equivalent; carrying both forward would keep two spellings for one thing,
which is what most of this cleanup exists to remove.

`args:` and `result:` are section-level and are never nested under
`authorization:`: they are phases, not authorization steps. They are valid on a
route and on a bundle, and not under `global:`.

---

## 3. Predicates

```text
truthiness    authenticated          role.hr           perm.view_ssn
comparison    delegation.depth > 2   client.trust_level == 'trusted'
              == != > >= < <=        attribute path on the left, literal on the right
membership    subject.id in authorized_users
              subject.id not in banned_list
existence     exists(delegation.origin_subject_id)
containment   security.labels contains "secret"
logical       & | !
grouping      ( )
interpolation data.tenants[subject.tenant].data_region

precedence    ()  >  !  >  &  >  |
```

Literals: `"double"` or `'single'` quoted strings, integers, decimal floats,
`true`, `false`.

---

## 4. Step entries

A `pre_invocation:` / `post_invocation:` entry, and a `do:` entry, is a string
or a single-key map.

### String forms

```yaml
- "require(P)"                        # deny unless P; P is any predicate.
                                     #   A top-level require(...) rule takes only
                                     #   `deny`: read as !P, `require(x): allow`
                                     #   would grant on the negation.
- "predicate: deny"                   # predicate: action
- "predicate: deny('reason')"
- "predicate: deny('reason', 'code')"
- "predicate: allow"
- "predicate"                         # bare predicate, default deny
- "deny"                              # bare action, unconditional
- "allow"
- "run(audit-log)"                 # the only invoke form
- "taint(PII)"                        # scope defaults to session
- "taint(PII, session)"               # scope: session | message
- "delegate(workday-oauth, subject: user, on_error: deny)"      # deny|continue
- "require_approval(ciba-elicitor, from: manager, purpose: 'release PII')"
                                     # NOT a step entry: a field operation is
                                     # valid only as an args:/result: map value
```

Elicitation verbs: `require_approval`, `confirm`, `require_step_up`,
`require_attestation`, `request_info`, `require_review`. First positional
argument is the handler plugin name; `from:` is required; `channel:`, `scope:`,
`purpose:` (alias `prompt:`), `timeout:`, `on_error:` are recognized, and any
other kwarg goes to the plugin.

`delegate(...)`: first positional is the plugin name, `on_error:` is reserved,
every other kwarg goes to the plugin. Values are scalars or `[a, b]` lists.

### Map forms

```yaml
- when: "role.hr & !perm.view_ssn"    # id: is reserved
  do:
    - "taint(restricted, session)"
    - "run(audit-log)"

- "role.hr": ["taint(restricted)", "deny('no')"]     # predicate: [effects]

- delegate:
    plugin: workday-oauth
    config: {audience: workday-api}
    on_error: deny

- restrict:
    allow_models: [gpt-4]
    deny_models: [gpt-3.5]
    allow_regions: [eu]
    allow_sites: [frankfurt]
    max_cost_tier: standard
    custom: {tier: gold}
    on_empty: deny                    # deny | fallback

- sequential: ["run(a)", "run(b)"]
- parallel: ["run(a)", "run(b)"]

- cedar:                              # PDP dialects: cedar opa authzen nemo cel
    action: read
    resource: employee
    on_deny: ["deny('cedar denied')"]
    on_allow: ["run(audit-log)"]

- opa("authz/allow"):                 # paren form: args are the call signature
    on_deny: ["deny"]
```

---

## 5. Field pipelines

`args:` and `result:` map a field to a `|`-separated chain, applied left to
right. A failing validator denies the phase.

| Category | Stages |
|---|---|
| Type validators | `str` `int` `bool` `float` `email` `url` `uuid` |
| Constraint validators | `enum(a, b, c)` `regex("...")` `len(1..100)` `0..100` `..100` `1..` |
| Transforms | `mask(N)` `redact` `redact(!predicate)` `omit` `hash` |
| Scans | `pii.redact` `pii.detect` `injection.scan` |
| Dispatch | `run(name)` `taint(label[, scope])` |

`validate(name)` is not implemented, and is rejected with a message naming
`regex(...)` and `run(...)` as the alternatives. It was never implemented in APL
either, so nothing is missing here that existed elsewhere.

`plugin(name)` is removed in both step and stage position; `run(name)` is the
only invoke form. `plugin` remains a kwarg name inside `delegate(...)`, the
elicitation verbs, and the `delegate:` map form, so the word is a noun
everywhere and never a verb.

---

## 5b. Keys removed for never having worked

Five keys were parsed by the config loader and honored by nothing. Three of them
warned "Setting ignored" at load (`engine.rs:405-431`); two were silent. All five
are removed, and each load error names its replacement.

| Removed key | Use instead |
|---|---|
| `plugin_dirs` | `register_factory()` plus the `plugins:` block |
| `engine_settings.parallel_execution_within_band` | per-plugin `mode: concurrent` |
| `engine_settings.fail_on_plugin_error` | per-plugin `on_error: fail` |
| an `authentication:` step's `on_error:` | the plugin declaration's own `on_error:` |
| a route's `when:` | a `when:` / `do:` step under `authorization:` |

A route's `when:` was the worst of the five: it was not merely ignored but
*scored*, adding a specificity bonus, so declaring a narrowing condition made the
route win more often.

One key in that class is kept and made to work instead:
`replace_inherited:` on a bundle. See the `authentication:` section.

## 6. What is being removed, and what each removal requires

| Key | Recognized today at | After |
|---|---|---|
| `apl:` | route, `global`, `defaults.*`, `groups.*` | not a key; APL terms sit on the section |
| `policy:` | route, `global`, groups, and inside `apl:` | not a key |
| `post_policy:` | same | not a key |
| `identity:` | `global`, `global.defaults.*`, `groups.*`, `routes[]` | not a key; `authentication:` is the only spelling |
| `global.policies:` | `global` | not a key; top-level `groups:` is the only spelling |

No key on the legacy list appears in the canonical document. Checked each
against section 1: `policy:`, `post_policy:`, `identity:`, `policies:`, and
`apl:` have no canonical role, and the canonical spellings that replace them
(`authorization.pre_invocation:`, `authorization.post_invocation:`,
`authentication:`, top-level `groups:`, and `authorization:` with its two phase
lists) are all in the document already. So the legacy tables are deleted wholesale rather than
filtered, and nothing has to be carried forward out of them.

**Each of these is currently a loud load error, and deleting the guard makes
three of them silent unless the key sets are closed first.**

`policy:`, `post_policy:` (`config.rs:986`, `parser.rs:2818`) and `identity:`
(`config.rs:936` `reject_renamed_identity_key`) exist only as rejection guards
that name the replacement. Deleting a guard does not make the key an error, it
makes it an *unknown* key, and unknown keys are handled in exactly one place:

- **routes** are covered. `reject_unknown_route_keys` (`config.rs:1056`) checks
  every route against `KNOWN_ROUTE_KEYS`, so `identity:` or `policy:` on a
  route still fails, just with a different message.
- **`global:` and `groups.<name>:` are not covered.** `GlobalConfig` and
  `PolicyGroup` have no `deny_unknown_fields` and no catch-all, and
  `config.rs:899` records that they silently ignore unknown fields. So deleting
  the guards turns `global.identity:`, `global.policy:`, and `global.policies:`
  into keys that load clean and do nothing. For `identity:` and `policies:`
  that means authentication steps and policy bundles vanish with no diagnostic,
  which is a fail-open and strictly worse than today.

So the cleanup has a prerequisite: close the key set at `global:`,
`global.defaults.<entity>:`, and `groups.<name>:` scope, the way
`reject_unknown_route_keys` already closes it for routes. The closed key set is
what replaces the rename guards as the diagnostic. With it in place, every
removed key fails as an unknown key at every scope it can be written, and the
guards are deleted rather than retained; without it, deleting them trades a
message that names the fix for no message at all.

`RouteYaml` in `ppe-apl-core` needs the same treatment: it has a
`#[serde(flatten)] other` catch-all, so a key it does not model is stashed
rather than rejected.

### Also removed as a consequence

`reject_legacy_apl_keys` (`visitor.rs:1149`), `renamed_apl_key_message`
(`config.rs:999`), `RENAMED_APL_KEYS`, `RENAMED_FIELDS`,
`ParseError::RenamedField`, and `reject_renamed_identity_key` all exist only to
report these five keys. They go with them.

### IR names follow

| Today | After | Note |
|---|---|---|
| `Phase::Policy` | `Phase::PreInvocation` | `rules.rs:448` |
| `Phase::PostPolicy` | `Phase::PostInvocation` | `rules.rs:452` |
| `CompiledRoute.policy` | `CompiledRoute.pre_invocation` | `rules.rs:529`, public and serialized |
| `CompiledRoute.post_policy` | `CompiledRoute.post_invocation` | `rules.rs:535`, public and serialized |
| `RouteEntry.identity` | `RouteEntry.authentication` | `config.rs:375`, drops the `rename` attribute |
| `GlobalConfig.identity` | `GlobalConfig.authentication` | `config.rs:173`, same |
| `PolicyGroup.identity` | `PolicyGroup.authentication` | `config.rs:203`, same |
| `GlobalConfig.policies` | renamed and rewired, not removed | it is the internal bundle store that top-level `groups:` folds into (`config.rs:928`) and that five production readers consult; only the YAML key goes |

`CompiledRoute` is `Serialize`, so renaming its fields changes the serialized
shape as well as the Rust API.

### `authorization:` requires an explicit phase

`AuthorizationYaml` (`parser.rs:2754`) marks both `pre_invocation` and
`post_invocation` `#[serde(default)]`, so `authorization: {}` loads as an empty
block. After this work, an `authorization:` block declaring neither phase is a
load error. It already carries `deny_unknown_fields`, so a misspelled phase
name is caught; what is missing is the "at least one" check.

---

## 6b. One structure, and the tables that stop implying otherwise

Measured across the two crates:

| Table | Size | Overlap |
|---|---|---|
| `KNOWN_ROUTE_KEYS` (`config.rs:1018`) | 19 | contains all 7 of `FLAT_APL_KEYS` |
| `FLAT_APL_KEYS` (`visitor.rs:1202`) | 7 | a strict subset of the above |
| `GLOBAL_ONLY_NON_DSL_KEYS` (`visitor.rs:1140`) | 3 | `pdp` and `session_store` are in all three tables |
| `HTTP_MATCH_KEYS` (`config.rs:674`) | 3 | none, a genuinely separate nested shape |

Three consequences, none of them decisions anyone took:

- `pdp:` and `session_store:` are listed as known *route* keys, so a route may
  write them, pass the key check, and get a warning. `attribute_files:` is not
  listed, so a route writing it errors. The three engine blocks disagree about
  their own scope.
- `attribute_files:` is in the global-only table and not in `FLAT_APL_KEYS`,
  which is exactly what makes it reachable only under `apl:`.
- The same seven keys are maintained in two places, in two crates.

Target: one table per scope, each key enumerated once, with the shared APL keys
named once and referenced rather than copied.

| Scope | Keys |
|---|---|
| route | `tool` `resource` `prompt` `llm` `http` / `meta` `groups` `plugins` `authentication` / `authorization` `args` `result` / `response` |
| `global` | `authentication` `defaults` / `authorization` / `response` / engine blocks |
| `groups.<name>` and `global.defaults.<entity>` | `description` `metadata` `plugins` `authentication` / `authorization` `args` `result` |
| engine blocks, `global` only | `pdp` `session_store` `attribute_files` |

`global:` takes `authorization:` and not `args:` / `result:`. That is settled: the
capability it removes is real, since `global.result: { ssn: "redact" }` is the only
way to cover every entity route once, and the CHANGELOG names it as a removed
capability rather than a tightened one. The scope tables are uniform as a result.

### Hook mode's key set

`dispatch: hooks` is a supported peer, not a deprecated path, so its surface is
stated here rather than left to a comparison table. It is where every config with
no engine-settings block sits today.

| Scope | Keys |
|---|---|
| top level | `engine_settings` `plugins` |
| `engine_settings` | `dispatch` `plugin_timeout` `short_circuit_on_deny` |
| a `plugins:` entry | `name` `kind` `hooks` `conditions` `priority` `mode` `on_error` `capabilities` `config` `tags` `description` `author` `version` |

`routes:`, `groups:`, and `global:` are load errors in hook mode, and
`route_cache_max_entries` has no meaning there. `conditions:` and `priority` are
hook-mode only, and are load errors in policy mode; the reverse holds for every
key in the policy-mode tables above.

Naming follows from the structure. With no `apl:` wrapper there is no flat form
to distinguish, so a constant cannot be called `FLAT_`. And scope belongs in
which table a key appears in, not in a name plus a runtime warning, so
`GLOBAL_ONLY_NON_DSL_KEYS` becomes the engine-block set that only the `global`
table references.

No duality survives. The flat phase spelling is removed, so `authorization:` is
the only wrapper, which is also what lets the key tables drop `pre_invocation`
and `post_invocation` entirely rather than carry them at two nesting levels.

## 7. Answers

**Point 1, `apl:` removal depth.** Three call sites, and they are not
independent.

`KNOWN_ROUTE_KEYS` (`config.rs:1031`) is the accept list. Dropping `"apl"`
there makes `apl:` on a route an unknown-key error. One line, no behavior
depends on it.

`apl_subblock` (`visitor.rs:1223`) is where it matters. Today an explicit
`apl:` wrapper wins *entirely*: when present, the flat keys on the same section
are not read at all. Removing the wrapper removes that precedence rule and
makes `FLAT_APL_KEYS` the only path, which is simpler in one direction and has
one hard requirement in the other: **`attribute_files:` is not in
`FLAT_APL_KEYS`.** That list is `pre_invocation`, `post_invocation`,
`authorization`, `args`, `result`, `pdp`, `session_store`, seven keys.
`attribute_files:` is only ever read as `global.apl.attribute_files`
(`visitor.rs:290`, `:526`), so deleting the wrapper without giving it a
section-level path makes the static `data.*` tree unloadable from configuration.
It becomes `global.attribute_files:` in the same change.

`response_yaml_block` (`visitor.rs:1304`) reads `response:` and falls back to
`apl.response:`, deliberately inverting `apl_subblock`'s precedence. With the
wrapper gone, the fallback and the long comment explaining why the two rules
disagree both go, which is the second thing removal buys.

Nothing else is reachable only through the wrapper. Audited: an `apl:` block
carries the three engine blocks, a `response:` block, and the `plugins:` override
map, and of those only `attribute_files:` has no section-level path today. The
`plugins:` map already reaches the section path through its map-shape test.
`ConfigVisitor::name()` returning `"apl"` (`visitor.rs:454`) is diagnostic
context in the engine's error messages (`engine.rs:946-1001`), not dispatch, so
no visitor contract changes; what does change is the trait's doc line
(`ppe-core/src/visitor.rs:69`) claiming a visitor's name matches its YAML key,
which stops being true. `strip_non_dsl_keys` exists to pull the engine blocks out
of an APL block before compiling it, and once those are `global:` siblings its
input is different, so it needs re-examining rather than carrying over. Fifteen
test files write `apl:` in inline YAML, three of them in the PDP builtins
(`cedar-direct`, `opa`, `cel`), and those are the migration.

On the error question: a load error naming the key. A silent drop of `apl:`
takes the whole policy body with it, which is the fail-open shape this repo
already refuses for a dropped `policy:` block. Since `apl:` at route scope is
covered by `KNOWN_ROUTE_KEYS` and at `global:` / `groups:` scope is not, this is
the same prerequisite as section 6: close those key sets, then `apl:` fails
loudly everywhere it can be written.

**Point 4, `compile_config`'s map-keyed `routes:`.** `ppe-apl-core` exposes
`compile_config(yaml)` reading `ConfigYaml { routes: HashMap<String, RouteYaml> }`
(`parser.rs:2686`). That `routes:` is a **map keyed by an opaque route key with
no selector**:

```yaml
routes:
  t1:                     # not a selector, just a name
    pre_invocation: ["require(authenticated)"]
```

Canonical `routes:` is a list of selector-keyed entries, which is what
`ppe-core`'s `PolicyConfig.routes: Vec<RouteEntry>` is. So the two crates
disagree about what `routes:` means, and only one of them matches the canonical
form.

Every caller of `compile_config` in the tree is a test
(`ppe-apl-runtime/tests/*`). Production compiles one route's APL block at a
time through `compile_policy_block_value`, which the visitor calls
(`visitor.rs:545,646,672,759`). Nothing in a real load path reads the map shape.

The two also disagree on the accept set: `RouteYaml` does not model `apl:`, so
`compile_config` drops an `apl:` block into `other` and ignores it, while the
visitor path honors it. The same document compiles to different policy
depending on which entry point reads it.

**No, there is no reason to keep it.** It is public on `ppe-apl-core` and not
re-exported by the facade, so its only external surface is that one crate at
0.1.x. All six calling files are tests. What it does beyond
`compile_policy_block_value` is iterate a map of route keys and build a
`PluginRegistry` from a root `plugins:` block, and production does neither:
plugins reach the registry through `ppe-core`'s typed `PluginConfig` and
`visit_plugins`. `compile_route`, its per-route wrapper, is a legacy-key guard
plus a has-APL gate plus `compile_apl_blocks`, and the guard is being deleted, so
it collapses into `compile_apl_blocks` on its own.

So `compile_config`, `ConfigYaml`, `CompiledConfig`, and `compile_route` go.
`RouteYaml` and `compile_apl_blocks` stay, reached through
`compile_policy_block_value`. What the tests want is a compiled route plus a
plugin registry, which is a test helper rather than a public compiler, and no
production coverage is lost because the shared path is the one production
already takes.

**Point 5, `global.policies:`.** Dropped. Top-level `groups:` is the only
spelling. Same prerequisite as the others: the `global:` key set has to close,
or the removal is silent and every bundle written there disappears.

**The absent `a2a` selector was my error.** APL's first definition lists "tool,
resource, prompt, or A2A method" as the operations a route governs, and I read
that as a missing selector. A2A traffic is normalized into the CMF entity types
before routing, and those are `tool`, `llm`, `prompt`, `resource`, and `http`
(`cmf/constants.rs:102-114`), so an A2A method is selected by `tool:` or
`prompt:` like any other. There is no `a2a:` selector to be missing, and no gap
to record.
