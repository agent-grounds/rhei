# CLI examples

Validate a plan with the built-in default states definition:

```bash
cargo run -p rhei-cli -- validate examples/release-automation.rhei.md
```

Validate using a specific states file:

```bash
cargo run -p rhei-cli -- --state-machine docs/functional-spec/states.yaml validate examples/release-automation.rhei.md
```

Watch a plan and states file for changes:

```bash
cargo run -p rhei-cli -- validate --watch examples/release-automation.rhei.md
```

Repair missing export-prior edges with `rhei migrate export-priors --dry-run
PATH`, then `rhei migrate export-priors PATH`; see [§FS-rhei-migrate](functional-spec/rhei-migrate.spec.md#fs-rhei-migrate-rhei-migrate).

Render a plan as pretty JSON:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format json --pretty
```

Inspect the effective agents, models, and defaults for a plan:

```bash
cargo run -p rhei-cli -- roster examples/release-automation.rhei.md
```

Emit the complete merged roster and its provenance as JSON:

```bash
cargo run -p rhei-cli -- roster examples/release-automation.rhei.md --json
```

Both views use the execution settings merge; see
[Agents and models](functional-spec/rhei-agents.spec.md#11-global-and-project-settings).

Render a plan as GitHub-style markdown without metadata or subtask body text:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format github --no-metadata --no-content
```

Render a terminal progress report without ANSI color:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format progress --no-color
```

Reprice one completed run from its recorded measurements with a later book:

```bash
rhei cost WORKSPACE --by run
rhei summary WORKSPACE --run RUN_ID --prices later-prices.json
```

This summary is read-only: ordinary accounting keeps its stored prices. The
positional workspace and any repeatable `--rhei` flags bound which run records
and immutable completion report may be selected.

Render a self-contained HTML Flow visualization and open it in the browser:

```bash
cargo run -p rhei-cli -- viz examples/release-automation.rhei.md --open
```

`rhei viz <plan|workspace>` writes a single offline HTML page — the plan, every
task and subtask with its state, the resolved state machine, and the surroundings
inspector — under the workspace's `runtime/` directory (`runtime/<input>.html`,
or `runtime/rhei-viz.html` for a workspace directory), the same place a live run
freezes its final dashboard, or to `--output <FILE>`. Writing under `runtime/`
keeps generated HTML out of the source tree. It is the same Flow surface
`rhei run` serves live, frozen to a file; the live agent terminal and intervene
composer are inert in the static page. See
[`docs/functional-spec/rhei-viz.spec.md`](functional-spec/rhei-viz.spec.md).

Message a running agent during a live run (the headless sibling of the Flow
dashboard's intervene composer):

```bash
cargo run -p rhei-cli -- intervene --plan examples/release-automation.rhei.md \
  --task 3 -m "focus the review on error handling"
```

`rhei intervene` discovers the live run's dashboard from `runtime/dashboard.json`
and writes the message to the target agent's stdin — the same `/intervene`
channel the dashboard composer uses, never a plan transition. It only reaches
agents whose profile keeps stdin open (`intervene_stdin`); see [Enabling live
intervention](functional-spec/rhei-agents.spec.md#112-agents). Every
delivery is recorded to `runtime/interventions.log`.

Claim the next ready task and inspect its instructions:

```bash
cargo run -p rhei-cli -- next examples/release-automation.rhei.md
```

Complete a task and record the result:

```bash
cargo run -p rhei-cli -- complete examples/release-automation.rhei.md --task 1 --result "Brief approved"
```

For a multiline or Markdown-heavy result, read a named file or standard input
instead of placing its contents in a shell-expanded argument:

```bash
cargo run -p rhei-cli -- complete examples/release-automation.rhei.md --task 1 \
  --result-file result.md

cargo run -p rhei-cli -- complete examples/release-automation.rhei.md --task 1 \
  --result-file - <<'EOF'
Replayed onto `origin/main`.
Kept the result text verbatim.
EOF
```

Print crate versions surfaced by the CLI:

```bash
cargo run -p rhei-cli -- version
```

Reset a plan back to the initial state declared in its state machine:

```bash
cargo run -p rhei-cli -- --state-machine docs/functional-spec/states.yaml reset examples/release-automation.rhei.md
```
