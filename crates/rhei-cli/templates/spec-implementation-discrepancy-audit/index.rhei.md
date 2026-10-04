# Rhei: {{audit_title}}

## Context

Audit the written {{subject}} specifications under `{{spec_root}}` against the
current implementation. The workspace is partitioned so multiple agents can
independently scope, compare, elaborate, and propose reconciliation for
different spec areas before a human chooses the final reconciliation outcome.

Implementation roots to inspect:

{%- for root in implementation_roots %}
- `{{ root }}`
{%- endfor %}
{% if extra_context %}

Additional context:

{{extra_context}}
{% endif %}

Each task follows the local `states.yaml` workflow:

1. `scope-spec` inventories normative claims and implementation surfaces.
2. `compare-implementation` records raw discrepancies.
3. `elaborate-discrepancies` explains impact and evidence.
4. `propose-reconciliation` recommends a concrete path.
5. `human-decision` stops for a human multi-decision choice.

The human decision options are represented as terminal states:
`update-spec`, `update-implementation`, `update-both`, `defer-follow-up`,
`no-change`, and `cancelled`.

## Audit Rules

- Treat spec files as normative only when they use concrete behavior language,
  command contracts, validation rules, state-machine schema, or artifact
  semantics.
- Cite implementation evidence with file paths and line numbers where practical.
- Distinguish implementation drift from stale spec text, ambiguous prose,
  missing tests, and intentional behavior.
- Do not edit the spec or implementation during this audit workspace. The
  output is a human decision package, not the reconciliation patch itself.
