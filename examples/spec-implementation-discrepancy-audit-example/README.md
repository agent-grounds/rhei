# Spec Implementation Discrepancy Audit Example

This is the checked-in smoke-test instantiation for the
`spec-implementation-discrepancy-audit` template.

## Inputs Used

The example was generated with these values:

```yaml
audit_title: Spec Implementation Discrepancy Audit Example
subject: Rhei
spec_root: docs/functional-spec
implementation_roots:
  - crates
  - skills
  - crates/rhei-cli/templates
  - examples
audit_target: codex[workspace]:openai:gpt-5.5
extra_context: |
  Example instantiation for the Rhei repository. The extra implementation root
  exercises non-default array input handling in the template smoke test.
```

The same values are checked in at
`examples/spec-implementation-discrepancy-audit-example/instantiation-values.yaml`.

## Validate

```bash
rhei validate examples/spec-implementation-discrepancy-audit-example
rhei run examples/spec-implementation-discrepancy-audit-example --dry-run
```

## Regenerate

```bash
rm -rf examples/spec-implementation-discrepancy-audit-example
rhei instantiate crates/rhei-cli/templates/spec-implementation-discrepancy-audit \
  --values crates/rhei-cli/templates/spec-implementation-discrepancy-audit/.example-values.yaml \
  --output examples/spec-implementation-discrepancy-audit-example
```

After regenerating, restore this README and the checked-in
`instantiation-values.yaml` if the generator overwrote them.
