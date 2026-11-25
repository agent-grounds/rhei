# Changelog

*Each release's section is generated from the pull requests merged since the
previous release; nothing here is written by hand.*

## 2. [0.7.0] - 2026-11-25

- [Show the gate brief in the dashboard and the run TUI, and answer a briefed gate there](https://github.com/agent-grounds/rhei/pull/773) (PR #773)
- [Give gating states a brief and answer it with rhei gate](https://github.com/agent-grounds/rhei/pull/768) (PR #768)
- [Summarize other rheis' orphan snapshots during member validation](https://github.com/agent-grounds/rhei/pull/775) (PR #775)
- [Explain finished workers refused by a destination's missing input in Attention, halt lines and retry](https://github.com/agent-grounds/rhei/pull/772) (PR #772)
- [Put the human-review-loop example on the built-in machine's states and stop presenting review states as built-in](https://github.com/agent-grounds/rhei/pull/771) (PR #771)
- [Make the agent-discussion example complete with its committed mock](https://github.com/agent-grounds/rhei/pull/769) (PR #769)
- [Explain spent destination visits in run warnings and Attention](https://github.com/agent-grounds/rhei/pull/765) (PR #765)
- [Show the operator prompt in one write and make the attended driver wait for its whole line](https://github.com/agent-grounds/rhei/pull/763) (PR #763)
- [Hold a non-concurrent state's limit per machine source, not per state name](https://github.com/agent-grounds/rhei/pull/762) (PR #762)

## 3. Older releases

- [0.6.6](changelog/0.6.6.md) - 2026-11-23: 9 pull requests.
- [0.6.5](changelog/0.6.5.md) - 2026-11-16: 41 pull requests.
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
