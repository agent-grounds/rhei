# Rhei: Agent Discussion Around a Point

## Overview

This directory workspace demonstrates a structured, multi-round discussion among
four agents that converges on a decision — and gates downstream work on that
decision.

Unlike a one-shot poll, participants take each other's points into account: every
round after the first reads the previous round's judge digest and responds to the
other participants by name. Each participant also argues from a different project
goal, so the discussion is a genuine multi-perspective deliberation rather than
four independent opinions.

## Notes

- The seed discussion task starts in `collect`, which declares
  `all_models: [claude, codex, gemini, cursor]`. The runtime invokes the
  `write-position` callback once per participant, with `RHEI_MODEL` set.
- Each participant writes `runtime/discussion/round-<N>/<model>.md`, arguing from
  the project goal it champions (see `goal_for` in `workflow.py`).
- The `judge` state synthesizes the round into
  `runtime/discussion/digest/round-<N>.md` and decides via a callback redirect:
  converge (`converged`), run another round (`collect`), or escalate to a human
  (`escalated`).
- On convergence the judge records `runtime/discussion/decision.md`. The
  `apply-decision` task is blocked (`**Prior:** Task discussion-seed`) until the
  discussion reaches `converged`, so the decision actually gates work.
- The `collect ↔ judge` loop is driven by the judge's `nextState` redirect, not by
  `visits` (which must not be combined with `all_models`). The round budget is
  capped by `CAP` in `workflow.py` (default 3). By default the mock converges in
  round 2; set `RHEI_DISCUSSION_FORCE_ESCALATE=1` to drive it to the human gate.
