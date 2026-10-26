### Task manual-commands: Audit manual worker and inspection commands
**State:** scope-spec

Audit the command contracts used by humans and manual agents outside the full
`rhei run` orchestrator.

Spec scope:

- `docs/functional-spec/rhei-next.spec.md`
- `docs/functional-spec/rhei-transition-cmd.spec.md`
- `docs/functional-spec/rhei-complete.spec.md`
- `docs/functional-spec/rhei-reset.spec.md`
- `docs/functional-spec/rhei-list.spec.md`
- `docs/functional-spec/rhei-states.spec.md`
- `docs/functional-spec/rhei-viz.spec.md`
- relevant command-surface sections in `docs/functional-spec/rhei-usage.spec.md`

Implementation roots:
- `crates`
- `skills`
- `crates/rhei-cli/templates`
- `examples`

Focus on claimability, assignee behavior, state instructions, transition
authorization, completion result files, reset semantics, list output, state
inspection, graph rendering, and CLI diagnostics.
