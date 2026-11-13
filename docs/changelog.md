# Changelog

*Each release's section is generated from the pull requests merged since the
previous release; nothing here is written by hand.*

## 2. [0.6.4] - 2026-11-09

- [Explain held visits budgets when releasing a live gate](https://github.com/agent-grounds/rhei/pull/654) (PR #654)
- [Keep failed-own-record tests reliable when a sibling starts late](https://github.com/agent-grounds/rhei/pull/652) (PR #652)
- [Recover shared-file fixture polling refusals and retain failure diagnostics](https://github.com/agent-grounds/rhei/pull/650) (PR #650)
- [Name the typed target and machine in the release note and the supervisor handoff](https://github.com/agent-grounds/rhei/pull/645) (PR #645)
- [Accept early account refusal in the detached budget log test](https://github.com/agent-grounds/rhei/pull/651) (PR #651)
- [Read a file saved with a UTF-8 byte order mark as the same file without it, so a state machine with one is no longer refused as more than one YAML document](https://github.com/agent-grounds/rhei/pull/639) (PR #639)
- [Keep a shell command a help offers whole on one line, recognized by the words that offer it](https://github.com/agent-grounds/rhei/pull/649) (PR #649)
- [Retain ownership of MCP preflight failure endpoints](https://github.com/agent-grounds/rhei/pull/648) (PR #648)
- [Resolve bare visualization filenames from the current directory](https://github.com/agent-grounds/rhei/pull/646) (PR #646)
- [Name original files and explain how to retry refused template placements](https://github.com/agent-grounds/rhei/pull/628) (PR #628)
- [Correct values-file help for YAML loading failures](https://github.com/agent-grounds/rhei/pull/644) (PR #644)
- [Say a state machine document must be a mapping when it holds none](https://github.com/agent-grounds/rhei/pull/641) (PR #641)
- [Restore the lost line continuations in two diagnostics' prose](https://github.com/agent-grounds/rhei/pull/642) (PR #642)
- [Refuse a state machine whose name, version or models entries are not the type FS-rhei-states.1.1 names](https://github.com/agent-grounds/rhei/pull/640) (PR #640)
- [Start every rhei a test runs with none of its caller's RHEI_*, so a parent run's reservation no longer fails the suite](https://github.com/agent-grounds/rhei/pull/637) (PR #637)
- [Explain when a state machine may omit profiles and node_policy](https://github.com/agent-grounds/rhei/pull/638) (PR #638)
- [Read a plan's frontmatter as YAML in every command that edits its text, so `rhei complete` no longer refuses a plan `rhei validate` accepts](https://github.com/agent-grounds/rhei/pull/625) (PR #625)
- [Print rhei release's Released line only after that ticket's claim is removed](https://github.com/agent-grounds/rhei/pull/636) (PR #636)
- [Name the spent poll budget in the result entry it writes](https://github.com/agent-grounds/rhei/pull/626) (PR #626)
- [Read fenced lines as content in every pass of `rhei reset`, so a plan `rhei validate` accepts resets and keeps what its fences quote](https://github.com/agent-grounds/rhei/pull/621) (PR #621)
- [Stop telling a well-formed state machine that it is invalid YAML or should fix its syntax](https://github.com/agent-grounds/rhei/pull/624) (PR #624)
- [Stop promising `renews automatically` for a ticket that moved out of the state a bound refused it in](https://github.com/agent-grounds/rhei/pull/610) (PR #610)
- [Let basin tickets re-enter counted states after `rhei reset`](https://github.com/agent-grounds/rhei/pull/620) (PR #620)
- [Use matching fences for task completion, description checks and result memory](https://github.com/agent-grounds/rhei/pull/618) (PR #618)
- [Refuse before any preview, `--dry-run` included, a `rhei reset` the filesystem would stop, so it no longer leaves a plan half-reset](https://github.com/agent-grounds/rhei/pull/616) (PR #616)
- [Carry a serial run past a ticket that did not move, so a ticket deferred behind it still runs](https://github.com/agent-grounds/rhei/pull/598) (PR #598)
- [Read one header and one frontmatter block in every command, so `rhei complete` and `rhei reset` work on any header the parser accepts](https://github.com/agent-grounds/rhei/pull/607) (PR #607)
- [Report the actual halt cause of a worked pending ticket in Attention](https://github.com/agent-grounds/rhei/pull/600) (PR #600)
- [List a ticket one bound stopped once under `## Bounds`, however many passes refused it](https://github.com/agent-grounds/rhei/pull/606) (PR #606)
- [Report each dependency cycle alone and sorted, so a standing one no longer makes `rhei new` and `rhei remove` refuse at random](https://github.com/agent-grounds/rhei/pull/602) (PR #602)
- [Read a released supervisor over a live subtree as waiting on it, with no Attention row advising the reader to cancel it](https://github.com/agent-grounds/rhei/pull/601) (PR #601)
- [Keep effort fixture version probes from writing outside the test directory](https://github.com/agent-grounds/rhei/pull/595) (PR #595)
- [List a rhei's state machine once when the rhei gains or loses it, in rhei seal and the strict-inputs refusal](https://github.com/agent-grounds/rhei/pull/599) (PR #599)
- [Name the active ceiling holder in remedies at equality](https://github.com/agent-grounds/rhei/pull/593) (PR #593)
- [Name the spent bound and its remedy or renewal in the Attention row of a ticket it halted](https://github.com/agent-grounds/rhei/pull/594) (PR #594)
- [Make rhei budget refuse the targets rhei list refuses by shape, and name the command's target rather than the account root in every line it prints](https://github.com/agent-grounds/rhei/pull/590) (PR #590)
- [Report lifetime allowance provenance and effective halt remedies](https://github.com/agent-grounds/rhei/pull/589) (PR #589)
- [Let snapshot list and gc get past an unreadable generation, naming it and leaving it on disk](https://github.com/agent-grounds/rhei/pull/588) (PR #588)
- [Retain acknowledged interventions in final evidence without delaying agent exit](https://github.com/agent-grounds/rhei/pull/576) (PR #576)
- [Make successful attach-wait outage tests recover before run completion](https://github.com/agent-grounds/rhei/pull/575) (PR #575)
- [Refuse a reset of a plan validation already refuses, before any preview and with nothing deleted](https://github.com/agent-grounds/rhei/pull/574) (PR #574)
- [Make a failing command's help name the target it was given, so its line runs as pasted](https://github.com/agent-grounds/rhei/pull/567) (PR #567)
- [Send a finished ticket's forced line only to a state whose visit budget is not spent, and name the budget it passes over](https://github.com/agent-grounds/rhei/pull/573) (PR #573)
- [Refuse a reset that would leave a plan validation refuses, and stop offering it as the way back](https://github.com/agent-grounds/rhei/pull/571) (PR #571)
- [Give refusals help that addresses their blocking condition](https://github.com/agent-grounds/rhei/pull/560) (PR #560)
- [Start a finished ticket's help at its topmost finished ancestor, rather than offer a line the forced path refuses](https://github.com/agent-grounds/rhei/pull/566) (PR #566)
- [Stop commits and the required CI job depending on outside hosts a change does not touch, and check every external link in a daily sweep](https://github.com/agent-grounds/rhei/pull/568) (PR #568)
- [Stop the hung-up terminal test timing out when another test's process inherits its terminal, and say what the run left behind if it still does](https://github.com/agent-grounds/rhei/pull/563) (PR #563)
- [Discard a resumed poll window the scenario's own steps miss, and prove the run from a fresh workspace](https://github.com/agent-grounds/rhei/pull/558) (PR #558)
- [Stop the inflight-stop tests failing when their helper reads an empty pid file or a stall spends its 20 s wait](https://github.com/agent-grounds/rhei/pull/556) (PR #556)
- [Report spawn-record write failures and explain refused-spawn halts](https://github.com/agent-grounds/rhei/pull/550) (PR #550)
- [Make a message offering rhei reset print a command that runs, say how far it reaches, and put any one-ticket way back first](https://github.com/agent-grounds/rhei/pull/551) (PR #551)
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
