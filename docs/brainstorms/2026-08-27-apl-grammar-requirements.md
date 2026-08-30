---
date: 2026-08-27
topic: apl-grammar
---

# APL has a written grammar, and the parser agrees with it

## Summary

APL's grammar is written down as EBNF in one document covering both halves of
the language: the DSL embedded in strings (predicates, rules, actions, call
forms, pipeline stages) and the YAML shape those strings sit in (`routes:`,
`authorization:`, step maps, `when:`/`do:`, `restrict:`). The document states
one rule for quoting and escaping, one operator precedence table, and one
identifier production, and the parser is reconciled against it.

Reconciliation runs in both directions. Where the parser accepts text no
grammar would write, the grammar wins and the parser tightens. Where the parser
carries a special case the grammar cannot justify, the special case is removed
by making the grammar general enough not to need it. `require(...)` is the
clearest instance: read as a predicate meaning `!P`, it stops being a
rule-level special case, becomes nestable, gains comparison support, and every
policy line in use today keeps its current meaning.

The same reconciliation runs against the shape APL was first defined with, which
this engine has drifted from in both directions. Five keys are accepted that never
had a place in it (`apl:`, `policy:`, `post_policy:`, `identity:`,
`global.policies:`), and forms that plainly belong are rejected,
`require(!delegated)` among them. The five are removed at every scope, their
rename guards are deleted rather than retained, and the IR stops speaking a
vocabulary no config may use: `Phase::Policy` and `CompiledRoute.policy` take the
invocation-phase names their config keys have carried for a while.

A separate five keys are removed for a different reason: the config parses them
and nothing honors them. A route's `when:`, `plugin_dirs`,
`parallel_execution_within_band`, `fail_on_plugin_error`, and an `authentication:`
step's `on_error:`. Three announced it at load with "Setting ignored" and two were
silent, and `when:` was worse than silent, adding a specificity bonus so a
narrowing condition made its route win more often. Each removal names the working
mechanism that replaces it. One key in that class is kept and made to work
instead, `replace_inherited:` on a bundle, because nothing else lets a bundle
supersede inherited identity rather than add to it.

What a document may contain now depends on one declared key.
`engine_settings.dispatch` selects between policy mode, where `routes:`,
`groups:`, and `global:` drive dispatch through APL, and hook mode, where the
top-level `plugins:` declarations are the whole configuration and each plugin
fires at the hooks it declares. The two are mutually exclusive and each rejects
the other's keys, which is what retires the activation lists on routes and bundles
and what lets `dispatch` default to policy without silently disabling the plugins
in a config that declares no routes.

Exhaustiveness is a mechanism, not a claim. A checked-in accept/reject
conformance corpus carries at least one case per production and per documented
wart. Its completeness rests on the audit behind this document rather than on a
search: it makes every divergence found so far permanent, and no mechanism here
proves the absence of one nobody has found. The corpus is also the negative-test
material the coverage work needs.

The work is therefore grammar and config model together. The accept set the
grammar document describes cannot be stated without deciding which keys a
document may carry, and that decision is what the two dispatch modes, the closed
key sets, and the legacy-key removal settle. Splitting them would leave two
issues editing `config.rs` and each blocked on the other's half.

Addresses [praxis-proxy/policy#17](https://github.com/praxis-proxy/policy/issues/17).
Pairs with [#14](https://github.com/praxis-proxy/policy/issues/14).

---

## Problem Frame

`crates/ppe-apl-core/src/parser.rs` is 5,588 lines and the only statement of
what APL accepts is comments inside it. The parser is the spec, so a rough edge
is visible only by reading the code that implements it, and an operator writing
policy has nothing to read at all.

The one written summary is wrong. `parser.rs:9-24` lists what the parser
rejects:

| The header claims | Actually |
|---|---|
| `✗ in / not in / exists()` needs IR variants first; rejected | all three parse and evaluate |
| `✗ Steps (cedar:(), opa(), plugin(), taint())` rejected with clear errors | all four parse |
| `✗ Multi-effect do: lists, sequential:/parallel: blocks` rejected | all three parse |
| `Tok::And` must have surrounding spaces, caller enforces | nothing enforces it, `a&b` parses |

Four claims, four wrong. A reader who trusts the file's own account of the
grammar is misled about a third of the language.

**Quoting is handled at six sites with four behaviors.** The predicate lexer
scans to the next matching quote with no escape handling at all.
`split_top_level_commas` treats `\` as a splitting protection but never
unescapes. `split_predicate_action` and `split_top_level` track quotes but not
`\`. `unwrap_quotes` strips one matching pair. PDP paren arguments strip
nothing, so `opa("p/q")` reaches a resolver as the four characters `"p/q"`
including the quotes. The consequence, measured:

| Input | Result |
|---|---|
| `a == 'it\'s'` | `unterminated string literal` |
| `a == "say \"hi\""` | `unexpected char '\'` |
| `deny('it\'s bad')` | reason is `it\'s bad`, backslash retained |
| `regex(")` | a regex matching one literal `"` |
| `opa("p/q")` | args are `"p/q"` with quotes |

The five hand-rolled quote strippers were consolidated after two of them
crashed the parser on a lone quote, which removed the crash without settling
what a quote means. `regex(")` no longer aborts; it now silently compiles a
pattern the author did not write.

**The grammar's own escape hatches are indistinguishable from typos.** A step
map key that matches no known step keyword becomes `PdpDialect::Custom(key)`
and loads clean, so `whens: { a: 1 }` compiles into a call to a decision point
named `whens`. That open set is deliberate: `cedar-direct` and the OPA resolver
both support registering under a custom dialect, so the parser cannot know the
valid set at load time. The cost is that a misspelled step key is accepted as a
policy step that will never resolve.

**Where the informal grammar lives, and what it does not cover:**

| Where | What it documents | Gap |
|---|---|---|
| `parser.rs:9-24` | the accepted/rejected surface | wrong on four counts |
| `parser.rs:598-608` `parse_rule` | four rule forms | precedence, quoting, identifier shape all absent |
| `parser.rs:1042-1050` `parse_delegate_call_args` | delegate kwargs, "informal" EBNF | the only EBNF in the tree, scoped to one call form |
| `parser.rs:880-891` `try_parse_deny_call` | deny arity | describes single quotes as the spec form, accepts both |
| `parser.rs:104-117` `Tok` | operator spellings | no precedence, one stale comment |
| nothing | pipeline stage grammar | 20 stage names, arity and argument shape undocumented |
| nothing | which YAML keys APL consumes | `RouteYaml.other` swallows the rest |

**Position changes meaning, and nothing says so.** `|` is predicate OR in a
rule, a stage separator in a pipeline, and an operand separator inside
`require(...)`. `result.x | redact` in rule position does not fail: it parses as
`Or([IsTrue result.x, IsTrue redact])` with a default deny, so a field
operation written one line too high becomes a truthiness test on an attribute
named `redact`. `validate` exists in the stage IR and the evaluator and no
parser path produces it: `parse_stage` rejects it unconditionally. `deny` and
`allow` are the only actions in rule position;
effect position also takes plugin, taint, delegate, and field operations.

**The parser accepts text no grammar would write.** Measured, all accepted
today: `a..b`, `a.`, `data.t[]`, `1.` as a float, `007`, `mask(4) |` with an
empty trailing stage, and `not` as an ordinary attribute name alongside its
role in the `not in` phrase. Bracket interpolation contents are unvalidated and
quote-blind, so `data.t[a:b]` parses as an attribute key while
`data.t["a]"]` fails as an unterminated string.

**Two rejections report the wrong thing.** `require(a): deny` fails in the
lexer with `at byte 10: unexpected char ':'`, because `require` is intercepted
before the colon split. `data.t[a:b] == "y"` as a bare predicate fails with
`unsupported action 'b] == "y"'`, because the predicate/action splitter tracks
parens and quotes but not brackets. A non-ASCII identifier reports
`at byte 3: unexpected char 'Ã'`, a byte offset and a mojibake character.

**The accept set has drifted from the shape APL was defined with, in both
directions.** APL's original definition (`contextforge-org/cpex`,
`docs/content/docs/apl/_index.md`, cited here for provenance rather than as an
authority over this engine) has no `apl:` wrapper and no `policy:` key; its phases
are `args`, `authorization.pre_invocation`, `result`, and
`authorization.post_invocation`. This build accepts five keys that shape does not
have, and rejects a rule form it shows working:

| Key | Recognized at | How |
|---|---|---|
| `apl:` | route, `global`, `defaults.*`, `groups.*` | live wrapper, and it wins *entirely* over the flat keys on the same section |
| `policy:` / `post_policy:` | the same four scopes | rejection guards naming the replacement, in two tables (`config.rs:986`, `parser.rs:2818`) |
| `identity:` | `global`, `defaults.*`, `groups.*`, `routes[]` | a third rejection guard (`config.rs:936`) |
| `global.policies:` | `global` | deprecated alias for top-level `groups:`, merged at parse |

| Canonical form | Result here |
|---|---|
| `require(!delegated)` | `expected identifier inside require(...)` |
| `require(delegation.depth < 3)` | `expected ',', '\|', or ')' in require(...), got Lt` |

So `require(...)` accepting only bare identifiers is not merely an internal
wart. A negation inside `require(...)` is the obvious spelling, it appears in APL's
own first documentation, and this parser cannot read it, which is evidence the
restriction was accidental rather than chosen.

**The rejection guards are the only diagnostic, and unknown keys are closed in
exactly one place.** `reject_unknown_route_keys` (`config.rs:1056`) closes the
key set for routes. `GlobalConfig` and `PolicyGroup` have no
`deny_unknown_fields` and no catch-all, and `config.rs:899` records that they
ignore unknown fields silently. `ppe-apl-core`'s `RouteYaml` has a
`#[serde(flatten)] other` that stashes anything it does not model. So deleting a
rename guard does not make its key an error at `global:` or `groups:` scope, it
makes the key inert: authentication steps and policy bundles written there would
vanish with no message.

**The removed names are still the language's internal vocabulary.** `policy:`
has not been a valid config key for a while, and `Phase::Policy`
(`rules.rs:448`), `Phase::PostPolicy` (`:452`), and the public, serialized
`CompiledRoute.policy` / `.post_policy` (`:529`, `:535`) all still carry it, as
does rustdoc that explains behavior in terms of a `policy:` key
(`visitor.rs:1258-1288`). Symmetrically, three struct fields are named
`identity` whose YAML key is already `authentication:` (`config.rs:173`, `:203`,
`:375`). An operator writes one word and every diagnostic downstream says
another.

**There are two `routes:` keys.** `ppe-core`'s is `Vec<RouteEntry>`, a list of
selector-keyed entries, which is the shape APL was defined with. `ppe-apl-core`'s
`ConfigYaml.routes` is `HashMap<String, RouteYaml>`, keyed by an opaque name
with no selector, reachable through the public `compile_config`. Every caller of
it in the tree is a test; production compiles per-route blocks through
`compile_policy_block_value`. The two also disagree about `apl:`, which the
visitor honors and `compile_config` silently ignores, so one document compiles
to different policy depending on the entry point.

---

## Actors

- A1. Policy author: writes rule strings and pipe chains. Has no document to
  read and learns the grammar from error messages, examples, and the parser's
  behavior on the text at hand.
- A2. Deployment operator: writes the YAML the strings sit in. Needs to know
  which keys are consumed and which are silently ignored.
- A3. PPE maintainer: adds a stage, an effect, or a step form. Needs one place
  where the language is stated so a new form does not collide with an existing
  one by accident.
- A4. Reviewer of a policy change: needs to read a diff and know what it
  accepts without running the parser.
- A5. Author of the coverage work (#14): needs the parser's roughly 90 error
  sites enumerable against something other than the source.
- A6. Downstream config author outside this repo, including the praxis-demos
  policies: bears the migration cost of every breaking change taken here.

---

## Requirements

**The document**

- R1. A grammar document exists under `docs/` and is the language's normative
  statement. Where it and the parser disagree, the disagreement is a defect in
  one of them, not a difference of opinion.
- R2. It contains the full EBNF for the string DSL: predicates, rule forms,
  action forms, the call forms (`require`, `deny`, `plugin`, `run`, `taint`,
  `delegate`, the six elicitation verbs), and pipeline stages with each stage's
  argument shape.
- R3. It contains the YAML shape as a structural section: the `routes:` map,
  the flat and nested `authorization:` forms, step entries as string or
  single-key map, `when:`/`do:`, `restrict:`, PDP call maps, and `args:` /
  `result:` field maps. Where the shape is enforced by serde rather than by a
  production, it says so.
- R4. It states one lexical rule set: token spellings, whitespace
  significance, identifier and attribute-path shape, numeric literal forms,
  and the quoting and escaping rule.
- R5. It contains one operator precedence and associativity table covering
  `!`, `&`, `|`, the comma inside `require(...)`, the comparison operators,
  `contains`, `in`, `not in`, and parenthesized grouping, including how `!` binds
  relative to a comparison. The comma binds lower than both `&` and `|`, so
  `require(a, b | c)` is `!(a & (b | c))` and not `!((a & b) | c)`; the two readings
  produce opposite decisions, so the table states it rather than leaving it to
  prose.
- R6. It names every remaining wart as a wart, with the reason it survived.
  A rough edge that is documented and tested is a known property; one that is
  only implemented is a trap.
- R7. It records, per form, what position accepts it: rule position, effect
  position, pipeline position, or map-key position. A form that means different
  things in two positions says both.
- R8. `parser.rs`'s header comment block stops describing the grammar and
  points at the document instead. A second, drifting account is worse than none.

**Lexical rules**

- R9. One quoting rule applies at every site that reads a quoted string. A
  string literal is delimited by matching `'` or `"`, and the same escape
  sequences are recognized and applied wherever a string literal appears:
  predicate literals, `deny(...)` arguments, `delegate(...)` values, and PDP
  call arguments.
- R10. An escape sequence is unescaped, not merely skipped. A backslash that
  protects a delimiter during splitting does not survive into the parsed value.
- R11. The set of recognized escape sequences is finite and listed. An
  unrecognized escape is an error, not a literal backslash.
- R12. An unterminated string literal is an error at every site. A lone quote
  is never content: `regex(")` and `enum(")` are rejected rather than yielding
  a one-character argument.
- R13. An attribute path is a production, not a byte scan. A trailing `.`, a
  doubled `.`, a leading `.`, and an empty interpolation group are all
  rejected. An interpolation group's contents parse as an attribute path, which
  makes `data.t[a:b]` and `data.t["a]"]` errors by construction rather than by
  accident.
- R14. Identifiers are ASCII, and the document says so. A non-ASCII identifier
  is rejected with a message naming the character.
- R15. Numeric literal forms are stated exactly: integer and decimal float,
  with or without a leading sign, and no exponent form. Text that looks numeric
  but matches no form is rejected with a message naming the form it is missing,
  not with a trailing-token error.
- R16. Whitespace between tokens is insignificant, and the document says so.
  `a&b`, `a & b`, and `a  &  b` are one expression.
- R17. `&&` and `||` are rejected with a message naming `&` and `|`. They are
  a predictable import from other languages and deserve better than
  `expected atom`.

**Predicates**

- R18. `not` is a reserved word. It is not usable as an attribute name or as
  the first segment of an attribute path, which removes the case where one word
  is both a grammar phrase and an identifier.
- R19. `not` outside `not in` is rejected with a message naming `!`. One
  negation operator, one spelling.
- R20. The comparison operators take an attribute path on the left and a
  literal on the right, in that order, and the document states the asymmetry.
  A reversed comparison and an identifier on the right are both rejected with a
  message that names what is accepted.
- R21. `exists(...)` takes an attribute path, not a string literal, and the
  document says so.
- R22. Parenthesized grouping, `!`, `&`, and `|` compose freely over any
  predicate. There is no predicate form that is legal at the top of an
  expression and illegal inside a group.

**`require`**

- R23. `require(P)` is a predicate over any predicate `P`, and its meaning is
  `!P`. The rule-level desugaring follows from that plus the existing
  bare-predicate default: `require(P)` as a rule is the predicate `!P` with the
  default deny action, which is exactly what it produces today.
- R24. Every `require(...)` form in use keeps its current meaning.
  `require(a)` denies when `a` is falsy. `require(a | b)` denies when both are
  falsy, which is `!(a | b)`. `require(a, b)` denies when either is falsy,
  which is `!(a & b)`, so the comma is retained as a low-precedence `&`
  separator rather than removed.
- R24b. The desugaring states its normalization, because De Morgan gives
  equivalent meaning and not an identical tree. `Not(Condition(IsTrue{k}))` folds
  to `Condition(IsFalse{k})`, and `Not(And(..))` / `Not(Or(..))` distribute, so
  `require(a)`, `require(a, b)`, and `require(a | b)` compile to the IR they
  compile to today. Without the fold they would build `Not` over `IsTrue`, three
  existing tests that assert the current shapes by name would break, and R45's
  promise that `parse_rule` keeps its results would hold only in spirit.
- R25. `require(...)` accepts any predicate, including comparisons, so
  `require(delegation.depth < 3)` is valid.
- R26. `require(...)` nests inside `&`, `|`, `!`, and parentheses, and mixing
  `,` with `|` inside one call is no longer an error, because both are ordinary
  predicate operators under a stated precedence.
- R26b. A rule whose predicate is a top-level `require(...)` accepts only the
  `deny` action. Any other action, `allow` included, is a load error naming the
  inversion: read as `!P`, `require(authenticated): allow` grants exactly when the
  subject is not authenticated, and today that text cannot exist because the colon
  dies in the lexer. The restriction is on the action, so `require(...)` stays
  composable inside `&`, `|`, `!`, and parentheses where its reading is
  unambiguous.
- R27. `require(...)`'s special case in `parse_rule` is removed rather than
  relocated. `require(a): deny` parses as a predicate and an action, and
  `require (a)` with a space parses, both as consequences of `require` being a
  keyword token rather than a string prefix.

**Rule and effect position**

- R28. A field operation is not a rule. Text whose left side is a path rooted
  at `args.` or `result.` followed by a top-level `|` is a field operation
  wherever it appears, and in rule position it is an error naming the position
  it belongs in. It no longer parses as a disjunction of truthiness tests.
- R29. The predicate/action split is bracket-aware, so an interpolation group
  containing a `:` does not split a rule. This is a defect fix, independent of
  which direction the grammar moves.
- R30. Rule-position actions and effect-position effects are two productions,
  both written down, with the difference between them stated. Actions are
  case-sensitive and the document says so.
- R31. An empty pipeline stage is rejected. A leading, trailing, or doubled `|`
  in a pipe chain is an error rather than a silently dropped segment.
- R32. The pipeline stage table lists every stage name, its arity, its argument
  form, and the positions it is valid in. `validate(name)` is rejected in every
  position, field rule included, with a message naming `regex("...")` and
  `run(name)`: `parse_stage` refuses it unconditionally, and `parse_pipeline` is
  the only stage parser a field rule goes through.

**Map forms**

- R33. A step map's key set is closed against typos. A key that is neither a
  recognized step keyword nor a recognized PDP dialect is reported at load
  rather than compiled into a call to a custom decision point.
- R34. A custom PDP dialect stays reachable through an explicit spelling, so
  the feature survives the closed key set. A resolver registered under a custom
  dialect is a supported deployment and is not collateral damage of R33.
- R35. The shorthand-detection exclusion list (`delegate`, `sequential`,
  `parallel`, and the five known dialects) is replaced by the closed key set.
  Its current form is exactly the special-case handling the grammar does not
  justify: the sets are disjoint by construction once the keys are enumerated.
- R36. The document states which keys APL consumes on a route and which it
  stashes for other consumers, so a key that nothing consumes is identifiable.
  Where the authority for a route-level key belongs to `ppe-core` rather than
  to APL, the document says which side owns it.
- R37. `id:` on a `when:`/`do:` map is documented as reserved rather than
  discovered as tolerated.
- R38. What a PDP resolver receives from each call form is stated. The paren
  form's argument reaches a resolver as a scalar and the map body reaches it as
  a mapping, and the document says which form the shipped resolvers read.

**Canonical conformance**

- R49. The grammar document is the definition of APL for this engine, not a
  summary of one held elsewhere. It carries no dependency on an external document
  and does not defer to one, so a reader needs nothing but it to know what a
  policy may say.
- R50. Where a decision departs from how APL was first defined, the document says
  so at that decision and gives the reason. Provenance is cited at the point of
  use rather than as a blanket reference, because three decisions here deliberately
  differ: the flat phase spelling is removed, `run(name)` replaces `plugin(name)`,
  and `args:` / `result:` are refused under `global:`.
- R50b. The reverse direction is a defect and reads as one. A form APL was
  defined with that this parser rejects is evidence of an accidental restriction,
  and `require(!delegated)` is the instance. R23 fixes it on its own merits, and
  the provenance is corroboration rather than the justification.

**Legacy keys**

- R51. `apl:`, `policy:`, `post_policy:`, `identity:`, and `global.policies:`
  leave the accept set at every scope they are recognized today: a route,
  `global:`, `global.defaults.<entity>:`, and `groups.<name>:`.
- R52. Nothing is carried forward out of the legacy tables. No key they name has
  a place in the language, so `RENAMED_APL_KEYS`, `RENAMED_FIELDS`,
  `ParseError::RenamedField`, `renamed_apl_key_message`,
  `reject_legacy_apl_keys`, and `reject_renamed_identity_key` are deleted as
  guards. Their name-to-replacement content is not: R53 keeps it as the hint the
  unknown-key error carries.
- R53. Every removed key fails at load naming both the key and its replacement
  spelling, at every scope it can be written. None is silently ignored. A closed
  key set restores loudness but not guidance, so the name-to-replacement mapping is
  carried into the unknown-key diagnostic rather than lost with the guards: without
  it the five keys that have a replacement would get a plainer error than the
  never-worked keys R73 covers.
- R54. R53 requires the key set to close at `global:`,
  `global.defaults.<entity>:`, and `groups.<name>:`. That closure is part of
  this work rather than a follow-on: today those scopes ignore an unknown key,
  so deleting a rename guard without closing them converts a load error into a
  fail-open. The closed key set is what replaces the guard as the diagnostic.
- R54b. `PolicyConfig`'s own top-level key set closes too: `engine_settings`,
  `global`, `groups`, `routes`, `plugins`. It has no `deny_unknown_fields` and no
  catch-all today, so a stale or misspelled top-level key is dropped by serde. That
  is the level where this work's own rename bites: a config keeping
  `plugin_settings:` loses every engine setting silently, `dispatch:` included, and
  therefore takes the default mode rather than the one it declared.
- R55. `RouteYaml` closes its own key set. Its `other` catch-all is the same
  silent-drop shape one crate over.
- R56. `attribute_files:` becomes `global.attribute_files:`, a sibling of
  `authorization:` rather than a key inside a wrapper. It is read only as
  `global.apl.attribute_files` today, so removing the wrapper without moving it
  would make the whole `data.*` namespace unloadable from configuration and take
  every predicate that reads `data.*` dark with no load error. It joins
  `pdp:` and `session_store:` as a global-only engine block.
- R56b. Nothing else is reachable only through the wrapper. Audited: the three
  engine blocks, the `response:` fallback, and the `plugins:` override map are
  the whole of what an `apl:` block carries, and only `attribute_files:` has no
  section-level path today. `ConfigVisitor::name()` returning `"apl"` is
  diagnostic context in the engine's error messages, not dispatch, so no visitor
  contract changes; the trait's doc line stating that a visitor's name matches
  its YAML key stops being true and is rewritten.
- R56c. `strip_non_dsl_keys` is re-examined rather than carried over. It exists
  to remove the engine blocks from an APL block before compiling it; once those
  blocks are `global:` siblings, its input is different and it may have no job
  left.
- R57. Both precedence rules the wrapper creates go with it: the wrapper-wins
  rule in `apl_subblock`, and the deliberately inverted route-sibling-wins rule
  `response_yaml_block` applies to `response:`.
- R58. One `routes:` shape survives in the project, and it is `ppe-core`'s list
  of selector-keyed entries. `compile_config`, `ConfigYaml`, `CompiledConfig`,
  and `compile_route` are deleted: their only callers are tests, no production
  path reads the map shape, and the map's `routes:` is the second definition of
  the key. `RouteYaml` and `compile_apl_blocks` stay, reached through
  `compile_policy_block_value`, which is the entry point production already uses.
- R58b. The tests that used `compile_config` get what they actually needed from
  it, which is one compiled route plus a plugin registry. Nothing that only
  `compile_config` exercised loses coverage, because `compile_route` collapses
  into `compile_apl_blocks` once the legacy-key guard it wraps is deleted.

**Two modes, selected by one key**

- R82. One key names the configuration model, and it says which model it names.
  `plugin_settings:` becomes `engine_settings:`, and the boolean
  `routing_enabled: bool` becomes `dispatch: policy | hooks`. `policy` makes
  `routes:`, `groups:`, `global:`, and `global.defaults:` available with APL
  deciding what runs. `hooks` makes the top-level `plugins:` declarations the
  whole configuration, each plugin firing at the hooks it declares, filtered by
  its own `conditions:` and ordered by its `priority`.
- R82b. The rename reaches the Rust surface: `PluginSettings` becomes
  `EngineSettings`, `PolicyConfig.plugin_settings` becomes `engine_settings`, and
  `PolicyConfig::routing_enabled()` becomes an accessor that names a mode rather
  than a feature. Roughly two hundred references to each of the two YAML names
  exist across the workspace, so this is the largest mechanical change in the
  work and is worth landing on its own commit.
- R82c. An unrecognized `dispatch:` value is a load error naming both modes. A
  three-state key read leniently is how a typo becomes a silent mode switch, and
  the mode decides whether half the document is even legal.
- R83. The two are mutually exclusive and each rejects the other's keys by name.
  In hook mode, `routes:`, `groups:`, `global:`, and `global.defaults:` are load
  errors. In policy mode, per-plugin `conditions:` and `priority` are load errors,
  because nothing consults them once APL decides dispatch.
- R84. `dispatch:` defaults to `policy`.
- R85. Policy mode does not silently activate nothing. A configuration that
  declares plugins and nothing that reaches them is reported at load rather than
  loading inert, because R84 makes that state reachable for every config that
  never opted into routing.
- R85b. What the flip changes depends on the request, and the document states
  both shapes rather than one. A request carrying protocol metadata resolves to an
  installed annotation, or to nothing once R86 removes the activation lists:
  `resolved_name` is always `Some` for `entity_type == "http"`, either the matched
  selector or the reserved global name, so such a request never reaches the
  unfiltered path. A request carrying no `meta` or no `entity_type` returns every
  registered entry for the hook unfiltered (`engine.rs:1901-1905`), and today the
  same request in a routing-disabled config is filtered by each plugin's own
  `conditions:` instead. So the flip moves metadata-less requests from
  condition-filtered to unfiltered, and metadata-carrying ones from
  every-declared-plugin to only-what-a-policy-names.
- R85c. What counts as reaching a plugin is enumerated, because it is wider than
  `run(name)`: a `run(name)` step, a `run(name)` or `taint(...)` stage in an
  `args:` / `result:` pipeline, a `delegate(...)` call, an elicitation verb's
  handler, and an `authentication:` step at any scope. `identity.resolve` branches
  to `resolve_identity_plugins_for_route` independently of annotations, so an
  `authentication:` plugin is reached without any policy body naming it.
- R85d. Only the config visitor can evaluate R85. It sees the stacked compiled
  layers and the raw section YAML, so it can compute the full set R85c names;
  `ppe-core` depends on no `praxis-policy-apl-*` crate and can see neither.
  `warn_unreferenced_plugin_overrides` is the weaker cousin of this check, scoped
  to override maps. Where the check lives and whether a visitor-less host writing
  `dispatch: policy` is protected is a planning question, not a grammar one.
- R85e. In policy mode a request carrying no `meta` or no `entity_type` is
  denied, with its own 400-class violation code distinct from a policy deny,
  because a request the engine cannot identify cannot be authorized against
  entity-scoped policy. Firing every registered plugin unscoped is not the
  conservative alternative: those plugins evaluate against absent context, and a
  missing bag key makes a comparison false, so an authorization rule reaches no
  opinion and allows. The behavior is undefined rather than safe in either
  direction.
- R85f. That denial is guarded the way the unreadable-path denial already is: it
  applies only when a policy is installed that could have answered for the
  request. `RouteResolutionError::UnreadablePath` sets the precedent, denying only
  when an `http:` route is declared, on the reasoning that a request whose path
  cannot be interpreted cannot be authorized against path-scoped policy. A config
  that declares no policy does not start denying traffic it used to pass.
- R85g. The CHANGELOG names it. Before this work a metadata-less request in a
  routing-disabled config was filtered by each plugin's own `conditions:`; the
  default flip alone would move it to unfiltered dispatch, and R85e moves it to a
  denial, so a host that does not populate protocol metadata sees a behavior
  change either way.
- R86. Activation lists are removed from every scope, not only from a route.
  `routes[].plugins`, `groups.<name>.plugins`, `global.defaults.<entity>.plugins`,
  and the reserved `all` group's plugins are one construct, and dropping the
  route's alone would leave a worse asymmetry than the one it fixed. In policy
  mode a plugin runs because a policy step names it with `run(name)`.
- R86b. R86 states what replaces chain-wide activation rather than only what it
  removes. A policy-mode config runs one plugin across every request with a
  `run(name)` step under `global.authorization`, which stacks onto every entity
  route, and the migration for a config using the reserved `all` group's plugins
  is recorded. The residual case is named too: a plugin that must fire at a hook no
  APL block annotates has no policy-mode spelling, and `dispatch: hooks` is the
  answer for it.
- R87. `plugins:` as a list survives only as the top-level declaration block, in
  both modes. The `plugins:` override map survives in policy mode. Neither is an
  activation list.
- R89b. The `http:` route inertness report is deleted rather than reworded.
  `config.rs:1357-1364` reports that `http:` routes exist while routing is off,
  "which is the default". Once `routes:` is a load error in hook mode, a config
  carrying `http:` routes is necessarily in policy mode, so the branch has no
  reachable input. The no-catch-all report beside it stays.
- R89d. `response:` is treated as released surface, unlike the `http:` selector.
  It was introduced 2026-07-14 in `b8b4c0d` and shipped in `v0.1.0`, so any change
  to it needs a migration note rather than an in-place CHANGELOG edit. R57 removes
  its `apl:`-nested spelling, which is such a change.
- R89e. `response:` gains the CHANGELOG entry it never got. It shipped in
  `v0.1.0` with no entry describing it; its only mention is incidental, inside the
  unknown-route-keys entry listing the keys a route mapping legitimately carries.
  A released key with no changelog line is the same documentation gap this work
  exists to close, one level up from the grammar.
- R89c. The `http:` selector's entry in the unreleased CHANGELOG is edited in
  place, not migrated. It names `plugin_settings.routing_enabled: true` and says
  it "defaults to false", and both halves stop being true. An unreleased entry
  describing a key that never shipped under that name is a documentation bug, not
  a migration note.
- R89. `route_cache_max_entries` is policy-mode only and the document says so.
  `plugin_timeout` and `short_circuit_on_deny` are read by the executor in both
  modes.

**One spelling per construct**

- R68. `run(name)` is the only form that invokes a registered plugin, in step
  position and in pipeline-stage position alike. `plugin(name)` is removed from
  both. It is an alias today, recognized in `detect_step_kind`, in
  `parse_step_string`, and as a stage in `parse_stage`, and an alias is the same
  two-spellings-one-meaning problem the wrapper and the flat phase form were.
- R69. `plugin` survives as a noun and never as a verb: the kwarg that names
  which plugin inside `delegate(...)` and the elicitation verbs, and the `plugin:`
  key in the `delegate:` map form. One word, one part of speech, so `run` is the
  only thing that runs something and `plugin` is only ever what gets named.
- R69b. This picks the opposite primary from APL's first definition, which lists
  `plugin(name)` as the form and `run(name)` as the alias, and the document says so
  where it makes the choice. `run` is the verb every other step form is; `plugin`
  is the noun those forms take as an argument. The migration is roughly eighty
  policy-text occurrences of `plugin(` across nine files, against `run(` already in
  use in two.

**One structure, one table per scope**

- R63b. `when` leaves `KNOWN_ROUTE_KEYS` with the key itself, so a route still
  carrying it fails as an unknown route key rather than being accepted and scored.
- R64. The key sets coalesce. Every key is enumerated once, at the scopes it is
  valid, and no key appears in two tables. Today `FLAT_APL_KEYS` is a strict
  subset of `KNOWN_ROUTE_KEYS`, all seven of seven duplicated, and `pdp` and
  `session_store` appear in three tables at once.
- R65. No table or constant name implies a structural alternative that no longer
  exists. With the `apl:` wrapper gone there is no flat form to distinguish, so
  `FLAT_APL_KEYS` names a distinction that is not there. `GLOBAL_ONLY_NON_DSL_KEYS`
  states a scope and a negation in a name, both of which belong in the scope
  table instead.
- R66. Scope is a property of the table a key is in, not a warning issued after
  the key is accepted. A global-only engine block written on a route fails at
  load. Today `pdp:` and `session_store:` are listed as known route keys and only
  warn, while `attribute_files:` is not listed and errors, so the three engine
  blocks disagree about their own scope.
- R67. `args:` and `result:` are section-level in every scope and are never
  nested under `authorization:`, which the document states so the question does
  not arise.
- R67b. `plugins:` is documented per scope and per shape, because one key name
  carries two unrelated meanings decided by its YAML type: a list activates
  plugins, a map overrides settings of plugins something else activated. The
  document states which shapes each scope accepts.
- R67h. `authentication:` is documented in both its shapes, with the step grammar
  spelled out: a list of steps, or a map of `replace_inherited:` plus `steps:`,
  where a step is a bare plugin name or a map of `name:` with optional
  `config:`. The list form always means additive.
- R67k. A step's key set closes, which is what makes R76 safe.
- R67e. In policy mode a section may not carry a structural `plugins:` list at
  all, whether or not it declares an APL block. It is a load error naming
  `run(name)` as the way a policy invokes a plugin. The list belongs to hook mode,
  which keeps it.
- R67f. The reason is that the two are mutually exclusive per route and hook, and
  the split lands mid-route. The annotation short-circuit
  (`engine.rs:1859-1885`) returns the APL handler as the entire chain, so
  `resolve_plugins_for_entity` never runs for an annotated hook, while pre and
  post handlers install independently (`visitor.rs:855-856`) according to which
  phases the block declares. So a route with `authorization: {pre_invocation:}`
  and a `plugins:` list has that list inert on the pre hook and live on the post
  hook: one list, one route, two behaviors, decided by which phases the block
  happens to carry. Nothing reports it today; the only lint,
  `warn_unreferenced_plugin_overrides`, covers the override map rather than the
  activation list.
- R67g. The document states what each `plugins:` shape belongs to rather than
  describing them as two spellings. The list is `ppe-core`'s imperative chain:
  plugins fire at the hooks they declared, ordered by `priority` and bands, with
  `short_circuit_on_deny` and per-plugin `conditions:`. The override map is APL's,
  adjusting `config`, `capabilities`, and `on_error` for a plugin a step invokes,
  and it stays valid alongside an APL block.
- R67d. No key in the document is inert. Every key it lists changes behavior when
  declared, because the ones that did not have been removed, and the one worth
  keeping is honored. There is no "parsed but ignored" column to maintain.
- R67c. A `plugins:` shape a scope does not accept fails at load. Today a list
  under `global:` is dropped twice over: `GlobalConfig` has no such field, and
  `apl_subblock` copies `plugins` only when it is a mapping. So
  `global: { plugins: [audit-log] }` loads clean and activates nothing, which is
  the same silent-ignore shape as the legacy keys.

**`authorization:`**

- R59. An `authorization:` block declares at least one of `pre_invocation:` or
  `post_invocation:`. Declaring neither is a load error, not an empty block.
- R60. `authorization:` is the only place the two phase lists may appear. The
  flat `pre_invocation:` / `post_invocation:` spelling on a section is removed,
  so one structure carries the phases everywhere. This is the largest in-repo
  migration in this work: one YAML fixture and roughly twenty test files write
  the flat form today, and it is the form this repo writes almost exclusively.
- R60e. R60 retires a published compatibility guarantee, and the document says
  so. The one YAML in the tree writing the flat form is
  `crates/ppe-core/tests/fixtures/legacy-policy-document.yaml`, asserted by
  `crates/ppe-core/tests/wire_compatibility.rs` ("the rename must not move the
  policy wire surface"), and the 0.1.0 CHANGELOG names the policy document format
  as "deliberately unchanged" and as "the surface a deployment depends on".
  Rewriting that fixture is the action the test exists to prevent doing quietly, so
  the CHANGELOG states that the guarantee is retired rather than letting it lapse
  in a diff.
- R60b. Removing the flat spelling removes the code that reconciled the two.
  `ParseError::ConflictingAuthorizationForms`, which reports a section declaring
  a phase both nested and flat, has no reachable input afterward and is deleted
  with the merge that produced it.
- R60c. `args:` and `result:` stay section-level and are never nested under
  `authorization:`. They are phases, not authorization steps, and the document
  states this so the symmetry with `pre_invocation:` does not invite the guess.
- R60d. `args:` and `result:` are not accepted under `global:`. This is settled,
  not pending. It removes the only spelling for a field pipeline covering every
  entity route, so the CHANGELOG names it as a removed capability rather than a
  tightened one, and the scope tables are uniform as a result.

**IR vocabulary**

- R61. The IR names follow the config keys. `Phase::Policy` and
  `Phase::PostPolicy` take the invocation-phase names, `CompiledRoute.policy`
  and `.post_policy` follow, and the three struct fields named `identity` whose
  YAML key is `authentication:` are renamed to match it.
- R62. No diagnostic, rustdoc line, or serialized field names a config key that
  no config may contain. `crates/ppe-core/src/identity/route_config.rs` is the
  densest instance: its module and type documentation describe the block as
  `identity:` throughout, which is the key R51 removes.
- R62b. Where a type's documentation and its resolver disagree, the resolver is
  the fact and the documentation is the defect. `RouteIdentityConfig.replace_inherited`
  is documented as stored but not yet exercised, "no inheritance to override yet at
  route level", while `resolve_identity_plugins_for_route` honors it at route
  level today.
- R63. `CompiledRoute` is serialized, so its field rename changes the serialized
  shape as well as the Rust API. Both are listed.

**Keys that are parsed and ignored**

- R70. A key the runtime parses and never honors is removed, not documented and
  not implemented. Five qualify, each verified by reading its only readers:
  a route's `when:`, `plugin_dirs`, `engine_settings.parallel_execution_within_band`,
  `engine_settings.fail_on_plugin_error`, and an `authentication:` step's
  `on_error:`. One exception, R79.
- R71. Each becomes an unknown key at its scope, so a config still carrying it
  fails at load naming it rather than loading with it inert. The closed key sets
  are what make that true without a per-key check.
- R72. The warnings that existed to announce the ignoring are deleted with the
  keys. `engine.rs:405-431` warns "Setting ignored" for `plugin_dirs`,
  `parallel_execution_within_band`, and `fail_on_plugin_error`, and those three
  warnings have no subject afterward.
- R73. Each removal names its replacement in the load error and the CHANGELOG:
  `register_factory()` plus the `plugins:` block for `plugin_dirs`, per-plugin
  `mode: concurrent` for `parallel_execution_within_band`, per-plugin
  `on_error: fail` for `fail_on_plugin_error`, the plugin declaration's own
  `on_error:` for a step's, and an `authorization:` step for a route's `when:`.
- R74. Removing a route's `when:` removes the specificity bonus with it
  (`config.rs:1963`) and the dead carrier it filled: `ResolvedPlugin.when` and the
  assignment at `config.rs:1800`, which only tests read. This changes which route
  wins for a configuration that declared `when:` on one of two otherwise equally
  specific routes, and the CHANGELOG names that.
- R75. `when:` is the one removal that takes a capability rather than a no-op with
  it, and the CHANGELOG says so plainly. It never worked, so nothing regresses,
  but an operator who wrote one meant something by it and the migration note points
  at the `when:` / `do:` step that expresses it.
- R76. `RouteIdentityStep.extra`, the `flatten` catch-all that let any unknown
  field into a step, goes with the step's closed key set. It is what would have
  swallowed a re-added `on_error:` silently.

**`replace_inherited:` on a bundle**

- R79. `replace_inherited:` is honored at bundle scope rather than removed. It is
  the one parsed-and-ignored key worth keeping, because it expresses something no
  other key does: a bundle that supersedes inherited identity rather than adding to
  it. Today the route layer honors it and a tag bundle's is "parsed but not
  honored" by the resolver's own account.
- R80. Its meaning at bundle scope is stated: a bundle whose block sets the flag
  drops everything accumulated before it, the global layer and any earlier bundle,
  and layers after it still append. A route's flag continues to drop all inherited
  layers.
- R81. That ordering is well defined and stays that way. `route_static_tags`
  yields `meta.tags` in declaration order followed by `groups:` in declaration
  order, so which bundle replaces is deterministic and reproducible from the file.
  The non-deterministic tag iteration in `resolve_plugins_for_entity`, which walks
  a `HashSet`, is not a counterexample: it belongs to the activation lists R86
  removes.
- R80b. Honoring the flag at bundle scope is reported at load. Every route whose
  inherited global `authentication:` layer a bundle's `replace_inherited:` drops is
  named, in the shape the delegation-without-identity alarm already uses, because
  the change moves an authentication-removing control from route-local and visible
  to tag-inherited and remote: the route's own author never sees the bundle.
- R80c. The CHANGELOG names it as a behavior change for any config that already
  sets the flag on a bundle, parallel to R74's treatment of the `when:` bonus, and
  R47's external check covers it. Such a config sets a documented no-op today and
  silently starts dropping inherited authentication on upgrade.
- R81b. Whether runtime request tags compose for `authentication:` is settled in
  the same change. `resolve_plugins_for_entity` merges static and runtime tags;
  identity resolution walks only static ones. A flag that can drop inherited layers
  makes the difference load-bearing rather than cosmetic.

**Divergence closure**

- R39. A conformance corpus is checked in, with at least one accepted case and
  one rejected case per EBNF production, per documented wart, and per breaking
  change taken here. Each rejected case asserts the message class, not just
  that parsing failed.
- R41. Every divergence the audit found is resolved in the parser, in the
  document, or in both, and none is left as a note. The list below is the
  starting set, not the finished one; what the corpus turns up as it is written is
  resolved on the same terms.
- R42. The corpus is structured so the coverage work can consume it. The
  parser's error sites are reachable from it, and a case names the site it
  exercises.
- R43. No test in the corpus passes whichever way the parser behaves. A case
  whose assertion holds when its input is changed to something else is not a
  case.

**Migration**

- R44. Every change to accepted policy text is listed in the CHANGELOG with
  what it accepted before, what it accepts now, and how to rewrite the text.
- R45. The public entry points keep their behavior on input that is valid both
  before and after. `parse_predicate`, `parse_rule`, and `parse_pipeline` keep
  their signatures and their results on text the new grammar accepts.
  `compile_config` is deleted by R58 and is not among them.
- R46. In-repo policy fixtures are migrated in the same change. The
  `require(...)` forms in the legacy policy document fixture and the HTTP
  transpiler fixture keep their meaning, which R24 makes automatic, and any
  fixture that relied on a tightened form is rewritten.
- R47. Consumers outside this repo are checked, not assumed. The praxis-demos
  policies are read against the new grammar before this is called done, against
  every breaking change this work takes rather than a named subset, and whatever
  they rely on is named in the migration note. Measured already: that config needs
  exactly two changes, the `engine_settings` / `dispatch` rename and nesting four
  flat `pre_invocation:` blocks.
- R47b. One upgrade guide is a deliverable of this work, not a follow-on. It
  enumerates every key and form an existing 0.1.0 config must rewrite, with its
  before and its after, in one place. The document already holds the raw material,
  since every removal names its replacement; what it lacks is that content
  assembled once for the person paying the cost, who currently meets it one load
  error at a time.
- R48. `make ci` passes.

---

## Acceptance Examples

Not every requirement has a named example here, and that is a division of labor
rather than a gap. The lexical and predicate productions (R13-R22), the map-form
key rules (R35-R38), and the pipeline stage table (R31-R32) are verified by R39's
conformance corpus, which asserts an accepted and a rejected case per production;
restating them as prose would duplicate the corpus without adding verification.
The examples below carry what the corpus does not: cross-cutting behavior,
load-time diagnostics, and the migrations.

- AE1. **Covers R1, R2, R3, R4, R5.** Given the document, a policy author finds
  the accepted spelling of a comparison, the precedence of `!` against `&`, the
  escape sequences a reason string may contain, and the YAML key a step list
  goes under, without opening `parser.rs`.
- AE2. **Covers R9, R10, R11, R12.** Given `a == 'it\'s'`, the predicate parses
  and the value is `it's`. Given `deny('it\'s bad')`, the reason is `it's bad`
  with no backslash. Given `regex(")`, parsing fails naming the unterminated
  literal. Given `a == "x\qy"`, parsing fails naming the unrecognized escape.
- AE3. **Covers R13, R14, R15.** Given `a..b`, `a.`, `.a`, `data.t[]`,
  `data.t[a:b]`, and `1.`, each is rejected with a message naming the
  production it violates. Given a non-ASCII identifier, the message names the
  character rather than a byte offset.
- AE4. **Covers R18, R19.** Given `not authenticated`, parsing fails naming
  `!`. Given an attribute path beginning `not.`, parsing fails naming `not` as
  reserved. Given `a not in b`, it parses as it does today.
- AE5. **Covers R23, R24.** Given `require(role.hr)`, the compiled rule denies
  when `role.hr` is falsy. Given `require(team.engineering | team.security)`,
  it denies when both are falsy. Given `require(a, b)`, it denies when either
  is falsy. All three match what the parser produces today, asserted against
  the compiled IR rather than against the source text.
- AE6. **Covers R25, R26, R26b, R27.** Given `require(delegation.depth < 3)`, it
  parses and denies at depth 3. Given `require(a) & b`, the compiled IR is
  `!a & b`. Given `require(a, b | c)`, the compiled IR is `!(a & (b | c))`. Given
  `require(a): deny` and `require (a)`, both parse, and neither reports a lexer
  error about `:`. Given `require(a): allow`, load fails naming the inversion.
- AE7. **Covers R28, R29.** Given `result.x | redact` in rule position,
  parsing fails naming effect position, rather than compiling a disjunction.
  Given `data.t[subject.tenant] == "y"` as a bare predicate with a `:` inside
  the group, it parses as one predicate.
- AE8. **Covers R31, R32.** Given `mask(4) |`, parsing fails naming the empty
  stage. Given `validate(x)` anywhere, including on a field rule, the message
  names `regex("...")` and `run(name)` as the alternatives.
- AE9. **Covers R33, R34.** Given a step map keyed `whens:`, load fails naming
  the key. Given a step that names a custom dialect in the documented explicit
  spelling, it compiles and routes to a resolver registered under that dialect.
- AE10. **Covers R39, R43.** Given the corpus, every production has an accepted
  and a rejected case, and every documented wart has a rejected case. Given any
  corpus case, changing its input invalidates its assertion.
- AE11. **Covers R44, R45, R46, R47.** Given the CHANGELOG entry, each
  tightened form appears with a before, an after, and a rewrite. Given the
  in-repo fixtures, they compile unchanged where R24 preserves meaning. Given
  the praxis-demos policies, they are read against the new grammar and the
  result is stated.
- AE12. **Covers R6, R8.** Given `parser.rs`, its header describes the file and
  points at the document rather than restating the grammar. Given the document,
  each surviving wart is listed with its reason and has a corpus case.
- AE13. **Covers R51, R52, R53.** Given `apl:`, `policy:`, `post_policy:`, or
  `identity:` on a route, at `global:`, under `global.defaults.tool:`, or on a
  `groups.<name>:` bundle, load fails naming the key in every one of those
  positions. Given `global.policies:`, load fails the same way. Given a grep of
  the tree, no rename table and no rename error variant remains.
- AE14. **Covers R54, R55.** Given a misspelled key at `global:`,
  `global.defaults.tool:`, `groups.<name>:`, or on a route's APL block, load
  fails naming it. No scope accepts a key nothing reads.
- AE15. **Covers R56, R57.** Given `global: { attribute_files: [...] }` written
  flat, the static `data.*` tree loads. Given a `response:` block, exactly one
  precedence rule decides which one applies, and it is the same rule that
  decides for every other APL key.
- AE16. **Covers R59, R60.** Given `authorization: {}`, load fails naming the
  missing phase. Given `authorization: { pre_invocation: [...] }`, it loads.
  Given the same steps written flat on the route, the compiled result is
  identical.
- AE17. **Covers R61, R62, R63.** Given the compiled IR, no phase or field is
  named `policy` or `post_policy`, and no struct field is named `identity` whose
  YAML key is `authentication:`. Given a serialized `CompiledRoute`, its phase
  keys are the invocation names, and the CHANGELOG names that change.
- AE18. **Covers R49, R50, R58.** Given the grammar document, a reader learns
  what a policy may say without opening another document, and each decision that
  departs from how APL was first defined carries its reason at that decision.
  Given a grep for `routes:` across the workspace, one shape is defined.
- AE18a. **Covers R67h, R67k.** Given an `authentication:` step declaring
  `on_error:`, load fails naming the key as unknown. Given a misspelled key inside
  a step, load fails naming it.
- AE18c. **Covers R82, R83.** Given `dispatch: hooks` and a `routes:` block,
  load fails naming the key and the mode. Given `dispatch: policy` and a plugin
  declaring `conditions:`, load fails the same way.
- AE18d. **Covers R84, R85.** Given a config that declares plugins and no
  `routes:`, `groups:`, or `global:`, and no `engine_settings`, load fails naming
  the plugins nothing activates. It does not load with every plugin silently
  inert.
- AE18e. **Covers R72, R73, R86, R87.** Given a `plugins:` list on a route, a bundle,
  a `defaults:` entry, or the `all` group, load fails naming `run(name)`. Given
  `plugin_dirs`, `parallel_execution_within_band`, or `fail_on_plugin_error`, the
  document names each as ignored and names its replacement.
- AE18b. **Covers R67e, R67f, R67g.** Given a route in policy mode with a
  `plugins:` list, load fails naming `run(name)`, whether or not the route also
  declares an `authorization:` block. Given the same route with a `plugins:`
  override map instead, it loads.
- AE19. **Covers R70, R71, R72, R73, R74.** Given a route declaring `when:`,
  `plugin_dirs` at top level, `parallel_execution_within_band` or
  `fail_on_plugin_error` under `engine_settings:`, or `on_error:` inside an
  `authentication:` step, each fails at load naming the key and its replacement.
  Given two otherwise equally specific routes, neither outranks the other on the
  basis of a `when:` that no longer exists. Given a grep of the tree, no
  "Setting ignored" warning remains.
- AE20. **Covers R79, R80, R81.** Given a route joining two bundles where the
  second sets `replace_inherited: true`, the global layer and the first bundle's
  steps are dropped and the second bundle's plus the route's remain. Given the
  bundles named in the other order, the result differs and is reproducible from
  the file. Given a route that sets the flag, every inherited layer is dropped as
  it is today.

---

## Success Criteria

- A policy author can write a correct rule, a correct pipe chain, and a correct
  step map from the document alone.
- Every claim the document makes about what the parser accepts is asserted by a
  test, so the document cannot drift the way the header comment did.
- No text that looks like one construct silently parses as another. A field
  operation in rule position, a misspelled step key, and a lone quote in a
  stage argument all fail instead of compiling into something else.
- `require(...)` is one production with one meaning, no longer a special case
  in the rule parser, and every `require` line written before this change means
  what it meant before.
- Quoting and escaping have one implementation, and a backslash that protected
  a delimiter never reaches a policy value.
- Every rejection names the construct that failed and the position in
  characters. No rejection reports a byte offset or a mojibake character.
- Every divergence the audit found is pinned by a corpus case, and the document
  says plainly that the corpus bounds what is known rather than proving nothing
  else exists.
- The coverage work can start from the corpus rather than from reading 5,588
  lines to find the error sites.
- The accept set is the documented key set. A config written against the grammar
  document loads, and a key that document does not list does not.
- No removal trades a message that names the fix for silence. Every key this
  work drops fails at load at every scope it could be written.
- No key in the document is inert. Every key it lists changes behavior when
  declared; the ones that did not are gone, and declaring one never quietly
  changes which route wins.
- One word means one thing from config to diagnostic to serialized output. An
  operator who writes `authorization.pre_invocation` and `authentication` never
  reads `policy` or `identity` back out.

---

## Scope Boundaries

- Splitting `parser.rs` into modules. The issue opens with the line count, but
  no acceptance criterion asks for a split, and a structural diff on top of a
  grammar diff makes both harder to review. The EBNF names the natural seams;
  the split is its own change.
- Evaluator semantics. Truthiness, missing-key behavior, comparison coercion,
  and effect ordering are not this document's subject. The exception is a
  desugaring, which is grammar: `require(P)` reducing to `!P` is stated here
  because it is what removes the special case. The second exception is
  `replace_inherited:` at bundle scope, which changes identity-resolver behavior
  and is kept for the reason Key Decisions gives; it is named here so this section
  does not under-disclose the review surface.
- New language features. No new operators, no new stages, no new step forms, no
  exponent notation, no `in` over literal lists. A capability gap found while
  writing the EBNF is recorded, not filled, with the single exception of
  `require(...)` accepting a comparison, which falls out of R23 rather than
  being added.
- Error message localization or structured error codes. Messages improve where
  a requirement names them; the error type stays as it is.
- Raising the coverage floor. That is #14. This work produces material it can
  use and does not own the number.

### Tracked elsewhere

- **The paren form of a PDP call reaches a resolver as a scalar, and the
  shipped OPA resolver reads only the map body.** So `opa("p/q"): { ... }`, a
  form the parser's own rustdoc documents, cannot be consumed by the resolver
  in this repo. R38 documents what each form delivers; making the resolvers
  agree is separate.
- **Roughly 25 provably unreachable guards in `parser.rs`** cap achievable
  coverage and cannot be excluded on stable. Noted in #14, unchanged here.


---

## Key Decisions

- **`require(P)` means `!P`, which dissolves the special case instead of
  relocating it.** The tempting fixes are to keep `require` a rule form with a
  better error, or to give it a boolean reading distinct from its deny
  behavior. The first leaves the special case in `parse_rule` and fails the
  acceptance criterion. The second is incoherent: `require(a) & b` would have
  to mean something other than what `require(a)` alone means. Reading it as
  negation makes all three problems one problem. A bare predicate rule already
  defaults to deny, so `require(P)` as a rule is `!P` plus that default, which
  reproduces today's `IsFalse` desugaring exactly, including both separator
  forms by De Morgan. Nesting, comparisons, the space before the paren, and the
  colon-split error all resolve as consequences rather than as separate fixes.
  The cost is that `require` reads as a negation in nested position, which is
  the same inversion a bare predicate rule already carries.

- **The comma inside `require(...)` stays, as a low-precedence `&`.** Removing
  it would be the cleaner grammar and would break `require(role.hr, perm.x)`
  for no gain, since the comma has an exact predicate equivalent. Keeping it
  costs one production and preserves every line in use.

- **Escapes unescape.** The alternative is to specify that APL has no escape
  mechanism and reject `\` in a string literal outright, which is defensible
  and simpler. It is rejected because `split_top_level_commas` already treats
  `\` as an escape for splitting, so authors have been able to write
  `deny('it\'s bad')` and get a value with a stray backslash in it. Half an
  escape mechanism is worse than either whole answer, and the direction that
  loses nothing is to finish it.

- **A field operation in rule position becomes an error, and the rule that
  makes it one is positional rather than lexical.** Reserving the stage names
  would also work and would take `redact` out of the attribute namespace for
  every policy. Recognizing the shape instead, a path rooted at `args.` or
  `result.` followed by a top-level `|`, uses a predicate that already exists
  in the parser and costs nobody an attribute name.

- **The step map key set closes, and custom dialects get an explicit
  spelling.** Leaving the set open is what makes `whens: { a: 1 }` a decision
  point call. Closing it without a replacement would break a supported
  deployment, since both shipped resolvers can register under a custom dialect
  and the parser cannot know the registered set at load time. A load-time
  warning naming unrecognized keys is the lighter alternative and stays on the
  table; it is not preferred, because the failure it warns about is a step that
  never resolves, and a policy step that silently does not run is the shape
  worth failing on. No in-repo config uses a bare custom dialect key, so the
  migration is external only.

- **The grammar is authoritative, but only after each divergence is priced.**
  Not every difference resolves toward the document. `!a == "x"` binding as
  `!(a == "x")` and comparisons taking an attribute path only on the left are
  both stated and kept, because the parser's reading is defensible and changing
  it would break text silently rather than loudly. The rule applied throughout:
  tighten where the parser accepts what no author meant, keep where the
  parser's reading is merely unstated, and change semantics only where the
  current behavior produces a value or a decision the author did not write.

- **Closing the `global:` and `groups:` key sets is a prerequisite for the
  removals, not a companion improvement.** The five legacy keys are all
  currently loud: three are rejection guards whose entire purpose is to name the
  replacement, and `apl:` and `policies:` are live keys. Delete a guard at a
  scope that ignores unknown keys and the key becomes inert instead of invalid,
  which for `identity:` means authentication steps stop running and for
  `policies:` means bundles stop applying, both with no diagnostic. Routes are
  already covered by `reject_unknown_route_keys`; `global:`,
  `global.defaults.<entity>:`, `groups.<name>:`, and `RouteYaml` are not. So the
  closure has to land in the same change as the removals, and the sequencing is
  fixed rather than a matter of taste.

- **`apl:` removal is three sites and one dependency, and the dependency is
  easy to miss.** Dropping `"apl"` from `KNOWN_ROUTE_KEYS` is cosmetic;
  `apl_subblock` is where the behavior lives, and `FLAT_APL_KEYS` is seven keys
  that do not include `attribute_files:`. That key is read only as
  `global.apl.attribute_files`, so removing the wrapper without extending the
  flat set silently removes the only way to load a static `data.*` tree from
  configuration. The payoff for doing it anyway is that two opposite precedence
  rules collapse into one: `apl_subblock` makes a wrapper win over flat keys,
  and `response_yaml_block` deliberately inverts that for `response:` alone.
  Neither rule has a reason to exist once there is no wrapper.

- **`require(!delegated)` shows the restriction was accidental, not chosen.** APL's
  first definition uses that form; this parser rejects it, along with any
  comparison. Reading `require(P)` as `!P` is justified on its own merits, since it
  removes the special case and preserves every existing spelling by De Morgan, and
  the provenance is corroboration rather than the argument. It does remove the
  case for keeping the special handling on compatibility grounds: the special case
  is what makes the obvious spelling fail.

- **The key tables coalesce into one per scope, because the duplication is what
  let them disagree.** Measured: `FLAT_APL_KEYS` is 7 keys, all 7 also in
  `KNOWN_ROUTE_KEYS`; `pdp` and `session_store` are in three tables;
  `attribute_files` is in the global-only table but not the flat one, which is
  what makes it wrapper-only. Every one of those inconsistencies is a
  same-fact-in-two-places bug rather than a decision anyone took. One table per
  scope, with the shared APL keys named once and referenced, removes the class.
  The naming follows: with no wrapper there is no flat form, so a constant cannot
  be called `FLAT_`, and scope belongs in which table a key appears in rather
  than in a name plus a runtime warning.

- **`compile_config` is deleted rather than reshaped.** It is public on
  `ppe-apl-core`, not re-exported by the facade, and every caller in the
  workspace is a test across six files. Production compiles one route's block at
  a time through `compile_policy_block_value`, so the map-keyed `routes:` shape
  is exercised only by tests, defines a second meaning for the key, and disagrees
  with the visitor about `apl:` on top of that. Reshaping it to the
  selector-keyed list would make `ppe-apl-core` model selectors it has no reason to know about
  and duplicate `RouteEntry`. What the tests want from it is a compiled route
  plus a plugin registry, which is a test helper, not a public compiler.

- **`args:` and `result:` go from `global:`, and the scope tables are uniform as
  a result.** The case for keeping them was real and was weighed: APL's first
  definition names `global` `args:` directly, a `global` block layers onto every
  entity route so `global.result: { ssn: "redact" }` is the only way to cover every
  tool once, and `reject_field_stages_without_fields` already refuses those blocks
  exactly where a payload has no addressable fields, deliberately not at global
  scope. Against that: one scope accepting phase keys the others refuse is the kind
  of exception every reader has to learn, and a field pipeline that silently means
  nothing on the entity-less HTTP path is the same inert-surface problem this work
  exists to remove. The capability loss is real and the CHANGELOG names it as a
  loss.

- **Enforcing the nested `authorization:` form is the largest migration and the
  clearest simplification.** It is the last place two spellings mean one thing
  after the wrapper goes, and keeping it would leave `FLAT_APL_KEYS` needing a
  name for a distinction that only applies to two of its entries. Removing it
  also deletes the reconciliation code the duality required, including the error
  variant for declaring a phase both ways. The cost is real and counted: one
  fixture plus roughly twenty test files, and a departure from APL's first
  definition, which calls the two forms equivalent.

- **The parsed-and-ignored keys are removed rather than implemented, and `when:`
  goes with them.** Five keys are read by the config parser and honored by
  nothing. Implementing each was the alternative, and for `when:` it was briefly
  the plan: compile it into the route layer as an outer guard, which the IR already
  expresses and the visitor has a seam for. What that buys is a capability nobody
  has been able to use, at the price of a semantics an operator did not ask for.
  `when:` reads as a match condition and a guard is not one: a false guard skips
  the route's own effects while inherited layers still apply, where a non-matching
  route would have fallen to a different route. Removing the key says the true
  thing, which is that APL already expresses this with a `when:` / `do:` step, and
  it collapses the specificity bonus and the dead `ResolvedPlugin.when` carrier
  with it. The same reasoning covers the other four: each has a working
  replacement one line away, and a key that looks like it works is worse than a
  key that is not there.

- **`replace_inherited:` on a bundle is the exception, because it expresses
  something nothing else does.** The other four ignored keys each duplicate a
  working mechanism. This one does not: no other key lets a bundle supersede
  inherited identity rather than add to it, and the route-scope half of the same
  flag is already honored, so the gap reads as unfinished rather than as declined.
  Honoring it needs one thing the plugins path does not have, and already has it:
  a deterministic bundle order. `route_static_tags` yields `meta.tags` then
  `groups:`, both in declaration order, so which bundle replaces is reproducible
  from the file. `resolve_plugins_for_entity` walks a `HashSet` and is not
  deterministic, which would have made the same flag unspecifiable there, and is
  moot because the activation lists it serves are being removed.

- **A structural `plugins:` list and an APL block on one section is refused
  rather than documented.** The two are not two spellings of one thing:
  `plugins:` is `ppe-core`'s chain, which works with no APL anywhere and is the
  whole feature for a deployment that enables routing only to scope plugin
  activation. That is why the key exists on routes and groups and why it stays.
  What cannot stand is both on one section, because the annotation short-circuit
  makes the list inert exactly for the phases the block declares and leaves it
  live for the phases it does not. An operator writing both gets half a chain,
  and which half depends on whether they happened to write a `post_invocation:`.
  Documenting that is not an option; it is not a rule anyone would choose. The
  alternative to refusing is to make the list always inert when a block is
  present, which is quieter and silently drops plugins the operator listed.

- **`routing_enabled` becomes a mode selector, and flipping its default is safe
  only with the unreferenced-plugin check landing beside it.** The two models
  already exist and are already mutually exclusive in effect: with routing off,
  `routes:` is never consulted and every declared plugin fires under its own
  `conditions:`; with routing on, dispatch comes from routes, groups, and global.
  What was missing was that the boundary is stated and enforced, so a config could
  sit in one mode while writing the other's keys and simply have them ignored.
  Making the key the declared mode is the smallest change that fixes that.

  The default flip is where the risk is, and it is under-enforcement rather than
  a single fail-open. A config with no `plugin_settings` enforces today because
  hook mode runs every declared plugin, filtered only by each plugin's own
  `conditions:`. Under a `policy` default the same file lands in policy mode, and
  what happens then splits by request shape, which R85b states: a
  metadata-carrying request runs only what a policy names, and a metadata-less one
  runs everything registered, unfiltered. Neither is what the operator wrote. The
  load-time check R85 asks for catches the config that reaches nothing at all; it
  cannot catch a plugin narrowing from every hook it declared to the one step that
  names it, which is why the migration note has to say so.

  The names follow the semantics, by the same argument that retired
  `FLAT_APL_KEYS`. `routing_enabled` named one mechanism while selecting a world,
  and `plugin_settings` named the plugins while holding engine-wide settings, two
  of which are not about plugins at all. `engine_settings.dispatch: policy | hooks`
  says what it does and makes the default readable in the file rather than
  inferred from a boolean's absence. It also turns the default flip from a
  behavior change under an unchanged spelling into a change an operator has to
  read, which is the safer shape for a change that decides whether their plugins
  run.

- **The mode model ships in this issue, so the work is grammar plus config
  model.** Splitting it was the alternative and the line does not hold: the
  legacy-key removal already requires closing the key set at `global:`,
  `global.defaults.<entity>:`, and `groups.<name>:`, which is `ppe-core` config
  work by any reading. Two issues would both edit `config.rs`, both need the
  closed key sets, and each would be blocked on the other's half. One issue, in
  commits a reviewer can read in order, is the honest shape.

  What that costs is that the diff is not reviewable against the EBNF alone, which
  is the argument that kept the `parser.rs` split out of scope. It stays out for
  that reason: the file split has no dependency in either direction, while the mode
  model and the closed key sets are prerequisites for the grammar document being
  accurate about what a document may contain.

  The sequencing that keeps it readable, and the order the plan should take: the
  `engine_settings` / `dispatch` rename on its own commit, since it is roughly four
  hundred mechanical sites and nothing else should be hiding in it, which means it
  preserves today's effective default (`dispatch: hooks`) rather than carrying
  R84's flip; then the closed
  key sets, which every removal depends on; then the legacy keys and the mode
  boundary, which the closed sets make loud; then the IR renames; then the grammar
  proper, with the EBNF, the parser reconciliation, and the conformance corpus;
  then the migrations of fixtures and tests.

- **Exhaustiveness is what the corpus pins, and the document says so rather than
  claiming more.** An earlier draft added a randomized differential search on top
  of the corpus. It could not do the job: a generator derived from the EBNF emits
  in-grammar strings, so it finds false rejections and never the over-acceptance
  this work opens with (`a..b`, `1.`, `007`, `mask(4) |`), and finding those needs a
  recognizer for the grammar, which is the second drifting account R8 exists to
  prevent. R39 already requires a rejected case per documented wart, which covers
  every over-acceptance the audit found, so the search would have added a vetted
  dependency, a generator, and a permanent `make` target for close to no coverage.
  What replaces it is a statement of the limit: the corpus makes the known
  divergences permanent, and the audit is what bounds the known set.

---

## Dependencies / Assumptions

- The audit's findings are measured, not read. Every behavior this document
  asserts about today's parser was observed by parsing the input through
  `parse_predicate`, `parse_rule`, `parse_pipeline`, or `compile_config`.
- `require(...)` appears in two in-repo fixtures, using the bare and the `|`
  forms. Both are preserved by R24, so no fixture rewrite is expected from the
  `require` change, and that expectation is asserted rather than assumed.
- No in-repo YAML uses the paren form of a PDP call or a bare custom dialect
  key, so R33 and R38 have no in-repo migration cost. External configs are
  R47's subject.
- No in-repo policy string contains a backslash escape, so R10 changes no value
  produced from a checked-in fixture.
- `PdpDialect::from_key` falls through to `Custom(other)`, and both
  `cedar-direct` and the OPA resolver support registering under a custom
  dialect. The open key set is a feature with a typo problem, not an oversight.
- `AuthorizationYaml` already uses `deny_unknown_fields` to turn a silently
  dropped authorization block into a load error. That is the precedent R33
  follows, one level down.
- `cargo-mutants` and `cargo-semver-checks` targets exist, so a semver check on
  the public parser entry points is already available to R45.
- `response:` was designed as a route-level block rather than an APL term, and
  its own commit says so: "no new fields on `PluginViolation` and no new APL
  grammar". It is the transpiled form of a Kuadrant AuthPolicy `denyWith`, so its
  shape answers to an external source rather than to APL's own design. That is why
  it sits in the scope tables as a route and `global` key rather than among the APL
  keys, and it is part of the language as defined, so it stays.
- `response:` reads its value out of band from the route YAML, and that pattern
  was copied deliberately: its commit describes it as read "like the `policy:`
  block". `policy:` is one of the keys this work deletes, so once `apl:` goes,
  `response:` is the last consumer of that shape, which is what makes R57 the end
  of it rather than a local tidy-up.
- Repo convention, from `CONTRIBUTING.md`: requirement identifiers from this
  document must not appear in commit messages, code comments, rustdoc,
  changelog entries, test names, or pull-request descriptions. Describe the
  behavior instead.

---

## Outstanding Questions

### Deferred to Planning

- [Affects R9, R11][Technical] Which escape sequences the finite set contains.
  `\\`, `\'`, and `\"` are the minimum that make the rule closed. Whether
  `\n` and `\t` join them is a question of whether a deny reason ever wants a
  newline.
- [Affects R34][Technical] How a custom dialect is spelled explicitly. A
  wrapper key such as `pdp(name):`, a declaration in config that the parser
  reads, or a marker prefix on the key. The first is closest to the existing
  call syntax; the second is the only one that lets load time verify the name.
- [Affects R31][Technical] Whether an entirely empty pipe chain stays valid.
  `parse_pipeline("")` returns an empty pipeline today and is a public entry
  point, so R45 argues for keeping it, while a field declared in `args:` with
  no stages is almost certainly a mistake. The two may want different answers.
- [Affects R20][Technical] Whether a reversed comparison is rejected outright
  or rewritten. Rejecting is louder and is what happens today, badly.
- [Affects R15][Technical] Whether `007` stays an integer. Harmless, and
  leading zeros in a policy usually mean the author wanted a string.
- [Affects R36][Technical] How much of the route-level key ownership split can
  be stated from `ppe-apl-core` alone, and how much needs `ppe-core` read
  alongside it.
- [Affects R58][Technical] Which of three dispositions `ppe-apl-core`'s
  map-keyed `routes:` gets. Delete `compile_config` / `ConfigYaml` / `RouteYaml`
  and move its test callers to `compile_policy_block_value`, which removes a
  public entry point and the second shape together. Reshape its `routes:` to the
  selector-keyed list, which makes `ppe-apl-core` model selectors it
  does not model today and duplicates `RouteEntry`. Or keep it and rename it so
  it stops using the word `routes:` for a single route's block. The first leaves
  one shape in the project; the third is the smallest diff.
- [Affects R81b][Technical] Whether runtime request tags start composing for
  `authentication:`, or the static-only behavior is documented as intended. The
  plugins resolver merges both and identity walks static tags only; extending
  identity means threading the request's tags into a resolver that does not take
  them today.
- [Affects R60][Technical] Whether the flat-form migration lands as one
  mechanical pass over the twenty-odd test files or is split from the grammar
  work so a reviewer can read the two separately.
- [Affects R54][Technical] Whether the closed key sets at `global:`,
  `defaults.<entity>:`, and `groups.<name>:` are enumerated tables like
  `KNOWN_ROUTE_KEYS` or `deny_unknown_fields` on the structs. The structs carry
  APL blocks the typed parse deliberately ignores, which is why
  `KNOWN_ROUTE_KEYS` exists in that form for routes, so the same shape probably
  applies. Worth confirming before writing four tables.
- [Affects R61][Technical] The exact spellings for the renamed phases.
  `Phase::PreInvocation` / `PostInvocation` mirrors the config keys;
  `CompiledRoute.pre_invocation` / `post_invocation` follows. Whether
  `CompiledRoute.args` and `.result` stay as they are, given they already match
  their config keys.
- [Affects R51][Technical] Whether removing `global.policies:` needs a merge
  path retired as well. Both locations are merged into one internal bundle map
  at parse, with top-level `groups:` winning on collision, so the merge has one
  input after the removal.
- [Affects R2, R32][Technical] Whether the elicitation verbs and the delegate
  kwarg forms are stated as full productions or as a table plus a shared kwarg
  production. Six verbs share one argument parser, so one production and a verb
  table is likely the honest shape.
- [Affects R47b][Technical] Whether the upgrade guide lives in the CHANGELOG as a
  per-form list or as its own document the CHANGELOG points at.
