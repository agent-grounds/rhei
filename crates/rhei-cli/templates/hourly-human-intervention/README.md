# Hourly Human Intervention Template

Instantiate this from the coordination workspace root:

```sh
rhei instantiate hourly-human-intervention --output .agent-grounds/scratchpad/hourly-human-intervention-$(date -u +%Y%m%dT%H%M%SZ)
```

The instantiate output prints the generated file tree, the task tree, the last
few tasks, and the current stop point. For a fresh workspace it stops before
execution because `--execute` was not passed.

Then run the instantiated workspace:

```sh
rhei run .agent-grounds/scratchpad/hourly-human-intervention-<timestamp> --parallel 1
```

The template creates three initial tasks:

- fetch and classify open `human-intervention` issues
- fetch and classify open `human-intervention` pull requests
- follow up on open `rhei` pull requests, including reviewer-comment fixes and
  merges when review and CI gates allow them

Each classification creates child tasks that run through deep analysis, route
selection, and the matching fix path:

- CI failure triage, restart/rerun, and completion or fix routing
- GitHub human handoff
- Forge fix and two review passes
- GraalVM proposal, human review, fix, and two review passes

The `rhei` PR follow-up task is separate from classification. It works in
isolated PR worktrees before pushing reviewer-comment fixes.

Keep the default serial run unless each mutating task has been moved to its own
worktree. The template edits shared local checkouts.

The `states.yaml` file is a template and contains input placeholders. To inspect
the concrete state machine, instantiate it first and run `rhei states` against
the generated `states.yaml`.

## Codex sandbox

This template's four targets default to `codex[workspace]`: the built-in
profile's mode that runs unattended (`approval_policy="never"`) with network on
and writes only under the roots Rhei grants at launch, which are the plan and
the Panta project around it, the checkout the state runs in with its git
directories, and Rhei's own state directory (§FS-rhei-agents.2.2). The
built-in's other mode, `yolo`, writes anywhere (`--sandbox danger-full-access`),
but a managed ChatGPT workspace refuses it, and the worker then falls back to
read-only, exits `0` and writes nothing.

Its fix states also write in the Forge and GraalVM checkouts, which are granted
only where they lie under the checkout root the state runs in. Grant each one
that does not, with its repository's git directory when it is a linked worktree,
and the same for the isolated worktrees the `rhei` pull-request follow-up works
in. Their builds write outside every checkout too: Forge's Gradle build needs
`~/.gradle`, and GraalVM's `mx` build, `mxb ee` included, needs `~/.mx`.

**Migrating.** A plan laid with an earlier Rhei keeps the settings it was laid
with, and with them `danger-full-access`. In a plan laid now, a Codex state
cannot write outside those roots: not a build cache such as `~/.cargo`,
`~/.gradle` or `~/.npm`, and not another repository. This template ships no
Codex profile, so grant a directory with one: an `agents` entry beside
`defaults` in the plan's `.agent-grounds/rhei/settings.json`, or in the Panta
project's copy if the settings were hoisted, that keeps the built-in's family:

```json
"agents": {
  "codex": {
    "family": "codex",
    "modes": {
      "yolo": ["--sandbox", "danger-full-access", "--skip-git-repo-check",
               "-c", "approval_policy=\"never\""],
      "workspace": ["--sandbox", "workspace-write", "--skip-git-repo-check",
                    "-c", "approval_policy=\"never\"",
                    "-c", "sandbox_workspace_write.network_access=true",
                    "--add-dir", "/home/me/.gradle",
                    "--add-dir", "/home/me/.mx"]
    }
  }
}
```

`modes` is replaced whole, so `yolo` is restated to keep it; `writable_roots` is
still inherited from the built-in. The path must be absolute: Rhei passes mode
tokens through untouched, so `~` and `$HOME` are not expanded. If settings of
your own replace `codex` by id, give that entry the same `family` and
`workspace` mode: without the mode a plan laid from this template is refused at
`rhei validate`, and without the family it is granted no roots unless it writes
`writable_roots` itself. The way back is to select `codex[yolo]`. It works only
on an account without managed requirements: where they refuse
`danger-full-access`, Codex rust-v0.159.3 stops at startup if they still allow
`approval_policy="never"` and otherwise falls back on both settings and runs
read-only, as earlier releases do; Rhei names each fallback. Where `never` is
refused, every Codex invocation also reports an `approval_policy` fallback,
harmless under `on-request`, since writes inside the granted roots need no
approval there.
