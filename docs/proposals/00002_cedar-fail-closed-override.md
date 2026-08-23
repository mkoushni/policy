---
issue: https://github.com/praxis-proxy/policy/issues/2
discussion: >-
  Originally opened as praxis-proxy/policy#2, which served as the
  approved discussion artifact. No separate GitHub Discussion was
  opened.
status: proposed
authors:
  - mkoushni
graduation_criteria:
  - A test that is red if translate() starts trusting Cedar's own
    decision, or if the Deny reason loses the fail-closed label and
    evaluation error text
stakeholders:
  - araujof
  - terylt
---

# Pin Cedar's fail-closed override

## What?

Make the Cedar PDP's fail-closed promise a tested contract, not a
comment. If any policy errors at evaluation, the request is Denied
even when Cedar itself would Allow. CEL already has a test for that.
Cedar does not.

### Goals

- Pin the product promise: a Cedar evaluation error cannot become
  an Allow.
- Cover the case the override exists for — Cedar Allows because a
  permit fired, and a sibling policy errored — not only the case
  Cedar would already Deny.
- Fail loudly if `translate()` is reordered, or a Cedar upgrade
  changes how evaluation errors are reported, and we start trusting
  `decision()`.
- Keep the Deny reason operator-visible: it says fail-closed, and
  it carries the evaluation error.

### Non-Goals

- Changing `rule_source`. Today it names the permit that fired,
  which is a bit strange on a Deny. Not this issue.
- Adding an `on_error: allow` escape for Cedar. CEL has that.
  Cedar does not, and this work should not grow one.
- Reworking the entity builder's empty defaults for `roles`,
  `permissions`, `teams`, and `claims`. Those exist so a missing
  `roles` is empty rather than an error. This proposal is about
  the residual case that builder cannot paper over.

## Why?

### Motivation

An access-control engine that sometimes cannot finish evaluating
still has to answer. The answer we chose is Deny. An Allow on a
partially failed Cedar evaluation is an untrusted decision, and
an untrusted decision is worse than a closed gate.

That override already lives in `translate()`. Nothing in the suite
puts Cedar in the state it exists for, so a refactor or a Cedar
upgrade can remove or invert it and every test still passes. The
CEL dialect already pins the same promise. Cedar should too.

This is not a coverage hole. It is the last line of defense on
this dialect.

### User Stories

- As an operator, I want a Cedar eval error to Deny even when
  another permit in the set would fire, so a broken `when` clause
  cannot open the gate.
- As a maintainer, I want a test that fails by becoming Allow if
  we start trusting Cedar's own `decision()`, so the override
  cannot disappear silently.
- As an on-call, I want the Deny reason to say fail-closed and
  name the evaluation error, so I can tell a closed gate from a
  normal default-deny.

## How?

### Requirements

- Drive the case through `CedarDirectResolver::evaluate`, the
  route-time path. A hand-built `Response` into `translate()`
  would bypass Cedar's error reporting.
- The fixture is a permit that would fire plus a sibling `when`
  that errors at evaluation. Without the override that fixture
  Allows. With it, Deny, and the reason contains `fail-closed`
  and the originating evaluation error. A Deny-only assertion
  would still pass if the reason were stripped.
- A lone erroring policy is not the fixture. Cedar default-denies
  when nothing permitted, so asserting Deny there still passes if
  the override is gone.

### Design

`translate()` already Denies when `diagnostics().errors()` is
non-empty, regardless of `decision()`. The test has to put Cedar
in that state, not construct the `Response` by hand.

`build_principal()` sets `id`, `type`, `roles`, `permissions`,
`teams`, and `claims`. It does not set `department`. A `when {
principal.department == "eng" }` is therefore a Cedar runtime
evaluation error.

Policy set: an unconditional permit with `@id("allow-all")`, plus
that `when`. Cedar records the error, still Allows on the permit,
and `translate()` must Deny with a fail-closed reason that carries
the error text.

One test in `builtins/pdps/cedar-direct/tests/basic_allow_deny.rs`,
same pattern as the other allow/deny cases: build a resolver, call
`evaluate`, assert the decision. No new file, no schema, no change
to `translate()`.

`principal.roles.contains(1)` is the wrong construction. Cedar
returns false for set non-membership without raising an error, so
the request Allows and the override never runs.
