# FS-rhei-cost-accounting: Rhei Cost Accounting

Rhei records token usage for agent work, converts measured tokens into cost
with a reproducible price book, rolls the result up to every task node, and
shows those totals in the CLI, TUI, and browser dashboard. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals) [§FS-rhei-run](rhei-run.spec.md#fs-rhei-run-rhei-run)

For agent spawning see [Agents Specification](rhei-agents.spec.md). For run
events and dashboard transport see [Run TUI Specification](rhei-run-tui.spec.md).
For visual dashboard behavior see [Flow Visualization](rhei-viz.spec.md).

## Goals

1. Every `claude-code`, `codex`, and `pi` invocation spawned by `rhei run`
   produces either a measured usage record or an explicit failure/status
   record. [§FS-rhei-agents](rhei-agents.spec.md#fs-rhei-agents-rhei-agents-specification)
2. Every task node has derived direct and subtree token/cost totals. [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification)
3. Token measurement and price calculation are separate so old runs stay
   explainable when provider prices change.
4. Unknown, omitted, unsupported, partial, and zero-valued token dimensions are
   distinct.
5. Monitoring views show spend, token totals, cache effect, and coverage while
   work is running. [§FS-rhei-run-tui](rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal) [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization)

## Non-Goals

- Guessing billing from transcript bytes, prompt text length, or local
  tokenizers when measured usage exists.
- Enforcing budgets or stopping a run based on spend.
- Writing cost rollups into task markdown.
- Failing a task just because accounting is unsupported or extraction failed.

## 1. Mental Model

Cost accounting has three layers:

| Layer | Meaning |
| --- | --- |
| Invocation record | One durable record for one spawned agent process. This is the source of truth. |
| Price book | The versioned table used to turn measured tokens into currency. |
| Rollups | Derived task, subtree, and run totals computed from invocation records. |

The important rule is: **measure first, price second, roll up last**.

## 2. Runtime Files

An **accounting root** is `runtime/accounting/` under an execution root:

```text
runtime/accounting/
  invocations/<invocation_file_id>.json
  captures/<capture_file_id>.jsonl
  tasks/<task_file_id>.json
  summary.json
  prices.json
```

A standalone plan or workspace has one, and it is both the run root and its
rhei's. A Panta project has **several**: one per participating rhei execution
root ([§AR-rhei-panta.5](../architecture/rhei-panta.spec.md#5-execution-root-and-per-rhei-runtime)), plus the run root, and single-file rheis share
the project directory so two of those can be the same directory. A run writes
each invocation record to the accounting root of the rhei that owns the ticket
and its capture streams to the run root's, and §5.1 already requires a price
book in every participating root. Which roots a *reading* covers is the reading
command's scope ([§FS-rhei-panta.6.5](rhei-panta.spec.md#65-cost-and-summary)); nothing here says one root holds a
project's whole history.

`invocations/` is authoritative for completed agent processes. `captures/`
stores normalized per-turn usage events while an agent is running; invocation
records are produced from those capture streams when the process exits.
`tasks/` and `summary.json` are derived indexes and may be regenerated from
invocation records and the current plan tree.

`invocation_id` is the logical identity inside the JSON record. New records
identify one spawned process — one attempt — rather than only its state visit;
§3.7 defines the spelling and the compatibility identity of older records.
File names use `invocation_file_id`, which is an opaque, path-safe UUID/ULID,
encoded id, or hash. Raw `invocation_id` text must not be used as the file
name.

Task rollup JSON contains the raw `task_id`; `task_file_id` must be a path-safe,
collision-resistant encoding of that id so distinct valid task ids do not
overwrite the same derived rollup file.

## 3. Invocation Record

Each supported agent spawn writes one JSON object:

```json
{
  "schema": "rhei.accounting.invocation.v1",
  "invocation_id": "plan.1::pending::claude-code-anthropic-sonnet::visit-1::run-b3ed70::move-0::attempt-2",
  "run_id": "b3ed70",
  "task_id": "plan.1",
  "state": "pending",
  "visit": 1,
  "target_slug": "claude-code-anthropic-sonnet",
  "agent": "claude-code",
  "provider": "anthropic",
  "model": "claude-sonnet-4-6",
  "model_profile": "review-sonnet",
  "started_at": "2026-05-20T10:30:00Z",
  "ended_at": "2026-05-20T10:34:23Z",
  "duration_ms": 263000,
  "cli_session": {
    "id": "8b04b0e8-5755-4d2e-bc01-1e14c89d0084",
    "store_path": "/home/alice/.claude/projects/example/8b04b0e8-5755-4d2e-bc01-1e14c89d0084.jsonl"
  },
  "extraction_status": "measured",
  "scope": "aggregate-agent-process",
  "token_convention": "input-total-includes-cache",
  "tokens": {
    "total": { "value": 14645, "source": "agent-usage-capture" },
    "input": {
      "total": { "value": 12345, "source": "agent-usage-capture" },
      "cached_read": { "value": 9000, "source": "agent-usage-capture" },
      "cache_write": { "value": 1200, "source": "agent-usage-capture" }
    },
    "output": {
      "total": { "value": 2300, "source": "agent-usage-capture" },
      "cached_read": { "status": "unsupported" },
      "cache_write": { "status": "unsupported" }
    }
  },
  "pricing": {
    "status": "priced",
    "currency": "USD",
    "amount_micro": 48135, "priced_amount_micro": 48135,
    "price_book_id": "builtin-2026-05-20"
  }
}
```

`model_profile` is optional provenance naming the selected settings profile.
Rhei writes it for a named profile selected through CLI, state, defaults,
`all_models`, or task `**Model:**` precedence, whether or not that profile
supplies prices. Literal target selectors do not acquire a profile identity.
Older `rhei.accounting.invocation.v1` records without this field remain valid
and are read unchanged.

An invocation record may also carry `extraction_diagnostics`, an array of
distinct diagnostic strings in first-seen order. A new record whose
`extraction_status` is `extractor-failed` always carries a non-empty array.
Records written by older v1 writers may omit it; readers treat that as an empty
array, and inspection republishes such a record without adding the field.

### 3.1. Token Dimensions

| Dimension | Meaning |
| --- | --- |
| `total` | Every token the invocation processed: the agent's own aggregate count, or `input.total` plus `output.total` when it reports no aggregate. |
| `input.total` | Every input token the provider counted for the invocation, cached reads and cache writes included. |
| `input.cached_read` | The part of `input.total` that was served from cache. |
| `input.cache_write` | The part of `input.total` that was written into cache. |
| `output.total` | Every output token the provider counted for the invocation. |
| `output.cached_read` | The part of `output.total` served from cache, if reported. |
| `output.cache_write` | The part of `output.total` written to cache, if reported. |

**One convention, whatever the agent.** `input.total` is the whole and the two
cache dimensions are parts of it, never additions to it. For every measured
record a built-in extractor writes:

```text
input.cached_read <= input.total
input.cache_write <= input.total
input.cached_read + input.cache_write <= input.total
```

and the same holds for `output`. Nothing downstream branches on `agent` to know
what the numbers mean: a consumer that adds `cached_read` to `input.total` is
double-counting whichever agent wrote the record, and `cached_read /
input.total` is the cache effect Goal 5 asks monitoring to show, which is a
ratio only while the two are nested.

Providers do not agree on this. OpenAI reports a whole-prompt `input_tokens`
with `cached_input_tokens` inside it; Anthropic and Pi report an input count
that excludes their cache dimensions. The extractor converts the provider's
shape into this one at the point of extraction (§4). A
`rhei.accounting.usage.v1` capture event is Rhei's own schema, already states
its dimensions in this convention, and is taken as written.

A reader that meets parts larger than the whole subtracts saturating rather
than underflowing.

Each dimension is either measured:

```json
{ "value": 12345, "source": "agent-usage-capture" }
```

or unavailable:

```json
{ "status": "unsupported" }
```

Unavailable statuses are:

| Status | Meaning |
| --- | --- |
| `unsupported` | The agent/provider cannot report this dimension. |
| `omitted` | The agent/provider may support it, but this invocation omitted it. |
| `unknown` | Rhei tried to extract it but could not determine the value. |

Measured zero is `"value": 0`; it is not the same as unavailable.

### 3.2. Extraction Status

Every `claude-code`, `codex`, and `pi` invocation writes a record even when
tokens cannot be measured.

| `extraction_status` | Meaning |
| --- | --- |
| `measured` | At least input or output total tokens were extracted. |
| `unsupported-agent` | The agent has no accounting extractor. |
| `extractor-unavailable` | The configured extractor could not run. |
| `extractor-failed` | The extractor ran but could not parse usage data. |
| `no-usage-emitted` | The agent exited without producing supported usage data. |

Only `extractor-failed` carries `extraction_diagnostics`. New writers do not
attach extraction diagnostics to the other four statuses, whose meanings and
precedence remain unchanged.

Unsupported custom agents may omit records only when the resolved agent profile
has no accounting extractor. Built-in `claude-code`, `codex`, and `pi` must not
silently omit records.

### 3.3. Measurement Scope

`scope` says what one record covers:

| Scope | Meaning |
| --- | --- |
| `aggregate-agent-process` | Usage for the whole spawned agent process. |
| `provider-call` | Usage for one provider API call. |
| `child-invocation` | Usage for a nested agent invocation Rhei can identify. |

The v1 built-in extractors may use `aggregate-agent-process` when the agent CLI
does not expose finer-grained usage.

### 3.4. Timing and Agent CLI Session

`duration_ms` is the elapsed wall-clock time between `started_at` and
`ended_at`, rounded down to whole milliseconds. New records always carry it;
readers must accept older v1 records where it is absent, and §3.4.1 says what
they report about one.

When structured agent output exposes a native session identity, the invocation
record carries `cli_session.id`. Built-in extractors recognize Claude Code's
result `session_id`, Codex's `thread.started.thread_id`, and Pi's
`session.id`. `cli_session.store_path` is optional and is written only when
Rhei can derive the native transcript path confidently. The whole
`cli_session` field is absent when no id was exposed; neither it nor
`store_path` is serialized as `null`. Session capture is independent of the
usage-event capture lifecycle, including replacement of cumulative Claude Code
usage events.

#### 3.4.1. Deriving the Elapsed Time on Read

`duration_ms` is **optional** and stays optional (§8.1), so a record written
before the field existed carries none. That elapsed time is not unknown: it is
defined above as the difference between `started_at` and `ended_at`, and every
v1 record carries both. A reader holding the two endpoints holds the number.

| Where | Required behavior |
| --- | --- |
| Writing | Every record Rhei writes carries `duration_ms`. A record already on disk without it is never rewritten to add one (§5.1). |
| Reading | A record with no `duration_ms` parses, and the parsed record still has none. The field is not filled in on the way past. |
| Reporting | A surface that reports an invocation's elapsed time derives it from `ended_at - started_at` when the stored field is absent, and reports nothing only when an endpoint is missing or unparseable, or when the two do not form a forward interval. |

The derivation belongs where a reading is published, not on the record it read:
a reading recomputes what it reports and never rewrites what it read (§5.1),
the same way a restated token count is (§5.2).

This is already what `rhei summary` does with these records
([§FS-rhei-summary.2.2](rhei-summary.spec.md#22-the-steps)), and what the
`invocations` array of `rhei cost --json --task` publishes (§8). Reporting
nothing instead is not reporting an unknown: a consumer reading no elapsed time
counts the invocation as instantaneous, so a surface that omits it hands back a
zero that was never true.

### 3.5. Run Attribution

`run_id` names the one invocation of `rhei run` that spawned the process — the
same id the run report, the workspace descriptor, and the run registry already
use for it. [§FS-rhei-run-headless.2](rhei-run-headless.spec.md#2-the-run-descriptor)

The field is **optional**, and the schema string stays
`rhei.accounting.invocation.v1`. A record written before the field existed, or by
a build that does not set it, still parses and is still read by every consumer.
Such a record is **unattributed**.

| Where | Required behavior |
| --- | --- |
| Writing | Every record written for an agent spawned inside `rhei run` carries `run_id`. |
| Reading | A record with no `run_id` parses, remains a candidate in §3.7's legacy identity inference, and counts toward every whole-workspace total. |
| Aggregating | An unattributed record is never dropped and never folded into a named run. Selection and grouping by run give it an explicit place of its own (§6.1). |

Unattributed records are not an error condition: they are the history a
workspace held before the field existed, and an aggregate that quietly omits
them under-reports that history.

### 3.6. Token Convention of a Record

`token_convention` names the convention a record's token dimensions follow.
Rhei writes one value, `input-total-includes-cache`, which is §3.1.

The field is **optional** and the schema string stays
`rhei.accounting.invocation.v1`, exactly as `run_id` is (§3.5). A record written
before the field existed still parses, is never dropped, and is never read at
face value.

| Where | Required behavior |
| --- | --- |
| Writing | Every record Rhei writes carries `token_convention`, and its dimensions satisfy §3.1. |
| Reading | A record carrying the field is read under it. A record without it is read under the convention its own `agent` implies, below. |
| Rewriting | Never. A stored record's tokens and amounts are what was computed when it was written and stay as written (§5.1). What the inference changes is what a recomputation over stored records produces (§5.2). |

The convention an `agent` implies, and the evidence for it:

| `agent` | Implied convention | Evidence |
| --- | --- | --- |
| `codex` | `input-total-includes-cache` | OpenAI's `input_tokens` is the whole prompt, with `cached_input_tokens` a subset of it. |
| `claude-code` | `input-total-excludes-cache` | Anthropic reports `input_tokens` disjoint from `cache_read_input_tokens` and `cache_creation_input_tokens`. |
| `pi` | `input-total-excludes-cache` | Pi's own aggregate says so: its `totalTokens` equals `input + cacheRead + cacheWrite + output`. |

An agent this table does not name has no known convention. Its record is read
as stored, because nothing about it is known to be wrong, and no aggregate
holding it reports `complete` (§6.2).

### 3.7. Attempt Identity

One `invocation_id` identifies one spawned agent process. Before either the
sequential or parallel scheduler spawns it, Rhei creates the id once from the
shared persisted spawn plan and carries that value through every streamed
usage report, the final report, and the durable invocation record. Reports
from one process therefore have one stable id; a retry, a later `rhei run`, or
a later entry into the same state has a different id.

New ids retain the readable `<task>::<state>::<target>::visit-<visit>` prefix
and append `run-<run-id>::move-<moves>::attempt-<attempt>`. `run-id` is the
current run's id, `moves` is the spawn plan's persisted visit key, and
`attempt` is its one-based attempt number. Each component is encoded so the
whole value remains an opaque string to consumers. The same shared spawn plan
supplies sequential spawns, parallel spawns, attempt-budget checks, retry
narration, and log naming; this identity rule does not introduce another
counter or change attempt budgets, interruption charging, log names, or reset
behavior.

A failed best-effort publication of `runtime/run.json` does not change the
current run's in-memory identity or prevent agent execution. Both schedulers
use that same identity for accounting even when no descriptor was published.

A v1 id without those suffix components is a **legacy id**. Readers infer a
legacy attempt identity from:

1. `invocation_id` and the record's task, state, visit, and target facts;
2. `run_id`, when present; and
3. the `started_at`/`ended_at` interval.

Exact record copies have one identity and count once, including when copied
between accounting roots. Records with the same legacy id are separate
attempts when their base facts agree and either their run ids differ or their
well-formed intervals distinguish them. A missing `run_id` leaves a record
unattributed under §3.5; it does not remove that record from whole-workspace
history. Equal identity evidence with unequal record contents, timing that is
missing or malformed and leaves identity ambiguous, or disagreement in the
base facts is an identity conflict under §11. Readers do not rewrite or
migrate any of these records.

## 4. Extraction Flow

Accounting is separate from snapshots. Snapshot support may provide a useful
transcript source, but a missing snapshot `session` profile must not disable
accounting for `claude-code`, `codex`, or `pi`. [§FS-rhei-snapshots](rhei-snapshots.spec.md#fs-rhei-snapshots-rhei-session-snapshots-specification)

For each agent invocation:

1. Before spawn, Rhei creates the attempt identity under §3.7 and the
   extractor declares any extra arguments, environment variables, or capture
   paths needed for structured usage. Rhei's built-in capture contract sets
   `RHEI_ACCOUNTING_USAGE_PATH` and
   `RHEI_ACCOUNTING_USAGE_SCHEMA=rhei.accounting.usage.v1`.
2. `rhei run` spawns the agent normally.
3. The extractor observes structured usage as it is produced and appends
   normalized usage events to `runtime/accounting/captures/*.jsonl`.
4. The agent exits and Rhei drains stdout/stderr.
5. Rhei evaluates completion and selects the outgoing transition.
6. Rhei sums the capture stream and writes the invocation record.
7. Rhei emits `UsageReported`.
8. Rhei applies normal snapshot side effects and task transition behavior.

Extraction failures affect accounting coverage only. They do not change the
agent exit code, completion condition, selected transition, or callbacks.

Built-in extractor requirements:

| Agent | Requirement |
| --- | --- |
| `claude-code` | For an ordinary one-shot launch, request Claude Code's typed `json` result output (`--output-format json`). Accept only a result envelope with `type: "result"`, a textual `result`, and a complete typed `usage` or `modelUsage` object containing input, cache-read, cache-write, and output token fields; normalize those dimensions. The envelope's `result` text is the human-readable agent output. When `intervene_stdin` selects the existing stream-json transport, retain that transport and apply the same result-envelope usage extraction to its final result event. |
| `codex` | Run `codex exec --json`; extract `turn.completed.usage` from JSONL stdout and normalize it into `runtime/accounting/captures/*.jsonl`. Do not depend on Codex snapshot support. |
| `pi` | Run `pi --mode json`; extract each assistant `message_end.message.usage` event and normalize it into `runtime/accounting/captures/*.jsonl`. Ignore the duplicate message usage carried by `turn_end` and `agent_end`. Do not depend on Pi snapshot session data. |

Each built-in extractor converts its provider's shape into §3.1's convention as
it normalizes, so the capture stream and the record are already in it. `codex`
reports the inclusive figure and is copied through; `claude-code` and `pi`
report an input count that excludes their cache dimensions, and those
dimensions are added into `input.total`. The conversion belongs to the
provider-native shapes only: a `rhei.accounting.usage.v1` capture event an agent
writes through the capture contract is already in Rhei's convention and is not
converted again.

If an upstream CLI changes format, the extractor records `extractor-failed`
with a concise diagnostic. The failure variant of a
`rhei.accounting.usage.v1` event carries that reason in the optional string
field `diagnostic`. Structured extractors generate stable reasons that are a
single line of at most 240 characters and do not quote raw agent output. For a
Claude result envelope that has `type: "result"` and usage but no textual
`result`, the reason is exactly `claude-code result envelope is missing
required \`result\` text`.

Capture aggregation copies failure reasons into the durable invocation's
`extraction_diagnostics`. It retains distinct reasons in first-seen order and
does not let a measured event override any retained failure. When an older
failure event has no diagnostic, a new invocation writer uses exactly `usage
capture reported extractor-failed without a diagnostic`. Claude cumulative
result replacement is unchanged: a later valid cumulative result replaces the
earlier capture content, including its failures, and produces a measured
record.

Diagnostics come only from structured parsing. Rhei must not guess from nearby
human-readable text, quote the rejected output, or parse arbitrary agent
stdout/stderr JSON as billing telemetry; it only accepts structured capture
events that identify the accounting schema.

## 5. Pricing

`runtime/accounting/prices.json` records the price book used for a run:

```json
{
  "schema": "rhei.accounting.prices.v1",
  "price_book_id": "builtin-2026-05-20",
  "currency": "USD",
  "provenance": {
    "publisher": "Anthropic",
    "retrieved_at": "2026-05-20T00:00:00Z"
  },
  "entries": [
    {
      "provider": "anthropic",
      "model": "claude-sonnet-4-6",
      "effective_at": "2026-05-20T00:00:00Z",
      "note": "Standard service tier, up to 200k context",
      "unit": "1m_tokens",
      "input_total_micro": 3000000,
      "input_cached_read_micro": 300000,
      "input_cache_write_micro": 3750000,
      "output_total_micro": 15000000
    }
  ]
}
```

### 5.1. Price-Book Selection

`rhei run ... --prices <PATH>` selects a caller-owned local price book for
that run and takes precedence over profile-authored and built-in prices. Rhei
reads and validates the file before starting any agent. The
book must use the `rhei.accounting.prices.v1` schema shown above, provide a
non-empty `price_book_id` and currency, use `1m_tokens` for every entry, and
provide non-empty provider, model, and effective timestamp values. Duplicate
provider/model entries are rejected because pricing uses one exact match.
Missing, unreadable, malformed, wrong-schema, and unsupported books fail the
run with a diagnostic that names the supplied path. Selection never fetches a
book over the network.

The document object and each entry object accept arbitrary additional JSON
properties as metadata. Rhei preserves every additional property's JSON value
in each run-owned `runtime/accounting/prices.json` copy, including the run root
and every participating rhei execution root. Metadata does not participate in
known-field validation, provider/model matching, coverage, or cost calculation;
it cannot supply, replace, or override a required known field. Rhei preserves
JSON values, not the source file's whitespace or object-key order.

The selected in-memory book is shared by sequential and parallel agent
execution. Before any agent starts, Rhei atomically copies a caller-owned book
to `runtime/accounting/prices.json` in the run root and every participating
rhei execution root. Invocation pricing records the selected book's id and
currency. An explicit book bypasses profile-book construction and its conflict
checks while retaining its existing validation and exact-match behavior.

Without `--prices`, Rhei preflights every candidate invocation in the selected
run scope. Candidates include each selected task and its descendants, in their
current state and in nonterminal states reachable through declared transitions
under that task's node profile. Program and gating states contribute no agent
invocation, but their outgoing paths can reach later agent work in the same run.
Terminal states stop the walk. Readiness, transition conditions, and subprocess
outcomes can change during execution, so current readiness or one predicted
outcome must not exclude a later candidate. Unreachable states and tasks outside
the selected rhei scope contribute nothing. Each candidate uses the same
task/state/CLI selector precedence as execution. All these sources participate
in the single pre-execution composition and conflict check, before even the
first program starts.

A named profile contributes its profile entry when it has prices;
otherwise that invocation contributes the built-in exact match or an unpriced
marker. A literal target contributes the same built-in-or-unpriced fallback and
never a profile entry. Profile prices apply only when the invocation's final,
case-sensitive provider/model pair exactly equals the pair declared by that
profile; a mismatch is a pre-spawn error naming the profile and both pairs.

If no candidate contributes profile prices, Rhei uses the built-in book
unchanged, including its existing id and durable-copy behavior. Otherwise Rhei
composes one generated book for sequential and parallel execution:

- distinct provider/model pairs contribute distinct entries;
- identical profile entries for one pair deduplicate and retain every
  contributing profile id;
- a profile entry and a built-in or unpriced fallback for the same pair
  conflict, as do profile entries whose rates, effective timestamp, metadata,
  or source kind differ; and
- priced entries with different currencies conflict.

Every conflict is refused before any frontend, callback, nested run, or agent
starts. Its diagnostic names the pair and participating profile or literal
sources. Unpriced markers take part in conflict detection but are not written
as entries. Thus a selected profile's tier cannot price a literal invocation or
another profile merely because their exact pairs coincide.

Each generated entry carries `source` metadata. A profile entry uses
`{"kind":"profile"}` and a sorted, deduplicated `model_profiles` string array;
a fallback entry uses
`{"kind":"builtin","price_book_id":"<built-in-id>"}` and no
`model_profiles`. Entry order is ascending by provider then model. All object
keys are recursively sorted for identity; arrays keep semantic order except
that entries and `model_profiles` use the sorting just stated. The compact
UTF-8 JSON encoding of the generated book's `schema`, `currency`, and entries,
excluding `price_book_id`, is its canonical semantic content. Its id is
`profiles-sha256-` followed by the lowercase hexadecimal SHA-256 digest of
that content. Metadata and source provenance therefore participate in the id;
source-file whitespace and object-key order do not.

Before any agent starts, Rhei atomically writes the generated book to
`runtime/accounting/prices.json` and to the immutable
`runtime/accounting/price-books/<price_book_id>.json` in the run root and every
participating rhei execution root. An existing archive with identical bytes is
reused; an existing path with different bytes is an error. No root is changed
unless durable identity and currency validation succeeds for every
participating root. Explicit caller-owned books continue to be copied only to
`prices.json`; this contract does not archive them.

Every currency-bearing durable invocation record in a participating accounting
root, including an unpriced record, must use the selected book's currency.
While holding all participating run locks, Rhei checks every root before it
writes any selected book or starts a frontend, agent, callback, or nested run.
If a record's currency differs, the run fails with the accounting root and both
currencies in the diagnostic, and no participating root is changed. This check
applies to caller-owned and built-in selection. Existing invocation records
remain authoritative: Rhei neither reprices nor rewrites them, converts their
amounts, or replaces their currency.

Price entries match provider and model exactly. A selected book with no exact
entry leaves the measured invocation explicitly unpriced; it never implies a
zero price or falls back to the built-in book. The selection applies only to
invocations recorded by that run: no older record's stored amount, currency, or
price-book id is rewritten by it.

Rules:

- Prices are integer micro-units of the configured currency.
- Rhei must not use floating-point arithmetic for cost calculation.
- One price book has exactly one currency. Mixed-currency price books are
  rejected.
- Every priced invocation in one run uses the same price-book currency.

Cost formula:

```text
uncached_input = input.total - input.cached_read - input.cache_write

cost = ( uncached_input     * input_total_micro
       + input.cached_read  * input_cached_read_micro
       + input.cache_write  * input_cache_write_micro
       + output.total       * output_total_micro ) / unit_tokens
```

The full input rate applies to the remainder, not to `input.total`, because the
two cache dimensions are parts of `input.total` (§3.1) and each already carries
its own rate. Charging `input.total` at the full rate *and* the cache
dimensions at theirs charges every cached token twice. An unavailable dimension
contributes nothing and subtracts nothing, and the subtraction saturates at
zero rather than underflowing.

A `codex` invocation of 1,000 input tokens, 700 of them cached reads, and 50
output tokens, on a book charging $4/M input, $0.40/M cached read and $20/M
output, costs **2,480** micro-USD: 300 fresh input at the full rate, 700 cached
reads at the cache rate, 50 output. Charging all 1,000 at the full rate and the
700 again at the cache rate gives 5,280.

Pricing status:

| `pricing.status` | Meaning |
| --- | --- |
| `priced` | Every measured billable dimension had a price. |
| `partial-price` | Some measured billable dimensions had prices and some did not. |
| `unpriced` | Tokens were measured, but none of the measured billable dimensions had prices. |
| `not-applicable` | No measured tokens were available to price. |

Missing prices must not be treated as zero-cost. A `priced` result always
writes equal `amount_micro` and `priced_amount_micro` values. A
`partial-price` result may write `priced_amount_micro` as a lower-bound amount,
but never writes `amount_micro`. `amount_micro` is written only when status is
`priced`.

### 5.2. Recomputing a Stored Record

Every rollup, report, and inspection surface computes from stored records (§6),
so a record written before §3.1 was stated has to be read into §3.1's
convention on the way out. Nothing on disk changes: `tokens`, `amount_micro`,
and `priced_amount_micro` are the record of what was computed and stay as
written (§5.1). What changes is what a recomputation over them produces.

Take the record's convention from §3.6, then:

| The record's convention | Its tokens | Its money |
| --- | --- | --- |
| `input-total-includes-cache` | Already §3.1. Read as stored. | Must be recomputed. The stored amount charged `input.total` at the full input rate *and* the cache dimensions at theirs, so it over-charges every cached token by the full input rate. |
| `input-total-excludes-cache` | `input.total` becomes `input.total + cached_read + cache_write`; `total` becomes the restated `input.total + output.total`. | Read as stored. It is already right: the dimensions were disjoint and were priced as disjoint, which is exactly what §5's formula computes on the restated tokens. |

Restating tokens needs no price book. Recomputing money needs one, and only one
number out of it: the full input rate for the record's provider and model. A
record's price book is **reachable** when its `price_book_id` is the built-in
book's, the id of the `prices.json` beside it in the accounting root it was
read from, or the id of a valid generated
`price-books/<price_book_id>.json` archive beside it. Archive lookup accepts
only a generated `profiles-sha256-…` id, requires the file's id and canonical
content digest to match the requested id, and never searches another root.
Selection never fetches a book over the network (§5.1), so any other book named
by id alone and absent from the current `prices.json` is unreachable.

A later run may replace `prices.json` and create a new generated archive after
settings change. It does not rewrite or delete older archives and does not
rewrite an older invocation's tokens, stored amount, currency, profile, or
price-book id. Archive reachability permits the existing read-time correction
below; it does not reprice history under a later rate book. Archiving explicit
books and applying a later book to completed invocations are outside this
contract.

When a record's money must be recomputed and its book is unreachable, the
record is read as `unpriced`: its measured tokens still count, and it
contributes no amount. The stored amount is not carried forward. It is known to
be an over-charge, and it cannot be reported as `priced_amount_micro` either,
which is a lower bound (§5) — an over-charge is an upper one. The aggregate
then follows the ordinary rules for a selection holding an unpriced record
(§6.2), which is how the doubt reaches a reader.

A record whose cache dimensions are zero or unavailable needs no correction at
all: the recomputation and the stored amount agree, and it stays priced whether
or not its book is reachable.

## 6. Rollups

Rollups are derived from invocation records:

```text
direct(node)         = sum(invocations where invocation.task_id == node.id)
subtree(node)        = direct(node) + sum(subtree(child) for every child node)
workspace_total      = sum(subtree(root) for every root node)
run(id)              = sum(invocations where invocation.run_id == id)
unattributed         = sum(invocations with no run_id)
window(since, until) = sum(invocations where since <= started_at < until)
```

`workspace_total` is one workspace's whole accounting history, for as long as
its artifacts have existed. Through 0.3.3 this quantity was called `run_total`,
which reads as *what one `rhei run` cost* and is not what it measures. The two
are different numbers and are named apart wherever either is shown: `run(id)` is
one invocation of `rhei run`, `workspace_total` is the workspace's lifetime.

Coverage says how complete the rollup is:

| Coverage | Meaning |
| --- | --- |
| `complete` | Every invocation was measured and fully priced. |
| `partial` | At least one invocation or dimension is missing, unsupported, unknown, or only partly priced. |
| `unpriced` | Tokens exist, but no cost could be computed. |
| `none` | No measured usage exists. |

Missing invocation records for supported built-in agents count as `partial`,
because the absence itself is a coverage defect.

Dimension summaries expose partial rollups without forcing dashboards to fetch
every invocation:

```rust
pub enum DimensionStatus {
    Measured,
    Partial,
    Unsupported,
    Omitted,
    Unknown,
}

pub struct DimensionSummary {
    pub value: Option<u64>,
    pub status: DimensionStatus,
    pub measured_count: u64,
    pub missing_count: u64,
}
```

For one invocation, `Measured` means `value` is present. For a rollup,
`Measured` means every contributing invocation reported the dimension.
`Partial` means `value` is a subtotal from measured contributions and at least
one contributing invocation was missing, unsupported, omitted, or unknown.

### 6.1. Selections

A rollup is computed over a **selection** of invocation records, and selection
happens before aggregation. Three axes select, and they compose:

| Axis | Selects |
| --- | --- |
| Run | Records whose `run_id` equals the given id. The reserved id `unattributed` selects the records that name no run (§3.5). |
| Window | Records whose `started_at` lies in the half-open interval `[since, until)`. A record with no `run_id` is selected by a window like any other. |
| Plan tree | Records whose `task_id` is a node or a descendant of it — `direct` and `subtree` above. |

A grouping partitions the selection. Grouping by run keys on `run_id` and gives
the records that name no run one explicit group of their own, keyed
`(unattributed)`. Grouping by day keys on the UTC calendar date of `started_at`.

### 6.2. Coverage of a Selection

Every aggregate carries coverage in the vocabulary above, and a selection can be
incomplete in a way no single record's status shows.

- A selection **by run** never reports `complete` while any unattributed record
  falls inside the window the run was asked for within, because one of those
  records may belong to that run: a `complete` reading becomes `partial`. A
  record the window excludes is not part of what the aggregate claims, so it
  cannot put that claim in doubt. How many records could not be attributed is
  reported beside the total whatever the coverage is.
- The `(unattributed)` group carries its own coverage, computed from its own
  records like any other group.
- A **window** is not made incomplete by unattributed records: `started_at` is
  present on every record, so the window's membership is exact. The ordinary
  measurement and pricing rules decide its coverage.
- No aggregate reports `complete` over a set it could not fully see. Where a
  retention boundary bounds what was readable, the aggregate says what it could
  not see rather than summing what is left. [§FS-rhei-run-headless.6](rhei-run-headless.spec.md#6-rhei-runs)
- An accounting root that **exists but cannot be read** is one of those
  boundaries. The reading names it and reads the rest, and no aggregate over it
  reports `complete`. A reading over several roots (§FS-rhei-panta.6.5) is
  where this bites: with one root an unreadable directory printed
  `(no accounting records found)` and the emptiness was the report, while with
  several it would silently shrink a total that still claimed to be whole. A
  malformed *record* is not this case — it is reported and skipped without
  demoting anything (§11), because the records around it were read.
- A record read under an inferred convention (§3.6) is not itself a doubt: the
  inference is exact for the three built-in agents. A record from an agent that
  table does not name is, and no aggregate holding one reports `complete`. A
  record whose money could not be recomputed (§5.2) is unpriced, and demotes the
  aggregate the way any unpriced record does.

## 7. Run Events

After the invocation record is written, Rhei emits:

```rust
pub struct UsageSummary {
    pub invocation_id: String,
    pub state: String,
    pub agent: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_total: DimensionSummary,
    pub input_cached_read: DimensionSummary,
    pub input_cache_write: DimensionSummary,
    pub output_total: DimensionSummary,
    pub output_cached_read: DimensionSummary,
    pub output_cache_write: DimensionSummary,
    pub cost_micro: Option<u64>,
    pub priced_cost_micro: Option<u64>,
    pub currency: Option<String>,
    pub coverage: UsageCoverage,
    pub status: UsageStatus,
    pub pricing_status: PricingStatus,
}

pub enum UsageReport {
    Streamed,
    Final,
}

pub enum RunEvent {
    UsageReported {
        slot: Option<Slot>,
        task: String,
        invocation_id: String,
        report: UsageReport,
        usage: UsageSummary,
    }
}
```

`UsageReported` may arrive repeatedly for the same attempt id as a streaming
extractor observes additional turns, and may also arrive after `SlotReleased`;
frontends must upsert by invocation id and update task, slot history, and run
totals without assuming the slot is still active. Reports for another attempt
have another id and contribute a separate row and amount to task and run
totals. [§FS-rhei-run-tui](rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal)

`RunSummary.accounting` contains an optional `AccountingRunSummary` with the
same dimension, cost, currency, coverage, and pricing-status shape as
`UsageSummary`. It is `None` when the run did not enter agent mode or no
accounting records were produced.

### 7.1. Line-Oriented Frontends

One attempt may emit several `UsageReported` events, and `report` is what tells
them apart. Every report for that attempt carries the id §3.7 assigned before
spawn, including the final report derived from the durable record.

A **streamed** report is emitted while the agent is still running, once for each
turn a streaming extractor recognizes. Each one is re-summed from the whole
capture stream, so it is a running total for that invocation rather than one
turn's slice: it rises toward the final figure and reads lower than the
invocation cost until the last turn is in.

A **final** report is emitted once, after the invocation's
`runtime/accounting/invocations/` record is durably written, and carries what
that record says the invocation cost. Exactly one final report exists for each
invocation that reaches a durable record, and an invocation that ends before its
record is written emits none.

A frontend that keys a view by invocation id upserts both kinds, as §7 requires,
and so shows spend rising while work is running.

A frontend that **appends lines** instead cannot upsert, because a line once
written cannot be revised. Such a frontend:

- writes **at most one** accounting line per invocation;
- writes it on the **final** report only, ignoring every streamed report;
- carries that final report's value on the line, never a running total;
- writes **no** line for an invocation with no priced cost, and none for an
  invocation that never reaches a durable record.

The clauses above bind the frontend a run is read through. `StdoutSink` is that
frontend for `--no-tui`, a non-TTY stdout, and the `runtime/run.log` of a
headless run [§FS-rhei-run-tui.1.3](rhei-run-tui.spec.md#13-sink-implementations), so totalling the accounting lines of a
run gives the same figure as `runtime/run-report.md` and `rhei cost`, and each
figure is an invocation's own cost rather than a moment's reading of it. The
transition journal is not one of those frontends: it keeps one entry per run
event in every mode [§FS-rhei-run-tui.1.7](rhei-run-tui.spec.md#17-journal-format), so it appends a line per report and
still carries running totals. Its lines name their `invocation=`, which an
accounting line does not, so a reader can tell them apart rather than sum them.

This is a deliberate trade against goal 5 above, and only on this surface: a
reader tailing a line-oriented run no longer watches a figure rise. The TUI cost
view, the browser dashboard, and `rhei run --json` receive every report and keep
showing spend while work is running.

## 8. CLI Inspection

`rhei cost` reads accounting artifacts without changing the plan:

```bash
rhei cost <RHEI_PLAN_OR_WORKSPACE> [--task <ID>] [--json] [--rhei <ID>]
          [--run <ID>] [--since <TIME>] [--until <TIME>]
          [--by agent|model|state|node|run|day]
```

`--rhei <ID>` (repeatable) narrows the reading to named rheis, exactly as
`rhei list`'s does. Which accounting roots the positional and the flag between
them select is [§FS-rhei-panta.6.5](rhei-panta.spec.md#65-cost-and-summary); everything below computes over the records
those roots hold, as one set.

Default text output shows workspace totals, coverage, and highest-cost nodes by
subtree cost. `--task <ID>` shows that node's direct and subtree totals plus
the contributing invocation records. `--json` emits the same data with stable
field names matching the runtime artifact schema.

Task-detail JSON includes the `invocations` array and republishes each stored
invocation's `extraction_diagnostics` when present. In task-detail text, every
diagnostic of a contributing failed invocation appears directly below that
invocation, in array order:

```text
    <invocation-id> <agent> <model> unpriced
      extractor-failed: <reason>
```

A historical failed invocation without the array keeps the existing single
invocation line and gains no diagnostic line.

The two halves of a `--task` payload are not read the same way. Its
`invocations` are the durable records themselves, emitted as stored and never
rewritten (§5.1), while the `direct` and `subtree` rollups beside them have
already been read under §3.6's inference. On a record that carries no
`token_convention` the two therefore disagree, and the payload names the
difference only by that field's absence. A consumer summing `invocations` to
compare the sum against a rollup must apply the inference table first.

When no accounting artifacts exist, `rhei cost` exits 0 and prints the line
below, **and names the roots it searched underneath it**:

```text
(no accounting records found)
searched <root>
```

```text
(no accounting records found)
searched 3 accounting roots under <project>:
  <root>
  <root>
  <root>
```

The first line is unchanged and carries the answer; the tail says where the
answer came from. An empty reading over a project that holds thousands of
records reads as "this run cost nothing" unless it says which directories it
looked in, and that is the one thing the reader needs in order to tell a zero
from a miss. One root is named inline; several are counted, then listed one per
line, indented two spaces, in the order the roots were enumerated
([§FS-rhei-panta.6.5](rhei-panta.spec.md#65-cost-and-summary)). Roots print as the command resolved them, the same path
text the malformed-artifact warnings use (§11). A reading that found records
prints no such tail — `--json` carries the roots either way (§8.4).

`rhei summary` reads the same artifacts for the other question — not what the
run cost but what it did, one numbered line per invocation, as Markdown short
enough to paste into a pull request ([§FS-rhei-summary](rhei-summary.spec.md#fs-rhei-summary-rhei-summary)).

### 8.1. Published Accounting Schemas

Rhei publishes one JSON Schema for every accounting schema id it writes:

- `rhei.accounting.invocation.v1`
- `rhei.accounting.summary.v1`
- `rhei.accounting.usage.v1`
- `rhei.accounting.cost.v1`
- `rhei.accounting.task.v1`
- `rhei.accounting.prices.v1`

The versioned source files live in `crates/rhei-cli/schemas/` and are embedded
in the binary. `rhei schema <schema-id>` writes the embedded file bytes to
stdout. `rhei schema` and `rhei schema --list` list every published id, one per
line. An unknown id exits nonzero and names both the unknown id and the listing
command.

Published v1 schemas permit additive evolution: fields may be added within v1,
and consumers must tolerate unknown fields at every object extension point. A
removal, rename, type change, or semantic change to an existing field requires
a new schema id. Fields documented as optional, including `duration_ms`,
`cli_session`, `run_id`, and `token_convention`, remain optional so artifacts
from older Rhei versions still validate.

Within those same v1 ids, the usage schema defines the optional string
`diagnostic` on its `extractor-failed` variant. The invocation schema and the
nested invocation definition in the cost schema define optional
`extraction_diagnostics` as an array of unique, non-empty, single-line strings
of at most 240 characters. Newly written artifacts satisfy the stronger
requirements in §3 and §4; optionality in the schemas preserves historical v1
artifacts.

Attempt-scoped `invocation_id` values remain
`rhei.accounting.invocation.v1`. The field is still a string identifying one
spawned process; no field or one-process meaning changed, only the opaque value
new writers choose. New readers accept historical v1 spellings under §3.7
without migration. Schema-conforming older readers accept the new strings and,
because attempts no longer share them, count each attempt correctly; parsing
the example spelling is not a published contract.

### 8.2. Selecting Records

`--run`, `--since`, and `--until` narrow the set of records everything else is
computed from (§6.1). They compose.

| Flag | Selects |
| --- | --- |
| `--run <ID>` | Records whose `run_id` is `<ID>`. `--run unattributed` selects the records that name no run (§3.5). |
| `--since <TIME>` | Records with `started_at >= TIME`. |
| `--until <TIME>` | Records with `started_at < TIME`. |

`<TIME>` is an RFC 3339 instant (`2026-09-01T00:00:00Z`), a bare UTC date
(`2026-09-01`, meaning that date's midnight UTC), or a duration before now
(`7d`, `24h`, `90m`). An unparsable `<TIME>` is a usage error, not an empty
window: silently selecting nothing is how a caller reads zero as an answer.

A selection that matches nothing is a different answer from a workspace holding
no records at all. It exits 0 and prints:

```text
(no accounting records match the selection)
searched 3 accounting roots under <project>:
  <root>
  <root>
  <root>
```

The tail is the one §8 defines, rendered identically, so the distinction
between a workspace that holds nothing and a selection that matched nothing
stays where it already was — in the first line, and only there.

### 8.3. Grouping

`--by` takes `agent`, `model`, `state`, `node`, `run`, or `day`, and `node`
stays the default.

`--by run` keys each group on `run_id`, and emits the group keyed
`(unattributed)` whenever the selection holds a record that names no run — that
group is never omitted and its records are never folded into a named run
(§3.5). `--by day` keys each group on the UTC calendar date of `started_at`,
formatted `YYYY-MM-DD`. Every group carries its own coverage (§6.2).

### 8.4. Compatibility

`rhei cost <RHEI_PLAN_OR_WORKSPACE>` with none of the flags in §8.2 and no
`--by` prints exactly what it printed before those flags existed, with the same
exit behavior and the same `(no accounting records found)` line. The selection
surface is additive; the unselected reading does not move.

The roots tail (§8) is the one exception, and it is an addition rather than a
change: the `(no accounting records found)` line is byte for byte what it was
and stays the first line of the output, and a line naming the roots follows it.
A caller matching on that line still matches; a caller matching on the whole of
stdout sees one more line. Nothing else moves. A reading that **found** records
prints what it printed before, byte for byte, and over a standalone workspace —
whose one root is both the run root and its rhei's, so the union is a single
read — that is every non-empty reading it has.

There is one narrow exception for extraction diagnostics: `rhei cost --task`
adds the indented lines specified in §8 beneath a contributing
`extractor-failed` invocation when that record carries diagnostics. Successful
invocations and historical failed invocations without diagnostics keep their
previous text byte for byte. Default cost output and every other text surface
remain unchanged.

`--json` gains fields rather than changing existing ones. Whatever flags were
given, the `rhei.accounting.cost.v1` payload carries `selection`,
`run_attribution`, and `roots`:

```json
{
  "schema": "rhei.accounting.cost.v1",
  "selection": { "run": null, "since": null, "until": null, "invocation_count": 7 },
  "roots": [
    { "path": "billing/runtime/accounting", "rheis": ["billing"], "invocation_count": 7 }
  ],
  "run_attribution": {
    "attributed_invocation_count": 1,
    "unattributed_invocation_count": 6,
    "unattributed": { "...": "an AccountingRunSummary over those records" }
  },
  "summary": { "...": "..." },
  "task": null,
  "groups": [
    { "key": "b3ed70", "unattributed": false, "summary": { "...": "..." } }
  ],
  "errors": []
}
```

`roots` is one entry per accounting root the reading covered, in enumeration
order ([§FS-rhei-panta.6.5](rhei-panta.spec.md#65-cost-and-summary)): its `path`, the `rheis` whose execution root it is,
and the `invocation_count` it contributed. It is present on every reading, not
only an empty one, so a consumer can tell a total over one root from a total
over nine without parsing text. Adding it is additive within
`rhei.accounting.cost.v1` (§8.1) and needs no new schema id.

`run_attribution` counts over the set the run filter was applied to — the
window's scope, after `--since` and `--until` and before `--run` — and
`run_attribution.unattributed` rolls up the records in it that name no run.
Counting after `--run` would report zero unattributed records whenever a run was
asked for, which is exactly when the doubt §6.2 demotes for has to be visible. A
caller reading `summary` alone therefore cannot mistake an unattributed history
for an attributed one.

## 9. Visualization

The TUI header shows a compact run-level strip when accounting is available:

```text
Cost: $1.23  total=2.6M  in=2.4M  in_cached=1.5M  out=180k
```

The header uses absolute token totals rather than a cache percentage so cached
input stays visible as its own dimension. Unavailable dimensions render as `-`.
Each active TUI slot line shows the current direct accounting reported for that
task next to its elapsed running time. When an agent only reports a final total
token count, the compact slot line shows `total` and leaves input/cache/output
dimensions unavailable rather than misclassifying the total as input or output.
The slots pane also shows a current
run-level token/cost total below the slot rows, even before the first usage
report arrives; unavailable dimensions render as `-`.

When `UsageReported` arrives after slot release, the TUI updates the run-level
header, slot-pane total, and journal summary. Active task lines update while the
same task remains in a slot; completed-slot history is not kept in the terminal
UI.

The end-of-run console summary and the durable run report include a run-level
accounting strip. It reports **the run that just ended** — the records that name
it (§6.1) — and never the workspace's lifetime total in its place. A run that
spawned no agent reports zero, because zero is what it spent. `workspace_total`
(§6) is still shown, on a labelled row of its own, so naming the two apart loses
nothing. What the strip contains, and how its two possible sources are told
apart, is in [§FS-rhei-run-report.2.1](rhei-run-report.spec.md#21-accounting-strip). Task rows may show a compact direct task cost only; the direct
task cost is the sum of usage reported for all agent states spawned for that
task in the run. Both are frontends under [Run Events](#7-run-events), and the
rule reaches both of their levels: the run-level strip and each task row are
built by upserting on invocation id, so one invocation counts once however many
reports it sent and whether or not its slot was still active when the last one
arrived. Where two reports for one invocation differ, the later one replaces
the earlier one; it is neither added to it nor discarded in favour of it.
[§FS-rhei-run-report](rhei-run-report.spec.md#fs-rhei-run-report-per-run-report)

The browser dashboard adds a **Cost** tab before **Journal**. Its live summary
shows:

- run totals;
- top-level task direct and subtree costs;
- top-level task subtree input, cached input, and output totals.

The dashboard serves per-invocation details from `/accounting/invocations` so a
future drill-down can show token dimensions and pricing status without bloating
the frequently polled `/snapshot` payload.

Task accounting rollups are carried in `task_runtime` so dashboard views can add
direct cost, subtree cost, input, output, and cached input where that density
fits. The current Cost tab exposes the compact top-level task table. The
selected-task surroundings panel includes a token section with direct and
subtree rollups for the clicked task, and live refreshes that section when
`/snapshot` reports updated accounting. The dashboard Slots view shows per-slot
task accounting columns and a current run total below the slot table. Future
Cube and Sankey modes may use subtree cost as heatmap color or ribbon width.
[§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization)

## 10. Dashboard Data

The frequently polled `/snapshot` payload carries compact rollups:

```ts
type TaskAccounting = {
  direct?: AccountingRollup;
  subtree?: AccountingRollup;
};

type TaskRow = {
  // existing flattened task fields
  accounting?: TaskAccounting;
};

type Snapshot = {
  accounting?: AccountingRunSummary;
  tasks: TaskRow[];
};
```

Invocation details are served from a separate loopback endpoint such as
`/accounting/invocations` so `/snapshot` stays small. [§FS-rhei-run-tui](rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal)

## 11. Failure Modes

| Failure | Required behavior |
| --- | --- |
| Extractor failure | Write an invocation record with `extractor-failed` and the non-empty reasons required by §3 and §4, expose those reasons in task inspection, emit `UsageReported`, and continue normal completion and transition handling. |
| Missing price | Record measured tokens with `unpriced` or `partial-price`. |
| Accounting write failure | Warn in the run journal and mark run accounting coverage partial. Do not hide the agent log or transition outcome. |
| Malformed accounting artifact | `rhei cost` reports the bad path and continues reading other valid records. With `--json`, it returns a structured error. |
| Accounting identity conflict | Inspection retains the first valid record, reports the conflicting record and both paths, and continues read-only. Run preflight checks the complete in-scope root union, using the root selection and shared-root task filtering in [§FS-rhei-panta.6.5](rhei-panta.spec.md#65-cost-and-summary), before per-root currency checks. A conflict within or across those roots refuses before mutation or spawn with an accounting-identity diagnostic naming both paths; it must not describe the conflict as a selected-currency failure. |
| Unreadable accounting root | Name the root, read every other root in scope, and do not report `complete` (§6.2). With `--json`, a structured error naming the root, and its `roots` entry carries the count it could contribute. |
| Concurrent writes | Write to a unique staging path, then atomically rename to `<invocation_file_id>.json`. Rollup files may be regenerated after pass writes complete. |

## Related Specifications

- [Agents Specification](rhei-agents.spec.md) - agent configuration and spawn behavior
- [Run Specification](rhei-run.spec.md) - orchestrator execution loop
- [Run TUI Specification](rhei-run-tui.spec.md) - event surface and dashboard transport
- [Flow Visualization](rhei-viz.spec.md) - visual plan views
- [Snapshots Specification](rhei-snapshots.spec.md) - separate session snapshot feature
