### Task tui-monitoring: Audit run TUI, journal, and live monitoring behavior
**State:** scope-spec

Audit the monitoring and terminal UI surface for running plans.

Spec scope:

- `docs/functional-spec/rhei-run-tui.spec.md`
- TUI and journal requirements referenced by `docs/functional-spec/rhei-run.spec.md`

Implementation roots:
- `crates`
- `skills`
- `crates/rhei-cli/templates`
- `examples`

Focus on event semantics, transition journal format, stdout compatibility,
agent traffic interception, lifecycle event preservation, log tailing, slot
layout, non-TTY behavior, `--tui` / `--no-tui`, and failure/timeout display.
