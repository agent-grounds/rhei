### Task syntax-authoring-validation: Audit plan syntax, authoring, parsing, and validation
**State:** scope-spec

Audit the core plan language and validation contract for {{subject}}.

Spec scope:

- `{{spec_root}}/rhei-plan-language.spec.md`
- `{{spec_root}}/rhei-authoring.spec.md`

Implementation roots:

{%- for root in implementation_roots %}
- `{{ root }}`
{%- endfor %}

Focus on markdown grammar, directory workspace semantics, task hierarchy rules,
metadata fields, `states.yaml` resolution, prior dependency semantics, terminal child
coherence, artifact contract validation, and diagnostics promised by the specs.
