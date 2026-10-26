### Task templates-skills-completions: Audit templates, skills, completions, and generated workflows
**State:** scope-spec

Audit generated and assistant-facing workflow surfaces.

Spec scope:

- `docs/functional-spec/rhei-templates.spec.md`
- `docs/functional-spec/rhei-install-skills.spec.md`
- `docs/functional-spec/rhei-completions.spec.md`
- `docs/functional-spec/rhei-state-machine-writer.spec.md`
- relevant workflow examples in `docs/functional-spec/rhei-usage.spec.md`

Implementation roots:
- `crates`
- `skills`
- `crates/rhei-cli/templates`
- `examples`

Focus on template manifests, rendered plan validity, bundled `states.yaml`
lookup, generated task structure, parent/prior safety, skill installation paths,
completion shell behavior, and whether assistant-facing skills accurately encode
the current spec and validator constraints.
