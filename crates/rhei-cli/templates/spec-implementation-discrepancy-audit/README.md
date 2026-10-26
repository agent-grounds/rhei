# Spec Implementation Discrepancy Audit Template

This template creates a directory workspace for auditing written
specifications against implementation. It partitions the audit into spec areas,
asks agents to scope each area, compare implementation behavior, elaborate
discrepancies, propose reconciliation, and then stop at a human-only decision
gate.

## Inputs

| Name | Type | Default | Description |
|---|---|---|---|
| `audit_title` | string | `Spec Implementation Discrepancy Audit` | Title for the instantiated workspace. |
| `subject` | string | `Rhei` | Product, repository, or subsystem being audited. |
| `spec_root` | string | `docs/functional-spec` | Documentation root containing the Rhei functional spec files. |
| `implementation_roots` | array<string> | `crates`, `skills`, `crates/rhei-cli/templates` | Root paths containing implementation, tests, templates, or skills. |
| `audit_target` | string | `codex[workspace]:openai:gpt-5.5` | Execution target for each autonomous audit state. |
| `extra_context` | string | empty | Optional context appended to the workspace overview. |

## Task Paths

| Task kind | State path | Purpose |
|---|---|---|
| Audit partition | `scope-spec` -> `compare-implementation` -> `elaborate-discrepancies` -> `propose-reconciliation` -> `human-decision` -> terminal choice | Audits one spec area and produces a human decision package. |

The terminal human choices are `update-spec`, `update-implementation`,
`update-both`, `defer-follow-up`, `no-change`, and `cancelled`.

## Flow

1. Instantiate the template inside the repository being audited.
2. Each partition starts in `scope-spec` and writes a scope inventory.
3. The comparison pass checks scoped claims against code, tests, templates,
   skills, fixtures, and command behavior.
4. The elaboration pass explains risk, impact, and evidence.
5. The proposal pass recommends a reconciliation option for every finding.
6. The human reads the proposal artifact and transitions the task to the
   chosen terminal reconciliation state.

The state machine is documented in [states.yaml](states.yaml).

## Codex sandbox

`audit_target` defaults to `codex[workspace]`: the built-in profile's mode that
runs unattended (`approval_policy="never"`) with network on and writes only
under the roots Rhei grants at launch, which are the plan and the Panta project
around it, the checkout the state runs in with its git directories, and Rhei's
own state directory (§FS-rhei-agents.2.2). The built-in's other mode, `yolo`,
writes anywhere (`--sandbox danger-full-access`), but a managed ChatGPT
workspace refuses it, and the worker then falls back to read-only, exits `0` and
writes nothing.

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
                    "--add-dir", "/home/me/.cargo"]
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

## Instantiate

```bash
rhei instantiate spec-implementation-discrepancy-audit \
  --set audit_title="Spec Implementation Discrepancy Audit" \
  --set subject="Rhei" \
  --set audit_target="codex[workspace]:openai:gpt-5.5" \
  --output docs/plans/spec-implementation-discrepancy-audit
```

For non-scalar inputs, use a values file:

```bash
rhei instantiate spec-implementation-discrepancy-audit \
  --values audit-values.yaml \
  --output docs/plans/spec-implementation-discrepancy-audit
```

See the pre-rendered example at
`examples/spec-implementation-discrepancy-audit-example/`.
