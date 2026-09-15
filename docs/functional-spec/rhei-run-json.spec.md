# FS-rhei-run-json: `rhei run --json`

The machine-readable form of a run. `--json` selects a third `rhei run`
frontend beside the TUI and plain stdout ([§FS-rhei-run-tui.1.4](rhei-run-tui.spec.md#14-frontend-selection)): every engine
event is written to stdout as one JSON object per line, in the order the engine
emitted it, and **nothing else is ever written to stdout**. A tool that reads
`rhei run --json` needs no screen-scraping and no knowledge of the terminal
surface. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

The same records are what `rhei run` writes durably to `runtime/events.jsonl`
(§3) and what `rhei attach --json` replays from a detached run
([§FS-rhei-run-headless.5.3](rhei-run-headless.spec.md#53---json)), so one contract covers live output, the durable log,
and attachment.

## 1. Selecting the Format

| Flag | Effect |
|------|--------|
| `--json` | JSONL event stream on stdout; implies `--no-tui` |

`--json` conflicts with `--tui`: a run cannot both own the terminal and be a
clean byte stream. `--json --dry-run` emits the dry-run preview as JSON
(§4). Human-oriented engine prose that the plain frontend prints is carried in
`message` records rather than dropped, so a JSON consumer sees the same
diagnostics an operator does. That includes every warning from the initial
validation report: each follows `run_started`, precedes scheduling, and appears
once as a `message` record with `level: "warn"`, while stderr remains reserved
for failures that do not enter the stream (§FS-rhei-run.3).

**Errors do not enter the stream.** A run that fails before or during
execution writes the `{ "error": { "message", "help" } }` envelope of
[§FS-rhei-errors.5](rhei-errors.spec.md#5-machine-readable-errors) to **stderr** and exits non-zero. Stdout stays a pure
sequence of event records to its last byte, so a consumer may parse it
line-by-line without a mode switch.

## 2. Record Envelope

Every line is a JSON object carrying at least:

| Field | Type | Description |
|-------|------|-------------|
| `seq` | integer | 1-based, gap-free, monotonically increasing within one run. **Structural records only** |
| `ts` | string | UTC RFC 3339, second precision, when the engine emitted the record |
| `event` | string | The record kind (§2.1) |

`seq` is the cursor: `rhei attach --json --since <seq>` resumes exactly after
it, and a consumer that reconnects never has to deduplicate by content. It
restarts at `1` for every run, because `runtime/events.jsonl` is truncated at
run start — one file is one run.

**`agent_output` carries no `seq`** (§2.3). It is not a cursor point: it never
reaches `runtime/events.jsonl`, so numbering it would give the stdout stream and
the durable log two different sequences for the same run, and `--since` on one
would silently skip records of the other. Leaving it out is what keeps the
structural numbering identical and gap-free in both. `agent_output` is ordered
by its position in the stream alone — `ts` is second-precision and is not a
usable tiebreaker.

A record's `ts` is the instant the *run* emitted it. A replay
(`rhei attach --json`) re-emits it unchanged; it is not restamped with the
replay instant.

Unknown fields may be **added** to any record in a future version. A consumer
must ignore fields it does not know and must not assume field order. Removing
or repurposing a field named here is a breaking change and moves `schema`
(§2.2).

### 2.1. Records

| `event` | Emitted when | Payload beyond the envelope |
|---------|--------------|------------------------------|
| `run_started` | Once, at the head of the stream | `schema`, `run_id`, `workspace`, `parallel`, `total_tasks` |
| `pass_started` | Each scheduler pass begins | `pass`, `ready` (task ids in source order) |
| `slot_assigned` | A worker is spawned | `slot`, `task`, `from`, `to`, `agent` (null for programs), `log_path` |
| `slot_released` | That worker exits | `slot`, `task`, `from`, `to`, `log_path`, `outcome`, `exit_code`, `duration_ms` |
| `pass_ended` | Each scheduler pass ends | `pass`, `progressed` |
| `tasks_deferred` | Ready tasks yielded a same-state slot | `pass`, `tasks` |
| `task_outputs_missing` | A worker exited `0` without its required artifacts | `task`, `state`, `entries` |
| `usage_reported` | A turn was measured, and again once the accounting record was durably written | `task`, `invocation_id`, `slot`, `report` (`streamed`/`final`), `usage` |
| `message` | Engine diagnostics | `level` (`info`/`warn`/`error`), `text` |
| `link` | The run produced a URL or file link | `label`, `url` |
| `agent_output` | A live agent output line (§2.3) | `slot`, `task`, `stream`, `line` |
| `run_finished` | Once, when the run loop ends (§2.4) | `summary` |

`outcome` is one of `completed`, `failed`, `waiting`, `cancelled`, `timeout`,
`interrupted`, matching the journal vocabulary of [§FS-rhei-run-tui.1.7](rhei-run-tui.spec.md#17-journal-format).
`waiting` is emitted when the released invocation selected a poll state's
self-loop, whatever its exit code — a handled wait, not a failure and not a
finished state ([§FS-rhei-states.2.2](rhei-states.spec.md#22-semantics)). Paths
are workspace-relative when inside the workspace and absolute otherwise, as in
the journal.

`usage_reported.report` is `streamed` for a running total observed
mid-invocation and `final` for the one report that follows the durable record;
exactly one `final` record exists per invocation that reaches a record
[§FS-rhei-cost-accounting.7.1](rhei-cost-accounting.spec.md#71-line-oriented-frontends). Adding the field does not move the schema version
(§2.2), and a record written before it decodes as `final`.

A stream that ends without `run_finished` says the run did not reach its own
end: it was interrupted, it failed, or the process died. That is information,
not corruption — the exit code and the stderr envelope say which. Which is why a
*reader* that stops early says so the same way: `rhei attach --json` giving up on
a run whose liveness it cannot check ([§FS-rhei-run-headless.3](rhei-run-headless.spec.md#3-run-identity-and-liveness)) exits non-zero
with the envelope on stderr, rather than ending a partial stream at `0` and
letting it read as a run that was interrupted.

### 2.2. Schema Version

`run_started.schema` is an integer, currently `1`. It increases only when a
field named in §2.1 is removed or changes meaning. Adding a record kind or a
field does not move it, so a consumer pinned to `schema: 1` keeps working
across additive releases. A consumer that does not recognize the value should
say so and stop rather than guess.

Adding a **value** to an enumerated field of §2.1 does not move `schema` either
— and here the reading rule is deliberately the opposite of the one above. A
consumer that meets an `outcome` it does not know **carries it through**: prints
it, stores it, passes it on, and keeps reading. The two unknowns are not the same
kind of unknown. An unrecognized `schema` says the stream as a whole may not mean
what the reader thinks it means, so stopping is the only safe answer; an
unrecognized `outcome` is one field of one record whose every other field still
means exactly what it says, and a reader that halts on it turns an additive
release into an outage. `waiting` ([§FS-rhei-states.2.2](rhei-states.spec.md#22-semantics)) arrived this way,
under `schema: 1`.

### 2.3. Agent Output

`agent_output` is a firehose and is **excluded by default** from both the
stdout stream and `runtime/events.jsonl`. `slot_assigned.log_path` names the
per-task log, which is the complete durable transcript ([§FS-rhei-run-tui.1.2](rhei-run-tui.spec.md#12-live-agent-traffic));
a consumer that wants the traffic reads that file. `--json-agent-output` opts
into inline delivery for callers that would rather have one stream than one
stream plus N files.

This keeps the event log bounded: a long run's structural history stays small
enough to replay in full, which is what makes attachment cheap
([§FS-rhei-run-headless.5](rhei-run-headless.spec.md#5-rhei-attach)).

### 2.4. Where `run_finished` Sits

`run_finished` marks the end of the **run loop**, and a consumer may treat it
as the terminator. It is not always the literal last line: the run still writes
a closing diagnostic or two after its loop ends — the frozen dashboard's path,
for one — and those arrive as `message` records after it. No `slot_*`,
`pass_*`, or `usage_reported` record ever follows it, so a consumer that stops
reading at `run_finished` has the whole run; one that reads on gets the closing
notes as well. `rhei attach` forwards them and then ends.

## 3. Durable Event Log

Every non-dry run writes `runtime/events.jsonl` with the same records the
`--json` frontend would emit, whichever frontend is actually selected. The file
is truncated at run start, appended one line at a time, and flushed after each
line so another process can follow it while the run is live.

It is the durable, replayable form of the run's event stream and is what
`rhei attach` reads ([§FS-rhei-run-headless.5](rhei-run-headless.spec.md#5-rhei-attach)). Like the journal
([§FS-rhei-run-tui.1.3](rhei-run-tui.spec.md#13-sink-implementations)), a write failure is a warning on stderr, never an
aborted run.

`runtime/transitions.log` is unchanged and remains the fixed-column,
tail-friendly text journal. The two coexist deliberately: one is for a human
with `tail -f`, the other for a program with a JSON parser.

## 4. Dry Runs

`--json --dry-run` emits `message` records carrying the `would transition:`,
`manual-only:`, and no-work classification lines of [§FS-rhei-run.4](rhei-run.spec.md#4-dry-run), bracketed
by `run_started` and `run_finished`, and exits with the status [§FS-rhei-run.4](rhei-run.spec.md#4-dry-run)
defines. A dry run writes no `runtime/events.jsonl` and publishes no run
descriptor, because it is side-effect-free — but the frontend the caller asked
for is still the frontend it gets, so §1's "nothing else is ever written to
stdout" holds for a preview exactly as it does for a run. Initial validation
warnings use the same `message` records and startup ordering as a real run.

## 5. Exit Codes

`--json` does not change them. `rhei run --json` exits exactly as
`rhei run` does: `0` on a plan whose tasks are all terminal, non-zero when
progress halts, `128 + signal` when a signal ended it ([§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership)). The
stream and the exit code are two answers to different questions and a consumer
should read both.

## Related Specifications

- [Run Command](rhei-run.spec.md) — the execution loop, flags, and exit codes
- [Run TUI Specification](rhei-run-tui.spec.md) — the event surface and frontend selection
- [Detached Runs](rhei-run-headless.spec.md) — `--headless`, `attach`, `stop`, `runs`
- [Errors Specification](rhei-errors.spec.md) — the JSON error envelope
- [Cost Accounting](rhei-cost-accounting.spec.md) — the `usage` payload
