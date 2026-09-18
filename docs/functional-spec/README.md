# Functional spec

Rhei user-visible behavior and requirements live here as grund declarations.
Each textual spec file keeps the repo's `.spec.md` suffix convention and declares
one `FS-<slug>` ID at its H1.

| ID | Subject |
|---|---|
| [§FS-rhei-language-reference](rhei-language-reference.spec.md#fs-rhei-language-reference-rhei-language-reference) | Entry point for the complete user-authored Rhei language surface |
| [§FS-rhei-panta](rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis) | Panta, the invisible project root above all rheis and tickets |
| [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification) | Rhei plan language grammar and semantics |
| [§FS-rhei-usage](rhei-usage.spec.md#fs-rhei-usage-how-rhei-is-used) | Roles, coordination patterns, and agent workflows |
| [§FS-rhei-authoring](rhei-authoring.spec.md#fs-rhei-authoring-rhei-plan-language-usage-guide) | Practical plan authoring guide |
| [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification) | State machine format and default states |
| [§FS-rhei-transitions](rhei-transitions.spec.md#fs-rhei-transitions-rhei-transitions-specification) | Transition system, callbacks, and YAML schema |
| [§FS-rhei-callbacks](rhei-callbacks.spec.md#fs-rhei-callbacks-transition-callback-examples) | Transition callback examples |
| [§FS-rhei-agents](rhei-agents.spec.md#fs-rhei-agents-rhei-agents-specification) | Agent configuration, execution, and timeout behavior |
| [§FS-rhei-programs](rhei-programs.spec.md#fs-rhei-programs-rhei-program-states-specification) | Deterministic program states |
| [§FS-rhei-supervision](rhei-supervision.spec.md#fs-rhei-supervision-subtree-supervision-specification) | Subtree supervision: a parent woken at task or state checkpoints of its descendants |
| [§FS-rhei-memory](rhei-memory.spec.md#fs-rhei-memory-mid-term-memory) | Mid-term memory: how an invocation reads what the project did before it, by a fixed algorithm |
| [§FS-rhei-errors](rhei-errors.spec.md#fs-rhei-errors-cli-errors-and-guidance) | CLI error anatomy, help lines, and copy-paste safety |
| [§FS-rhei-validate](rhei-validate.spec.md#fs-rhei-validate-rhei-validate) | `rhei validate` command behavior |
| [§FS-rhei-render](rhei-render.spec.md#fs-rhei-render-rhei-render) | `rhei render` command behavior |
| [§FS-rhei-states-cmd](rhei-states-cmd.spec.md#fs-rhei-states-cmd-rhei-states) | `rhei states` command behavior |
| [§FS-rhei-run](rhei-run.spec.md#fs-rhei-run-rhei-run) | `rhei run` command behavior |
| [§FS-rhei-run-report](rhei-run-report.spec.md#fs-rhei-run-report-per-run-report) | Durable per-run Markdown report and dashboard affordance |
| [§FS-rhei-run-tui](rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal) | `rhei run` TUI and transition journal |
| [§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting) | Agent token/cost accounting and visualization |
| [§FS-rhei-budgets](rhei-budgets.spec.md#fs-rhei-budgets-bounded-neural-work-and-budget-operations) | Persistent admission bounds, provider qualification, settlement, and budget commands |
| [§FS-rhei-summary](rhei-summary.spec.md#fs-rhei-summary-rhei-summary) | `rhei summary`: a pull-request-ready Markdown account of a run |
| [§FS-rhei-next](rhei-next.spec.md#fs-rhei-next-rhei-next) | `rhei next` command behavior |
| [§FS-rhei-transition-cmd](rhei-transition-cmd.spec.md#fs-rhei-transition-cmd-rhei-transition) | `rhei transition` command behavior |
| [§FS-rhei-complete](rhei-complete.spec.md#fs-rhei-complete-rhei-complete) | `rhei complete` command behavior |
| [§FS-rhei-release](rhei-release.spec.md#fs-rhei-release-rhei-release) | `rhei release` command behavior |
| [§FS-rhei-reset](rhei-reset.spec.md#fs-rhei-reset-rhei-reset) | `rhei reset` command behavior |
| [§FS-rhei-list](rhei-list.spec.md#fs-rhei-list-rhei-list) | `rhei list` command behavior |
| [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization) | Flow visualization: the primary plan/machine visualization surface |
| [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification) | Rhei template format and instantiation behavior |
| [§FS-rhei-snapshots](rhei-snapshots.spec.md#fs-rhei-snapshots-rhei-session-snapshots-specification) | Session snapshot/inheritance model, storage, runtime, and per-agent integration |
| [§FS-rhei-snapshot-operations](rhei-snapshot-operations.spec.md#fs-rhei-snapshot-operations-rhei-snapshot-operations-specification) | Snapshot CLI, run override, settings, redaction, and rollout |
| [§FS-rhei-completions](rhei-completions.spec.md#fs-rhei-completions-rhei-completion-ux-specification) | Shell completion UX |
| [§FS-rhei-install-skills](rhei-install-skills.spec.md#fs-rhei-install-skills-rhei-install-skills) | `rhei install-skills` command behavior |
| [§FS-rhei-version](rhei-version.spec.md#fs-rhei-version-rhei-version) | `rhei version` command behavior |
| [§FS-rhei-distribution](rhei-distribution.spec.md#fs-rhei-distribution-rhei-distribution-and-release-process) | Distribution targets and release process |
| [§FS-rhei-state-machine-writer](rhei-state-machine-writer.spec.md#fs-rhei-state-machine-writer-rhei-state-machine-writer) | State machine writer guidance |
| [§FS-rhei-init](rhei-init.spec.md#fs-rhei-init-rhei-init) | `rhei init` |
| [§FS-rhei-new](rhei-new.spec.md#fs-rhei-new-rhei-new) | `rhei new` |
| [§FS-rhei-run-headless](rhei-run-headless.spec.md#fs-rhei-run-headless-detached-runs) | Detached Runs |
| [§FS-rhei-run-json](rhei-run-json.spec.md#fs-rhei-run-json-rhei-run---json) | `rhei run --json` |
| [§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux) | Console-First Visualization UX |

This index is navigational. Normative citations should target the specific
declaration ID rather than this file.

Supporting product documents:

- [Project purpose](../grund.md) [§GND-rhei-purpose](../grund.md#gnd-rhei-purpose-governed-agent-work)
- [Goals](goals.md)
- [Requirements](../requirements/README.md) [§REQ-cross-platform](../requirements/cross-platform.md#req-cross-platform-one-tool-on-linux-macos-and-windows)
- [Roadmap](roadmap.md) [§RM-rhei-roadmap](roadmap.md#rm-rhei-roadmap-roadmap)
- [Comparison](comparison.md)
- [Rhei vs. beads](rhei-vs-beads.md)
- [Tab completions setup](tab-completions.md)
- [PM review notes](pm-review-2026-04-22.md)
