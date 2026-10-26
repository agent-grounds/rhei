# Changelog

*Each release's section is generated from the pull requests merged since the
previous release; nothing here is written by hand.*

## 2. [0.6.3] - 2026-10-26

- [Let programs write results directly on fresh plans](https://github.com/agent-grounds/rhei/pull/512) (PR #512)
- [Launch the shipped templates' Codex states with workspace-write and granted writable roots, and report Codex configuration fallbacks](https://github.com/agent-grounds/rhei/pull/510) (PR #510)
- [Refuse an existing output in instantiate --dry-run like the real write](https://github.com/agent-grounds/rhei/pull/508) (PR #508)
- [Warn in rhei validate when the file layout serializes a concurrent state](https://github.com/agent-grounds/rhei/pull/505) (PR #505)
- [Report missing budget journals instead of blaming their lock file](https://github.com/agent-grounds/rhei/pull/506) (PR #506)
- [Verify program budget accounts before spawning](https://github.com/agent-grounds/rhei/pull/502) (PR #502)

## 3. Older releases

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
