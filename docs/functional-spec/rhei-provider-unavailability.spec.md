# FS-rhei-provider-unavailability: Recognizing a Provider Outage From an Agent's Own Stream

This is the recognition contract [§FS-rhei-agents.2.5](rhei-agents.spec.md#25-recognized-provider-unavailability) names: how rhei tells,
from an agent's own stream, that the agent's retries gave up on a provider that
stopped answering. Two families are recognized, each by its own stream:
`pi` ([§FS-rhei-provider-unavailability.1](rhei-provider-unavailability.spec.md#1-pi)) and `claude-code` ([§FS-rhei-provider-unavailability.2](rhei-provider-unavailability.spec.md#2-claude-code)). Every
recognition shares these rules:

- the family tested is the resolved family. The agent registry id is not
  tested, so a custom entry with `family: pi` or `family: claude-code` is
  recognized, and the same stream from any other family is not;
- some provider keys the wait ([§FS-rhei-run.3.3](rhei-run.spec.md#33-provider-limit-parking)). It is the provider the
  invocation resolves ([§FS-rhei-agents.1.4](rhei-agents.spec.md#14-resolution-order)); only `pi` has a second source
  ([§FS-rhei-provider-unavailability.1](rhei-provider-unavailability.spec.md#1-pi)). An invocation with no provider to key the wait on is
  not recognized and is charged as today;
- a timed-out or interrupted session is classified as that first and is never
  recognized, whatever its stream said before it ended. A partial stream is
  never read;
- only the stdout JSON events count. Stderr and non-JSON stdout contribute
  nothing;
- the family's own verdict decides: rhei recognizes only the agent saying that
  its own retries were used up. Rhei keeps no error grammar of its own. Where a
  family's count also covers an error that waiting cannot fix, rhei narrows it
  by HTTP status class, never by the family's error names or message text;
- the `signal` is the family's own give-up text, so the operator sees the real
  cause in the warning, the spawn record and the wait.

The recognized result is the `provider_unavailable` ending of
[§FS-rhei-agents.8.4](rhei-agents.spec.md#84-spawn-records); whether it spends an attempt is [§FS-rhei-agents.3.2.3](rhei-agents.spec.md#323-attempt-budget) and
[§FS-rhei-provider-unavailability.3](rhei-provider-unavailability.spec.md#3-what-a-recognized-ending-costs), and the wait it writes is [§FS-rhei-run.3.3](rhei-run.spec.md#33-provider-limit-parking).
It is not a refusal ([§FS-rhei-agents.2.3](rhei-agents.spec.md#23-recognized-provider-refusals)): no reset instant is printed. Where one stream matches
both, the refusal is checked first and wins, because its reset instant is a
better deadline than a backoff. Another family joins by a specification change
backed by its own stream.

## 1. `pi`

A `pi` session can lose its provider and still exit `0`: in `--mode json`, the
mode rhei runs it in for cost accounting ([§FS-rhei-cost-accounting.4](rhei-cost-accounting.spec.md#4-extraction-flow)), pi sets
no failing exit code when its own retries give up. Recognition requires all of
the following:

- the resolved family is `pi`;
- some provider keys the wait ([§FS-rhei-run.3.3](rhei-run.spec.md#33-provider-limit-parking)). It is the provider the
  invocation resolves ([§FS-rhei-agents.1.4](rhei-agents.spec.md#14-resolution-order)). When the invocation resolves no provider,
  it is the text before the first `/` of the model rhei hands the agent (the
  model-flag value of [§FS-rhei-agents.1.1.2](rhei-agents.spec.md#112-agents)), provided both that text and the rest
  are non-empty; it is taken verbatim and case-sensitively, and everything after
  that first `/`, further slashes included, stays the model. So
  `pi:openai-codex/gpt-6.1-sol` is recognized and waits on `openai-codex`, the
  same wait as `pi:openai-codex:openai-codex/gpt-6.1-sol`, and
  `pi:openrouter/vendor/model` waits on `openrouter`. A provider the invocation
  resolves wins over the model's prefix: `pi:openai:openai-codex/x` waits on
  `openai`. The derived provider keys the wait and nothing else: the model, the
  resolved provider, and everything read from them — pricing, `RHEI_PROVIDER`,
  `{model.provider}`, snapshots, accounting — are as they would be without it,
  and refusal recognition never tests it ([§FS-rhei-agents.2.3](rhei-agents.spec.md#23-recognized-provider-refusals));
- the invocation exited `0` and was neither timed out nor interrupted;
- a required output is missing ([§FS-rhei-agents.3.2.1](rhei-agents.spec.md#321-runtime-semantics) step 4);
- among the stdout JSON events, the last `auto_retry_end` has `success: false`
  and an `attempt` equal to the `maxAttempts` of the last `auto_retry_start`
  before it, and the last assistant `message_end` comes before it with
  `stopReason: "error"`.

The last condition is pi's own verdict.
pi retries only an error it classes as transient — a network failure, `429`, a
`5xx`, an overload — and its count reaches `maxAttempts` only when those retries
are used up. An error pi does not retry, such as a context overflow, an
exhausted quota or a `400`, ends the count below `maxAttempts` and stays
charged, because waiting will not fix it. Two endings still reach the count: an
error pi does not retry arriving on its last retry, and `finalError: "Retry
cancelled"` from an abort during the last backoff. Both are recognized, and the
cap of six ([§FS-rhei-agents.3.2.3](rhei-agents.spec.md#323-attempt-budget)) bounds what they cost. The `signal` is pi's
`finalError`, or the last turn's `errorMessage` when it has none.

Not recognized, and charged as today: `stopReason: "error"` with no
`auto_retry_end` (pi's auto-retry off, or a first error it does not retry); an
`auto_retry_end` with `success: true` or an `attempt` below `maxAttempts`; an
invocation that resolves no provider and whose model has no `/` or an empty
half, such as `pi:gpt-6.1-sol`; a timed-out or interrupted session; a non-zero
exit; and a session whose required outputs are all on disk, which moves exactly
as it does today.

## 2. `claude-code`

Claude Code retries a failed request on its own and reports each retry as a
`{"type":"system","subtype":"api_retry"}` event carrying `attempt`,
`max_retries` and `error_status`. When the retries run out it writes one
synthetic assistant event — `model: "<synthetic>"`,
`is_api_error_message: true`, its text the error — and exits `1`. Recognition
requires all of the following:

1. the resolved family is `claude-code`, and the invocation exited non-zero
   and was neither timed out nor interrupted;
2. the invocation resolves a provider to key the wait on, such as `anthropic`
   in `cld[yolo]:anthropic:claude-opus-5-5`. Claude Code models have no
   `<provider>/<model>` spelling, so nothing is derived: a bare `claude-code`
   target is charged as today;
3. the last `api_retry` event has `attempt` equal to that same event's own
   `max_retries`, so a site's `CLAUDE_CODE_MAX_RETRIES` is respected;
4. that event's `error_status` is a transient class: `null`, `429`, or a `5xx`;
5. after that event comes an assistant event with `is_api_error_message: true`,
   and no assistant event whose `model` is anything but `<synthetic>`.

The `signal` is the text of that error event, such as `API Error: Connection
refused — a firewall or proxy may be blocking it (ECONNREFUSED)`.

Condition 4 is the narrowing the lead allows. Claude Code retries server
errors, overloads, timeouts and `429`s, but it also retries a `401` up to its
count, so an expired login or a revoked key would otherwise read as an outage
and send the operator to the network when the fix is to sign in again. The
status class is the protocol's, not Claude Code's: its error names, such as
`"unknown"` for a refused connection, and its `api_error` field are never
tested. `error_status: null` means no HTTP response arrived. It covers a
refused, reset or hung connection, and equally a mistyped `ANTHROPIC_BASE_URL`,
a DNS failure, a TLS error and a broken proxy, which Claude Code retries to the
count in the same way. Those endings are recognized too: the cap of six
([§FS-rhei-agents.3.2.3](rhei-agents.spec.md#323-attempt-budget)) is what bounds them, and the `signal` carries Claude
Code's own text so the operator can see the real cause. An error that is not
retried, arriving on the very last retry, is recognized as pi's equivalent is,
and the same cap bounds it.

Not recognized, and charged and routed as today: a last `api_retry` below its
`max_retries` (including Claude Code giving up early, as it does on repeated
`529`s on some models); a last retry whose `error_status` is outside the
transient classes, such as `401`, `403`, `408` or `409`; no
`is_api_error_message` event after the last retry; an error Claude Code does
not retry at all, such as a `400`, `403`, `404` or `413`, which leaves no
`api_retry` event; an exit `0`, which the output check handles as for any
worker; a timed-out or interrupted session, however far its retries had got;
and an invocation that resolves no provider. Recognition does not depend on a
`type: "result"` event, which a give-up may or may not end with.

## 3. What a Recognized Ending Costs

An ending is free only when the provider answered nothing in the session
([§FS-rhei-agents.3.2.3](rhei-agents.spec.md#323-attempt-budget)), and what counts as an answer is the family's. For `pi`
it is an assistant `message_end` whose `stopReason` is not `"error"`. For
`claude-code` it is an assistant event whose `model` is anything but
`<synthetic>` — text, thinking or a tool call, from the main thread or a
subagent. The give-up message and every other `<synthetic>` message come from
Claude Code itself, not from the provider, so they never count.

The cap of six free endings per visit is about an hour of waiting
([§FS-rhei-run.3.3](rhei-run.spec.md#33-provider-limit-parking)) plus the sessions themselves. A `pi` session gives up in
about two minutes. A `claude-code` session gives up in about three when
connections are refused, and can run up to the state's `agent_timeout` when they
hang, where it is timed out and charged instead ([§FS-rhei-provider-unavailability.2](rhei-provider-unavailability.spec.md#2-claude-code)).
So an outage costs a visit at most six sessions and an hour of waiting before it
halts, naming `rhei retry`, which keeps [§REQ-bounded-neural-work.1](../requirements/bounded-neural-work.spec.md#1-the-five-levels) finite.

A free ending frees an attempt, not spend: each one settles its reservation as
any spawn does ([§FS-rhei-budgets.6.2](rhei-budgets.spec.md#62-reserve-then-settle)). A session that reported its usage, on a
priced model, settles at what it measured — zero for a give-up whose
`type: "result"` event carries zero usage — while each free ending on an
unpriced model, or one without a `result` event, still settles a full reserve.
