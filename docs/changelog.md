# Changelog

*Each release's section is generated from the pull requests merged since the
previous release; nothing here is written by hand.*

## 2. [0.6.5] - 2026-11-16

- [Let the failed-own-record test tolerate healthy delayed sibling startup](https://github.com/agent-grounds/rhei/pull/740) (PR #740)
- [Classify a polling fan-out with a free arm as waiting on its poll](https://github.com/agent-grounds/rhei/pull/738) (PR #738)
- [Explain a ticket held only by its poll deadline as waiting on its poll, which rhei resumes itself](https://github.com/agent-grounds/rhei/pull/737) (PR #737)
- [Keep a ticket halted on spent attempts or a refused log name out of the passes a deadline sleep opens](https://github.com/agent-grounds/rhei/pull/734) (PR #734)
- [Wait for provider-held supervisors with open children](https://github.com/agent-grounds/rhei/pull/732) (PR #732)
- [Specify that a provider wait holds only a ticket with an arm still to spawn, and only in the continuous run](https://github.com/agent-grounds/rhei/pull/733) (PR #733)
- [Contribute a wait deadline only for a ticket nothing but a clock holds, so an expired provider record no longer keeps a run rescanning](https://github.com/agent-grounds/rhei/pull/731) (PR #731)
- [Make a fan-out due when its first arm still to be spawned frees](https://github.com/agent-grounds/rhei/pull/730) (PR #730)
- [Predict spawn refusals in dry runs](https://github.com/agent-grounds/rhei/pull/716) (PR #716)
- [Append e2e fixture lines under a lock, so two processes appending to one file at once on Windows both keep their line](https://github.com/agent-grounds/rhei/pull/724) (PR #724)
- [Name the ready task as the instantiation stop point and record a real write's invocation in the past tense](https://github.com/agent-grounds/rhei/pull/720) (PR #720)
- [Number a stay by every arrival in its state, including ones the ledger never recorded](https://github.com/agent-grounds/rhei/pull/715) (PR #715)
- [Reset a ticket scope with `rhei reset --task`](https://github.com/agent-grounds/rhei/pull/704) (PR #704)
- [Say that rhei run --parallel counts tasks, each with all its fan-out arms](https://github.com/agent-grounds/rhei/pull/714) (PR #714)
- [Name the missing Tasks section at misplaced task headings](https://github.com/agent-grounds/rhei/pull/693) (PR #693)
- [Name the base and the path tried when a link's target does not exist](https://github.com/agent-grounds/rhei/pull/713) (PR #713)
- [Resolve a bare index.panta.md to the current directory as its project](https://github.com/agent-grounds/rhei/pull/721) (PR #721)
- [Resume the agent visit a run killed outright left behind, once its process group is gone](https://github.com/agent-grounds/rhei/pull/678) (PR #678)
- [Say that a parent's Prior does not hold its children](https://github.com/agent-grounds/rhei/pull/694) (PR #694)
- [Give the claim rollback and ledger-writer tests the shared 30 s patience so a slow runner cannot time them out at two seconds](https://github.com/agent-grounds/rhei/pull/712) (PR #712)
- [Explain invalid COMPLETE values and how to recover](https://github.com/agent-grounds/rhei/pull/711) (PR #711)
- [Let a qualified ticket id pick its rhei among loose plans](https://github.com/agent-grounds/rhei/pull/710) (PR #710)
- [Keep live gate fixtures alive when raising the visits budget](https://github.com/agent-grounds/rhei/pull/706) (PR #706)
- [Deliver the mid-grace shutdown at grace entry in the supervised precedence test so a late-scheduled raiser cannot fail it with TimedOut](https://github.com/agent-grounds/rhei/pull/707) (PR #707)
- [Remove ticket exports during narrowed reset so completion requires fresh output](https://github.com/agent-grounds/rhei/pull/705) (PR #705)
- [Refuse task metadata throughout workspace and Panta manifests](https://github.com/agent-grounds/rhei/pull/700) (PR #700)
- [Let a continuous run sleep through a provider wait instead of opening a pass every half second](https://github.com/agent-grounds/rhei/pull/699) (PR #699)
- [Record an `--until-idle` idle stop as finished, with exit code 3, in run.json, the run registry and the evidence manifest](https://github.com/agent-grounds/rhei/pull/697) (PR #697)
- [Wait out a refused read of c's spawn record in the failed-own-record fixtures so a Windows sharing violation cannot fail the test](https://github.com/agent-grounds/rhei/pull/698) (PR #698)
- [End a codex-family stdin command line `-- -` so a forked session reads its prompt](https://github.com/agent-grounds/rhei/pull/690) (PR #690)
- [Spawn a private copy of the rhei binary in the test harness so a concurrent relink cannot remove it](https://github.com/agent-grounds/rhei/pull/691) (PR #691)
- [Settle a fan-out arm at every exit, so a spawn error or worker panic leaves nothing outstanding after the run](https://github.com/agent-grounds/rhei/pull/682) (PR #682)
- [Explain budget admission refusals in the final run halt](https://github.com/agent-grounds/rhei/pull/686) (PR #686)
- [Let a `pi:<provider>/<model>` target wait out a provider outage instead of spending the state's attempts](https://github.com/agent-grounds/rhei/pull/683) (PR #683)
- [Refuse a Directory Workspace link that leaves the workspace root whether or not its target exists](https://github.com/agent-grounds/rhei/pull/687) (PR #687)
- [Attach Codex MCP servers through supported native configuration](https://github.com/agent-grounds/rhei/pull/675) (PR #675)
- [Name the template file when instantiate refuses a file it rendered](https://github.com/agent-grounds/rhei/pull/669) (PR #669)
- [Pipe the built-in pi's prompt on stdin with no trailing separator, so a prompt over 128 KiB starts pi](https://github.com/agent-grounds/rhei/pull/674) (PR #674)
- [Refuse overlapping exit_code rules from one state when the machine loads](https://github.com/agent-grounds/rhei/pull/665) (PR #665)
- [Settle each arm of a fanout at its own exit, from its own record](https://github.com/agent-grounds/rhei/pull/660) (PR #660)
- [Fold consecutive attempts at one visit into one rhei summary step](https://github.com/agent-grounds/rhei/pull/656) (PR #656)

## 3. Older releases

- [0.6.4](changelog/0.6.4.md) - 2026-11-09: 69 pull requests.
- [0.6.3](changelog/0.6.3.md) - 2026-10-26: 6 pull requests.
- [0.6.2](changelog/0.6.2.md) - 2026-10-19: 5 pull requests.
- [0.6.1](changelog/0.6.1.md) - 2026-10-12: 28 pull requests.
- [0.6.0](changelog/0.6.0.md) - 2026-10-03: 51 pull requests.
- [0.5.1](changelog/0.5.1.md) - 2026-09-28: - Let an `agents.<id>` profile name the built-in family it belongs to.
- [0.5.0](changelog/0.5.0.md) - 2026-09-14: - **Price books now accept and preserve the extension metadata their published v1 schema permits.** Additional document and entry properties retain their JSON values in every participating run-owned copy without affecting price selection, validation, coverage, or calculated amounts.
- [0.4.1](changelog/0.4.1.md) - 2026-09-07: - **Parallel refills preserve each task's requested execution identity.** When a freed `--parallel` slot schedules newly ready work, the reloaded task's full `**Target:**` override now continues to select its agent, mode, provider, and model instead of silently falling back to the state's target.
- [0.4.0](changelog/0.4.0.md) - 2026-09-05: - **The fissile config lives at `.agent-grounds/fissile.toml`, where an agent working in the repository can still reach it.** `.agents/` is where agent *instructions* live, and a managed permission profile mounts it read-only inside a checkout, so an agent that hit a size finding could not adjust a budget or record an exception without leaving its sandbox — tool config had ended up in the one directory it was least able to repair.
- [0.3.3](changelog/0.3.3.md) - 2026-08-31: - **`dir_template` can now name a per-working-directory session store.** A `FlatById` layout's `dir_template` may contain the placeholder `{cwd_dashed}`, which expands to this spawn's own canonicalized working directory with every character outside `[A-Za-z0-9-]` replaced by `-` — the convention Claude Code uses for its own per-project session directories — so a template like `~/.claude/projects/{cwd_dashed}` names the directory a supervised checkout actually writes into, instead of one literal path shared across every checkout.
- [0.3.2](changelog/0.3.2.md) - 2026-08-30: - **The re-spawn note on a poll state names its own `poll.max_attempts` instead of an internal sentinel.** A poll state is exempt from the visit attempt budget — `poll.max_attempts` already bounds it — and that exemption was encoded internally as `u64::MAX`, which `rhei run` then printed verbatim: `attempt 4 of 18446744073709551615`.
- [0.3.1](changelog/0.3.1.md) - 2026-08-30: - **The release commit stages `xtask/Cargo.toml`.** `Auto bump` had failed on its last three runs, always at `release.yml`'s version check and always before the publish step, with `xtask/Cargo.toml internal dependency requirement is stale: ...
- [0.3.0](changelog/0.3.0.md) - 2026-08-23: - Give a cold invocation the project's **mid-term memory**.
- [0.2.0](changelog/0.2.0.md) - 2026-08-22: - Separate a run from the surface that watches it.
- [0.1.0](changelog/0.1.0.md) - 2026-05-21: - Initial alpha release line for the Rhei CLI, Rust crates, npm wrappers, and PyPI wrappers.
