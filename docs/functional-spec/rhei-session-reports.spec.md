# FS-rhei-session-reports: Per-Session Reports

Every agent invocation already leaves a complete transcript: one session, one
log file under `runtime/logs/` ([§FS-rhei-agents.8](rhei-agents.spec.md#8-log-capture)). That log is written for
machines — a header, a raw JSONL event stream, an exit footer — and a human who
wants to know what a session actually did must scroll megabytes of deltas. Rhei
renders each session log into one readable Markdown report so an operator can
review a session — its prompt, its tool calls, what it wrote, and what it
achieved — without parsing the stream. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

The report is a derived artifact. It is computed only from the session log and
from facts the engine recorded while the session ran; regenerating it is always
safe, and deleting it loses nothing. It complements the per-run report
([§FS-rhei-run-report](rhei-run-report.spec.md#fs-rhei-run-report-per-run-report)), which aggregates a whole run: the run report answers
"what happened to every task", the session report answers "what did this one
invocation do".

## 1. Report Artifact

For every agent session log `runtime/logs/<name>.log` the renderer writes
`runtime/reports/<name>.md`. The report file name mirrors the log file name
exactly, including visit and attempt suffixes ([§FS-rhei-agents.8.1](rhei-agents.spec.md#81-log-file-naming)), so the
one-session-one-log invariant extends to reports: one session, one log, one
report, all three sharing a stem.

Program-state logs ([§FS-rhei-programs](rhei-programs.spec.md#fs-rhei-programs-rhei-program-states-specification)) are not rendered: they are already
plain stdout/stderr and carry no event stream. A run whose workspace declares
metrics also gets a run-level metrics summary; that artifact belongs to
[§FS-rhei-metrics.4](rhei-metrics.spec.md#4-presentation).

Reports use relative links — to the source log, to sibling session reports, and
to artifacts — so they stay useful after the workspace is committed, moved, or
pasted into an issue.

## 2. Markdown UI

Each report renders, in order:

1. **Header** — task id, state, visit and attempt, agent target and model,
   session id, start time, duration, and exit code, all read from the log's
   header and exit footer, plus a relative link to the source log.
2. **Metrics strip** — only when the workspace declares metrics and this
   session belongs to a recorded iteration: before/after value, delta, the
   sessions sharing the window, and the artifact the value was read from.
   Content and rules are owned by [§FS-rhei-metrics.4](rhei-metrics.spec.md#4-presentation).
3. **Prompt** — the first user message of the stream, verbatim and complete,
   inside a collapsed block. The prompt is never truncated.
4. **Agent actions** — the session's assistant events in stream order:
   - thinking blocks, collapsed;
   - text blocks, inline;
   - tool calls as `**{tool}** {argument summary}` with the matched execution
     result in a collapsed output block, errors marked. The argument summary
     prefers the argument that identifies the call (command, path, pattern,
     URL) over dumping the full argument object.
5. **Files produced** — every path the session wrote through its editing tools,
   grouped by path, with the operation sequence and the content: full final
   content for whole-file writes, old/new pairs for edits. This section is
   derived from tool-call arguments in the log, never from the current
   filesystem — it shows what the session did, not what later sessions left
   behind.
6. **Outcome** — the final assistant message, the reported token usage, and the
   stop reason.

## 3. Truncation

Tool outputs dominate log size and are mostly noise to a reviewing human. Each
tool output block is truncated at a configurable limit, default 10 KiB, with a
trailing note naming the source log and the line where the full output starts.
The prompt, files-produced content, and the final message are never truncated:
those are the sections a reviewer reads in full.

`rhei report --full` renders without truncation. The limit is a rendering
parameter only — it never changes what the log captured.

## 4. Rendering Triggers

Reports render in two ways, producing byte-identical output for the same log:

1. **Automatically**, when an agent session ends. Rendering failures are
   reported and never fail the transition or the run: the log is the record,
   the report is a view of it.
2. **On demand**, via `rhei report [task] [--state <state>]`, which (re)renders
   the matching sessions of a workspace — including runs that predate this
   feature, since everything the renderer needs is in the logs and the recorded
   runtime artifacts.

## 5. Fidelity Rules

The report never invents, softens, or omits what the log records:

- A log without an exit footer renders with an explicit "session ended without
  an exit record" marker instead of being skipped or given an inferred exit.
- A tool call with no recorded execution result renders as such.
- Failed sessions render like successful ones; a session that ended in error is
  exactly the session a reviewer most needs to read.
- The renderer reads the session log and recorded runtime facts only. It never
  re-executes anything, never reads mutable workspace files to fill gaps, and
  never reorders events.

## 6. Stream Extractors

The log body is the agent CLI's native event stream, so rendering is
per-extractor, mirroring how accounting already reads these streams
([§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)). The first supported extractor is the Pi session
stream. Claude and Codex stream renderers are additive follow-ups behind the
same extractor boundary; a log whose stream has no renderer yet is reported as
unsupported, not rendered wrongly.

## 7. Non-Goals

- No HTML or dashboard surface; the report is plain Markdown.
- Not a replacement for [§FS-rhei-run-report](rhei-run-report.spec.md#fs-rhei-run-report-per-run-report) or [§FS-rhei-summary](rhei-summary.spec.md#fs-rhei-summary-rhei-summary); those
  aggregate across sessions.
- No new capture: the renderer adds no fields to the log format and depends on
  none beyond what [§FS-rhei-agents.8.2](rhei-agents.spec.md#82-log-format) already specifies.
