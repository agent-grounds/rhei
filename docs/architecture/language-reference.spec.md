# AR-rhei-language-reference: Canonical Language Reference Architecture

Rhei's language is intentionally split across markdown plans, YAML state
machines, templates, and execution-time references. The architecture must keep
one canonical language-reference entry point that names those surfaces and
routes readers to the owning specs. [§FS-rhei-language-reference](../functional-spec/rhei-language-reference.spec.md#fs-rhei-language-reference-rhei-language-reference)

## 1. Architectural Requirement

The functional spec set must include a single language-reference page for the
complete user-authored Rhei language surface. Architecture and onboarding
documents must point readers to that page before listing lower-level grammar,
state-machine, transition, or template specs.

This page must exist because Rhei's parser, validator, runner, LSP, templates,
and skills all depend on the same authored source model. Without one entry
point, implementers and agents can easily treat one feature spec as the whole
language and miss constraints owned elsewhere.

## 2. Documentation Boundary

The language-reference page is a map, not a duplicate specification:

- It owns the taxonomy of user-authored language surfaces.
- It owns the recommended reading path for language questions.
- It points each surface to its normative owner.
- It must be updated when a feature adds or changes user-authored syntax.
- For template composition syntax, it distinguishes input-selected declaration
  groups and opt-in identity exposure and routes both to their specific block
  contract sections.

Detailed rules remain in the owning specs so grammar, state-machine, template,
and command behavior can evolve independently without creating conflicting
copies of normative text.
