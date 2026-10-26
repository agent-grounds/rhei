### Task run-orchestration-agents-programs: Audit run orchestration, agents, programs, and callbacks
**State:** scope-spec

Audit autonomous execution semantics for `rhei run`.

Spec scope:

- `docs/functional-spec/rhei-run.spec.md`
- `docs/functional-spec/rhei-agents.spec.md`
- `docs/functional-spec/rhei-programs.spec.md`
- `docs/functional-spec/rhei-callbacks.spec.md`
- relevant workflow sections in `docs/functional-spec/rhei-usage.spec.md`

Implementation roots:
- `crates`
- `skills`
- `crates/rhei-cli/templates`
- `examples`

Focus on ready-set selection, parallel scheduling, concurrent state handling,
gating barriers, subprocess completion authority, output artifact enforcement,
settings merge order, agent registry resolution, target selectors, timeouts,
callback invocation, and failure routing.
