# Rhei: {{plan_title}}

## Overview

This workspace runs a repeated product-management loop for `{{product_name}}`.
Each pass has three stages:

1. Multiple PM agents independently produce product entries.
2. `{{smart_target}}` aggregates and validates every entry.
3. `{{implementation_target}}` implements the accepted slice.

The loop runs for **{{loop_passes}}** passes by default for this instantiation.

## Product Brief

{{product_brief}}

## Implementation Scope

The implementation agent may change only the following scope unless the smart
agent explicitly records a narrow exception in the implementation slice:

`{{implementation_scope}}`
{%- if focus_areas %}

## Focus Areas

{%- for area in focus_areas %}
- {{area}}
{%- endfor %}
{%- endif %}

## Validation Criteria

{%- for criterion in validation_criteria %}
- {{criterion}}
{%- endfor %}

## Agent Roles

| Role | Target |
|---|---|
| PM fan-out | {% for t in pm_targets %}`{{t.selector}}`{% if not loop.last %}, {% endif %}{% endfor %} |
| Aggregation and validation | `{{smart_target}}` |
| Implementation | `{{implementation_target}}` |
