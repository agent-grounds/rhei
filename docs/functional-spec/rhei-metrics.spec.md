# FS-rhei-metrics: Declared Metrics

Loop-shaped workflows exist to move a number: coverage climbs, defects fall,
latency shrinks. Today that number lives only in workspace-specific artifacts,
so answering "which session produced which change" means replaying transition
history by hand. A workspace may declare its metrics, and Rhei then records —
at execution time, as a first-hand fact — which measurement produced which
value and which sessions ran inside that measurement's window, and presents the
trajectory in its reports. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

Rhei never computes a metric and never trusts an agent's claim about one. The
workspace's own measure states produce the values; Rhei only reads what they
wrote and binds it to the sessions on record. An agent can never claim
progress: only re-measurement moves a metric.

## 1. Declaration

Metrics are declared in a top-level `metrics:` mapping in `states.yaml`,
extending the schema of [§FS-rhei-states.1.1](rhei-states.spec.md#11-top-level-fields):

```yaml
metrics:
  api-coverage:
    label: API coverage
    measured_by: [api-measure]
    drivers: [api-cover]
    artifact: runtime/code-coverage/validation/api-cover-report-{iteration}.json
    pointer: /summary/coveragePercent
    detail: "{/summary/covered}/{/summary/total}"
    unit: "%"
    goal: increase
```

| Field | Required | Meaning |
|-------|----------|---------|
| `label` | no | Display name; defaults to the metric key. |
| `measured_by` | yes | List of state names whose successful runs materialize the metric. Always a list, even with one entry. |
| `drivers` | no | List of state names whose sessions are the intended movers of the metric; used for presentation emphasis only. |
| `artifact` | yes | Workspace-relative path template of the per-iteration boundary artifact, with `{iteration}` substituted. Any file format: only its existence marks a successful measurement. |
| `value_artifact` | no | Path template of the JSON document holding the value, when it differs from `artifact`. Defaults to `artifact`. |
| `pointer` | yes* | JSON Pointer into the value document, `{iteration}`-templated, yielding the value. Required unless `program` is given. |
| `program` | yes* | Command whose stdout is the value, run with `{iteration}` substituted in its arguments; the escape hatch for non-JSON value sources. Mutually exclusive with `pointer`. |
| `detail` | no | Template of `{<json-pointer>}` segments rendered beside the value, e.g. `673/801`. |
| `unit` | no | Suffix rendered after the value, e.g. `%`. |
| `goal` | no | `increase` or `decrease`; orients delta arrows and regression emphasis. |
| `kind` | no | `number` (default) or `string`; string metrics render values without deltas. |

Validation rejects a metric whose `measured_by` or `drivers` name states absent
from the machine, and a metric with both `pointer` and `program` or neither.
JSON is the recommended value-source contract; a workspace whose measurement is
not JSON either extracts with `program` or writes a small JSON sidecar from its
measure state.

## 2. Measurement Model

The **iteration number** is the deterministic key binding sessions to metric
changes. Driver sessions cause a change; only a measure run materializes it, so
the measure run anchors the record and clocks are never consulted.

- Iterations count from 0. Iteration 0 is the baseline: the first successful
  measurement, before any driver session's effect is measured.
- After a `measured_by` state's run completes, the engine checks whether the
  `artifact` path for the next unclaimed iteration now exists. If it does, the
  measurement succeeded: the engine resolves the value and appends the
  iteration record (§3). If it does not, the measurement failed: the iteration
  counter does not advance, and the failed run is a recorded fact, not a gap.
  Artifact existence is the sole success criterion — Rhei does not interpret
  the measure state's exit codes, whose meaning belongs to the workspace.
- A measurement's **window** is everything since the previous successful
  measurement of that metric. Every agent session of the owning machine's
  tasks that ran inside the window belongs to the iteration — driver sessions
  and repair sessions alike.
- The engine exposes the next iteration number to measure-state invocations as
  `RHEI_ITERATION`, so a new workspace can name its artifacts from Rhei's
  counter instead of keeping a second one. Workspaces that already number
  their artifacts keep working unchanged: existence-checking makes the two
  counters agree or fail loudly, never drift silently.

## 3. Iteration Record

Each successful measurement appends one line to
`runtime/metrics/<metric-key>.jsonl`, written by the engine at the moment the
measurement is confirmed — the same durability model as
`runtime/state-transitions.log`: append-only, crash-safe, one fact per line.

```json
{"iteration": 2, "value": 94.66, "detail": "673/801", "unit": "%",
 "artifact": "runtime/code-coverage/validation/api-cover-report-2.json",
 "measure_state": "api-measure", "measure_visit": 4,
 "sessions": [
   {"state": "api-cover", "visit": 2, "driver": true,
    "log": "runtime/logs/task-x.y-api-cover-model-2.log"},
   {"state": "api-fix", "visit": 1, "driver": false,
    "log": "runtime/logs/task-x.y-api-fix-model-1.log"}
 ],
 "recorded": "2026-08-23T23:44:43Z"}
```

This file is the single source of truth for session-to-metric binding. Every
presentation surface reads it; none re-derives the binding from transition
history, file timestamps, or artifact scans. It is also the machine-readable
contract for external consumers — dashboards, CI, ad-hoc scripts — which need
no knowledge of the workspace's measurement internals.

## 4. Presentation

Presentation never renames sessions and never collapses shared credit:

- **Sessions keep their visit identity.** A session is `api-fix #1` because its
  log is the visit-1 log ([§FS-rhei-agents.8.1](rhei-agents.spec.md#81-log-file-naming)); the iteration it belongs to
  is worn as a tag, not substituted for its number. Visit and iteration
  counters drift apart exactly when repairs intervene, and both must stay
  legible.
- **The delta belongs to the window.** When one session fills a window, the
  report may attribute the change to it plainly. When several share it, all
  are listed, drivers emphasized, and the delta is presented as measured over
  the window — never assigned to one session by guess.
- **Failed measurements are rows, not gaps.** A measure run that produced no
  artifact appears in the trajectory with its repair sessions, keeping pass
  numbering honest.

Two surfaces render the record:

1. **Run metrics summary** — `runtime/reports/metrics-summary.md`, one section
   per metric: a trajectory table (iteration, sessions in window with links to
   their session reports, value, delta, source artifact) and the declaration's
   label, measured-by states, and goal.
2. **Session report metrics strip** — each session report
   ([§FS-rhei-session-reports.2](rhei-session-reports.spec.md#2-markdown-ui)) of a session bound to an iteration shows
   before/after/delta, the sessions sharing its window, and the source
   artifact.

Latest values also surface in the run dashboard data
([§FS-rhei-run-report.7](rhei-run-report.spec.md#7-dashboard-affordance)) under a `metrics` key mirroring the last line of
each iteration record.

## 5. Non-Goals

- Rhei computes no metric values and ships no format parsers beyond JSON
  Pointer; XML, CSV, and regex extraction stay in workspace `program`s.
- No thresholds, gates, or stop decisions: whether a trajectory is good enough
  belongs to the workspace's own programs and transitions.
- No aggregation across workspaces or runs; the record is per-workspace,
  per-metric.
- Declared metrics change nothing about scheduling or transitions: a workspace
  with metrics runs exactly as one without.
