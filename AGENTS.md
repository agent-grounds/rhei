# Agent Workflow Notes

## Goals

- Focus on user experience and agent performance
- Make monitoring tools useful and pretty
- Rhei execution should be predictable

## Specification

- Start from the [Language Reference](docs/functional-spec/rhei-language-reference.spec.md)
— the one entry point for the whole user-authored language surface ([§AR-rhei-language-reference](docs/architecture/language-reference.spec.md#ar-rhei-language-reference-canonical-language-reference-architecture))
— before the narrower specs below.
- See the [Rhei Plan Language Specification](docs/functional-spec/rhei-plan-language.spec.md).
All textual spec files must end with `.spec.<file-ending>`.
- [ADR (Architecture Decision Record)](docs/adr/adr.md)
- Follow progressive disclosre in the spec

## CI Verification Commands

Run these commands from the repository root to mirror CI checks:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::all
cargo build --workspace --all-targets
cargo test --workspace --all-targets --no-fail-fast
```

Every change also adds a bullet under `## Unreleased` in `docs/changelog.md`
without a pull request number — the release stamps it, and the pre-push hook
refuses a push that adds no bullet (see `CONTRIBUTING.md`).


<!-- BEGIN GRUND MANAGED BLOCK -->
## Grounding with grund (v10)

This project uses [`grund`](https://github.com/vjovanov/grund): every spec, goal, decision, and end-to-end test has a stable ID `<KIND>-<slug>[.<section>]` (`KIND ∈ {GND, GOAL, REQ, FS, AR, DF, DA, ADR, RM}`), cited with the marker `§` — e.g. `<§>FS-user-login.3.1` (the `FS-user-login` here is a shape illustration, not a real ID in this repo, hence the `<§>` escape). Type `$$` in a grund-aware editor and it becomes `§`. Bare ID-shaped tokens are ignored — `[reference] strict = true` is set in `grund.toml`, so only `§`-prefixed citations are checked.

### Grounding from a citation

A `§<ID>` is a pointer to a fact, not a file path. Resolve it with `grund` and climb only as far as needed:

- `grund <ID>` — the lead (heading-less, cut at the first child section). The cheap first read for a bare `§<ID>` citation.
- `grund <ID> --toc` — the lead plus the nested section map. Use to choose which subsection to fetch next.
- `grund <ID> --full` — the entire body. Escalate to this when narrower reads aren't enough.
- `grund <ID> --brief` — heading + first paragraph only.
- `grund refs <ID>` — every site that cites the ID; add `--summary` for one line per file. Run before renaming or moving a declaration.
- `grund list` / `grund list --kind FS,AR` — discover IDs if you get lost
- `grund list --size=words --top 10` — find heavy leads and move detail into citable child points before reading them in full

### Project map

- [GND](docs/grund.md): Reason for existence
- [GOAL](docs/functional-spec/goals.md): Outcomes the project is trying to achieve
- [REQ](docs/requirements): Cross-cutting requirements every feature is held to
- [FS](docs/functional-spec): User-visible behavior and requirements
- [AR](docs/architecture): System design and technical structure
- [DF](docs/decisions/functional): Product behavior decisions and tradeoffs
- [DA](docs/decisions/architectural): Architecture decisions and tradeoffs
- [ADR](docs/adr): Architecture decision records
- [tests/e2e/](tests/e2e): User scenarios: black-box proof of the spec
- [tests/integration/](tests/integration): Integration tests: proof that the parts fit as designed
- [RM](docs/functional-spec/roadmap.md): Planned milestones and sequencing

### Project namespaces

A namespace is a project boundary, not a docs folder. The current project is the local namespace: cite its IDs as `§<ID>`.

Create or use a separate namespace when work introduces an independently checked app, package, service, or subproject. Give that project its own `grund.toml`, add it to the workspace root's `[workspace] members`, run `grund init` there, and set a stable `project_name`.

Do not create a namespace for a regular module or component that still belongs to this project. Cite across namespaces as `§alias/<ID>` and run `grund check` from the workspace root.

### Declarations and citations

Declarations are heading lines `# FS-user-login: …` in markdown. In a code doc-comment (Rustdoc, Javadoc, JSDoc, Python docstring, Go `//`, …) drop the `#` — write `/// FS-user-login: …` directly. Numbered headings inside a declaration are citable sections: use depth-matching headings (`## 1. …`, `### 1.1 …`, etc.) so `§<ID>.1` / `§<ID>.1.1` resolve; mismatched heading depth is a `grund check` error. Every non-declaration heading inside a Markdown declaration body must carry a numbered or enabled named section path; file titles, headings that close the body, fenced examples, non-ATX text, and source doc-comments stay exempt, while bold labels remain the non-citable alternative. One doc-comment may declare multiple IDs (e.g. an `AR-` and an `FS-` on the same class) — each gets its own body. An inline source declaration is reachable from the configured kind home via a one-line stub: `# <ID>: [<path>](<path>)`.

### Rules

- **Spec first.** For behavior or design changes, write or update the most-specific spec point before code.
- **Cite as you write.** Place `§<ID>` at the point a claim or behavior is made — on the doc-comment for a whole behavior, inline beside the clause it enforces.
- **Marker = live citation.** A `§`-prefixed token resolves and is checked wherever it appears — including inside Markdown backticks. To mention an ID without citing it, write `<§><ID>`, omit the marker, or use a fenced code block.
- **Inline citation style.** Inline notes: ≤ 1 line preferred, hard cap 3 lines; ≤ 120 columns. A note is one comment block: a blank line splits it, an empty comment line does not. Doc-comments (`///`, `//!`, `/** */`, a docstring, a comment right above a definition) are documentation, not notes: they are never measured, so cite in-sentence there.
- **Always cite the most-specific point.**

### Citation directions

`must`/`never` are `grund check` errors; `should`/`avoid` are suggestions (`grund check --suggestions`).

- Each file in **tests/e2e/** must cite FS; avoid citing AR.
- Each file in **tests/integration/** should cite AR.
Anything not listed above is allowed.

### Clickable citations

On repository web surfaces, link `§<ID>` to the PR branch in PR bodies, the reviewed commit in reviews, an exact commit for permalinks, and the default branch otherwise; fall back to plain when unsure.
<!-- END GRUND MANAGED BLOCK -->
## Rhei

The Rhei (Panta) project for this repository lives in `panta/`. Plans are
`*.rhei.md` files and workspace directories; ticket ids are
project-qualified (`<rhei>.<id>`). Work tickets with `rhei list`,
`rhei next`, and `rhei complete`; validate edits with `rhei validate`.
Orchestration (`rhei run`) is started by humans, never by agents.
<!-- rhei:end -->

<!-- BEGIN FISSILE MANAGED BLOCK -->
## Keeping Files Small With fissile (v3)

This repository caps file size with [`fissile`](https://github.com/vjovanov/fissile)
so that agents spend fewer tokens reading. Run `fissile check --staged` before
claiming work is done; its findings say what to split and how. Where the
pre-commit hook is installed it runs that same check — never get past it with
`--no-verify`.
<!-- END FISSILE MANAGED BLOCK -->
