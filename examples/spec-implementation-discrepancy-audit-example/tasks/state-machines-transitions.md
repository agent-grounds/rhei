### Task state-machines-transitions: Audit state machine and transition semantics
**State:** scope-spec

Audit state-machine YAML semantics and explicit transition behavior for
Rhei.

Spec scope:

- `docs/functional-spec/rhei-states.spec.md`
- `docs/functional-spec/rhei-transitions.spec.md`
- `docs/functional-spec/rhei-state-machine-writer.spec.md`

Implementation roots:
- `crates`
- `skills`
- `crates/rhei-cli/templates`
- `examples`

Focus on profiles, node policy, state fields, terminal and gating behavior,
artifact contracts, callbacks, counted visits, polling, concurrent state
scheduling metadata, and transition selection/validation semantics.
