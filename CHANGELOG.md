# Changelog

Rhei's changelog is [docs/changelog.md](docs/changelog.md). It carries the
latest release inline; the release generates the next one from the merged pull
requests and archives the release it displaces under `docs/changelog/`
(§FS-rhei-distribution.5).

Nothing below is maintained by the release. The `0.1.0` notes are kept here
because this is where they were written and the archive entry for that version
is a summary; every release after it is in `docs/changelog.md`.

## 0.1.0 - 2026-05-19

Initial release.

### Added

- Markdown plan parsing for single-file plans and directory workspaces.
- Semantic validation for task state, dependency, hierarchy, terminal-tree, link, and artifact-contract rules.
- CLI commands for validation, rendering, ready-work selection, task completion, explicit transitions, reset, state-machine inspection, template instantiation, skill installation, shell completions, and version reporting.
- YAML state-machine support for transitions, callbacks, program states, agent/tooling profiles, counted review loops, and human gates.
- Terminal and journal support for monitoring parallel `rhei run` execution.
- Renderers for JSON, GitHub-style Markdown, and terminal progress output.
- Example workspaces for release automation, review loops, changeset review, human-intervention workflows, CI healing, and spec/implementation audits. (Templates were project- and user-scoped in 0.1.0; a built-in library shipped inside the binary landed after it — see the unreleased notes.)
- Rust library crates for core parsing, validation, output rendering, TUI events, and N-API bindings.

### Known Release Limitation

- crates.io publication uses conflict-free package names, because `rhei` and `rhei-core` both belong to an unrelated project on crates.io: `rhei-cli` for the command and `rhei-plan` for the Rust plan-model API. The installed binaries are `rhei` and `rh`, and Rust import names remain stable through explicit library names and dependency aliases.
