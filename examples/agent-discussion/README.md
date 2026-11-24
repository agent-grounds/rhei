# Agent Discussion Example

This workspace demonstrates a structured, multi-round discussion among four agents
that **take each other's points into account**, **argue from different project
goals**, and **converge on a decision that gates downstream work**.

It is built entirely from existing Rhei primitives — `all_models` fanout, a
redirect-driven `collect ↔ judge` loop (callback `nextState`), a gating state, and
the `**Prior:**` DAG — so it adds no new engine features.

> **Why no `visits`?** The loop is driven by the judge callback's `nextState`
> redirect, not by a counted-loop counter. `visits` must **not** be combined with
> `all_models` on the same state — the engine would run that state per-target
> per-visit and spin. The round budget is enforced by `CAP` in `workflow.py`.

## The point under discussion

> **D-merge-policy:** When an agent discussion converges on a decision, how should
> that decision enter the plan — auto-merge, a recorded judge ruling, or human
> escalation?

## The four participants and their stances

Each participant argues the point from a different project goal (see `goal_for` in
`workflow.py`), so this is a genuine multi-perspective deliberation:

| Participant | Champions | Opening stance |
|-------------|-----------|----------------|
| `claude` | Developer Experience | Auto-merge; humans read the git diff |
| `codex` | Determinism & Auditability | No silent merge; record a structured ruling |
| `gemini` | Throughput & Scale | Never put a human in the hot path |
| `cursor` | Safety & Human Oversight | Irreversible decisions need a human gate |

## How it flows

1. **Round 1 (`collect`)** — `all_models: [claude, codex, gemini, cursor]` fans the
   `write-position` callback out once per participant. Each writes
   `runtime/discussion/round-1/<model>.md` — a blind opening stance from its goal.
2. **Judge (`judge`)** — the `judge-round` callback synthesizes
   `runtime/discussion/digest/round-1.md`, finds the positions in tension, and
   (returning no redirect) lets the engine loop back to `collect` for another round.
3. **Round 2 (`collect` again)** — the judge's redirect-free return sends the task
   back to `collect` (no counted-loop suffix — the loop is redirect-driven). Each
   participant now reads the round-1 digest and responds to the others by name —
   conceding and sharpening — so the positions actually move.
4. **Judge again** — the round-2 positions have converged on a **risk-tiered
   policy**. The callback writes `runtime/discussion/decision.md` and redirects
   (`{"nextState": "converged"}`) to the terminal `converged` state.
5. **`apply-decision`** — this task declares `**Prior:** Task discussion-seed`, so
   it stays blocked until the discussion is terminal. Once `discussion-seed` is
   `converged`, it runs, reads `decision.md`, and writes
   `runtime/discussion/applied.md`. That is how a discussion gates real work.

If the participants never converge, the judge escalates at the round budget
(`CAP` in `workflow.py`, default 3) to the gating `escalated` state, where a human
resolves it.

## How rounds map to the state machine

```
        write-position (×4, all_models)        judge-round (redirect)
collect ───────────────────────────▶ judge ──────────────┬─────────────▶ converged
   ▲                                                      │  (consensus)
   └──────────────── another round ──────────────────────┘
                                                          │  (budget exhausted)
                                                          └─────────────▶ escalated (human gate)
```

## Run it

By default the example is deterministic (canned positions) so it runs without
model credentials. The committed settings invoke `mock-agent.py` with Python 3,
which the callbacks also use. The launcher reads the stdin prompt and locates the
mock within the copied workspace. When the prompt includes a Result section, the
mock writes a nonempty result to that section's path before exiting successfully.
This supplies the results required for both `judge → converged` and
`apply → completed`; `workflow.py` still produces the positions, digests,
decision and downstream application. [§FS-rhei-states.3.3.1](../../docs/functional-spec/rhei-states.spec.md#331-agent-discussion-example-completion)

Validate the checked-in workspace from the repository root:

```bash
cargo run -p rhei-cli -- --state-machine examples/agent-discussion/discussion-states.yaml validate examples/agent-discussion
```

Run a disposable copy (the run mutates task state and writes `runtime/`):

```bash
cargo xtask examples run agent-discussion
```

or by hand:

```bash
tmp_dir="$(mktemp -d)"
cp -R examples/agent-discussion "$tmp_dir/agent-discussion"
cargo run -p rhei-cli -- --state-machine "$tmp_dir/agent-discussion/discussion-states.yaml" run "$tmp_dir/agent-discussion"
```

After the run, inspect:

- `runtime/discussion/round-1/*.md`, `round-2/*.md` — each participant's position per round
- `runtime/discussion/digest/round-*.md` — the judge's per-round synthesis
- `runtime/discussion/decision.md` — the converged consensus
- `runtime/discussion/applied.md` — proof the downstream task consumed the decision
- `runtime/results/agent-discussion.discussion-seed.md` — mock result permitting the discussion's terminal entry
- `runtime/results/agent-discussion.apply-decision.md` — mock result permitting the downstream task's terminal entry

## Variations

- **Force the human gate:** `RHEI_DISCUSSION_FORCE_ESCALATE=1` makes the judge
  never converge, so the discussion runs the full round budget and lands in
  `escalated`. The `apply-decision` task then stays blocked until a human
  transitions the gate.
- **Live agents:** `RHEI_DISCUSSION_MODE=live` dispatches each participant to its
  real CLI (`claude`, `codex`, `gemini`, `cursor-agent`) with a stance-aware prompt,
  and asks the judge CLI for a CONVERGED/CONTINUE verdict each round.
