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
pasted into an issue. For the same reason a path the agent spelled absolutely
is shown relative to the directory the session worked in, as the log header
records it (the worktree when there was one, the checkout otherwise); a path
outside that directory is shown as written.

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
([§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)). The supported extractors are the Pi session
stream, the Claude Code `stream-json` stream, and the Codex `--json` thread
stream. A log whose JSON event stream matches none of them is reported as
unsupported, not rendered wrongly.

### 6.1 Stream Detection

The extractor is chosen from the stream first — marker event types that are
disjoint across the three streams: Pi's `session`/`agent_start`, Claude
Code's `system` init event, and Codex's `thread.`/`turn.`/`item.` events.
Each body is folded through the detected stream's collectors alone, so one
stream's generic event names never leak into another's report. A body that
is mostly JSON events but carries no marker is read as the agent the log
header records; when even that reading collects nothing, the log is reported
as unsupported, never rendered as a confidently empty report. A body that is
mostly prose is plain output ([§6.4](#64-logs-without-an-event-stream)), even
when it quotes the odd JSON line.

### 6.2 Claude Code Stream

Every Claude Code launch requests the `stream-json` event stream
([§FS-rhei-cost-accounting.4](rhei-cost-accounting.spec.md#4-extraction-flow)), so its session log carries this stream.
Assistant messages contribute thinking, text, and `tool_use` events; `user`
events carry the matched `tool_result` payloads, errors marked. The token
usage of the last assistant message that reports any becomes the report's
usage; a `result` envelope kept as JSON supersedes it with the session
totals. Two capture facts shape the rendering:

- The final `result` envelope is logged as its extracted result text, not as
  JSON ([§FS-rhei-cost-accounting.4](rhei-cost-accounting.spec.md#4-extraction-flow)), so the final message is the last
  assistant text event of the stream. An envelope a capture kept as JSON
  adds its text only when the transcript does not already end with it — a
  conclusion that lives only in the envelope is never dropped.
- The stream does not echo the delivered prompt unless a `user` event
  carries plain text without tool results — text riding along a tool result
  is injected context, not the prompt. A session without an echoed prompt
  renders the explicit no-prompt marker rather than an inferred one.

### 6.3 Codex Stream

Completed thread items contribute the events: `agent_message` as text,
`reasoning` as thinking, and `command_execution`, `file_change`,
`mcp_tool_call`, and `web_search` as tool calls. A Codex item carries its own
execution result, so the call and its output arrive as one event; `file_change`
items name the changed paths and the change kind but no content, and the
files-produced section says exactly that — written paths only, a deletion
stays in the actions timeline. `turn.completed` usage becomes the report's
usage, last turn wins. Started and updated item events are deltas and are
skipped; the stream does not echo the delivered prompt.

### 6.4 Logs Without an Event Stream

A body with no event stream is not an unknown stream: it is the agent's
plain output, exactly as captured — an ordinary Claude Code session whose
result was logged as text, or a historical log written before structured
output. Such a body renders verbatim as a "Session output" section, truncated
like a tool output ([§3](#3-truncation)), instead of an empty report. Body
text that quotes rhei's own log markers stays body text: the exit footer is
the log's last well-formed footer block, and output a still-running
descendant appended after it is body again, in log order.

## 7. Non-Goals

- No HTML or dashboard surface; the report is plain Markdown.
- Not a replacement for [§FS-rhei-run-report](rhei-run-report.spec.md#fs-rhei-run-report-per-run-report) or [§FS-rhei-summary](rhei-summary.spec.md#fs-rhei-summary-rhei-summary); those
  aggregate across sessions.
- No new capture: the renderer adds no fields to the log format and depends on
  none beyond what [§FS-rhei-agents.8.2](rhei-agents.spec.md#82-log-format) already specifies.
