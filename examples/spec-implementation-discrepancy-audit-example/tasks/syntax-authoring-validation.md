### Task syntax-authoring-validation: Audit plan syntax, authoring, parsing, and validation
**State:** scope-spec

Audit the core plan language and validation contract for Rhei.

Spec scope:

- `docs/functional-spec/rhei-plan-language.spec.md`
- `docs/functional-spec/rhei-authoring.spec.md`

Implementation roots:
- `crates`
- `skills`
- `.agent-grounds/rhei/templates`
- `examples`

Focus on markdown grammar, directory workspace semantics, task hierarchy rules,
metadata fields, `states.yaml` resolution, prior dependency semantics, terminal child
coherence, artifact contract validation, and diagnostics promised by the specs.