# Changelog

*Each release's section is generated from the pull requests merged since the
previous release; nothing here is written by hand.*

## 2. [0.6.4] - 2026-11-02

- [Block an agent's spawn when a required MCP server cannot be reached](https://github.com/agent-grounds/rhei/pull/546) (PR #546)
- [Keep Codex writable-root tests from committing into repositories above TMPDIR](https://github.com/agent-grounds/rhei/pull/547) (PR #547)
- [Let a pi session that loses its provider wait instead of spending the state's attempts](https://github.com/agent-grounds/rhei/pull/544) (PR #544)
- [Wait for a poll the ready scan held back instead of halting when the clock's second turns](https://github.com/agent-grounds/rhei/pull/543) (PR #543)
- [Take an empty budget journal as the account's own on the first charge, and report it as no account yet](https://github.com/agent-grounds/rhei/pull/541) (PR #541)
- [Leave the budget account as it was when an append fails, and offer a torn witness the trim instead of the copy](https://github.com/agent-grounds/rhei/pull/538) (PR #538)
- [Verify the budget account before reporting a spent visit](https://github.com/agent-grounds/rhei/pull/539) (PR #539)
- [Pack a run's evidence into an archive and verify it away from its workspace](https://github.com/agent-grounds/rhei/pull/533) (PR #533)
- [Write the run report when a run stops on a plan a worker broke](https://github.com/agent-grounds/rhei/pull/532) (PR #532)
- [Record what every invocation was given and produced, and verify a run against it with rhei verify](https://github.com/agent-grounds/rhei/pull/527) (PR #527)
- [Read a Panta member's settings at its project root on every read](https://github.com/agent-grounds/rhei/pull/530) (PR #530)
- [Wait for the worker pool's threads before a stopped parallel run exits](https://github.com/agent-grounds/rhei/pull/528) (PR #528)
- [Seal what a run executes, and refuse to start work after a sealed file changed under rhei run --strict-inputs](https://github.com/agent-grounds/rhei/pull/524) (PR #524)
- [Give a stop on a break under no task's heading the loader's help](https://github.com/agent-grounds/rhei/pull/523) (PR #523)
- [Stop waiting for a poll attempt whose bound is spent](https://github.com/agent-grounds/rhei/pull/519) (PR #519)
- [Record which rhei build, template source and checkout a run used, and print a checkpointed provenance line in rhei summary](https://github.com/agent-grounds/rhei/pull/518) (PR #518)
- [Name the models and the book behind an unpriced count](https://github.com/agent-grounds/rhei/pull/516) (PR #516)

## 3. Older releases

- [0.6.3](changelog/0.6.3.md) - 2026-10-26: 6 pull requests.
- [0.6.2](changelog/0.6.2.md) - 2026-10-19: 5 pull requests.
- [0.6.1](changelog/0.6.1.md) - 2026-10-12: 14 pull requests.
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
