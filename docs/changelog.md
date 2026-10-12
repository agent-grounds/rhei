# Changelog

*Each release's section is generated from the pull requests merged since the
previous release; nothing here is written by hand.*

## 2. [0.6.1] - 2026-10-12

- [Launch Claude Code agents with strict MCP configuration so rhei's selection is their whole MCP surface](https://github.com/agent-grounds/rhei/pull/488) (PR #488)
- [Give every rhei-cli lib-test fixture one margin that outlasts a stalled runner, and pin the inherited pipe without a clock](https://github.com/agent-grounds/rhei/pull/486) (PR #486)
- [Read all of an agent's pre-exit output before the drain grace starts](https://github.com/agent-grounds/rhei/pull/485) (PR #485)
- [Refuse every task metadata field in a `rhei new` description, `**Excludes:**` included](https://github.com/agent-grounds/rhei/pull/482) (PR #482)
- [Print the agent's own task id in its prompt's `rhei note` command, and stop promising agents `RHEI_TASK_ID`](https://github.com/agent-grounds/rhei/pull/481) (PR #481)
- [Let a task name the MCP servers and skills its agent needs, and let a state withhold them](https://github.com/agent-grounds/rhei/pull/478) (PR #478)
- [Write the --mcp-config file in Claude Code's schema so declared MCP servers reach the agent](https://github.com/agent-grounds/rhei/pull/477) (PR #477)
- [Recognize Codex usage-limit refusals and park without spending attempts](https://github.com/agent-grounds/rhei/pull/473) (PR #473)
- [Name the project as the remedy for an unparsable basin sibling](https://github.com/agent-grounds/rhei/pull/472) (PR #472)
- [Validate instantiate dry runs through the destination project and name a blocking sibling](https://github.com/agent-grounds/rhei/pull/468) (PR #468)
- [Treat a bare instantiate --output as a member of the project in the working directory](https://github.com/agent-grounds/rhei/pull/469) (PR #469)
- [Add `rhei remove` to take back an untouched ticket and retire its id](https://github.com/agent-grounds/rhei/pull/467) (PR #467)
- [Let a machine delegate the two count ceilings to the project with `defaults.clamp_projects: false`](https://github.com/agent-grounds/rhei/pull/463) (PR #463)
- [Point the stricter spend grade at its own issue](https://github.com/agent-grounds/rhei/pull/461) (PR #461)

## 3. Older releases

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
