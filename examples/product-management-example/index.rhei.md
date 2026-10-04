# Rhei: Rhei Product Management Run

## Overview

This workspace runs a repeated product-management loop for `Rhei`.
Each pass has three stages:

1. Multiple PM agents independently produce product entries.
2. `codex[xhigh]:openai:gpt-5.5` aggregates and validates every entry.
3. `codex[medium]:openai:gpt-5.4-mini` implements the accepted slice.

The loop runs for **2** passes by default for this instantiation.

## Product Brief

Improve the Rhei authoring and execution experience for teams that use
agent-driven plans. Focus on predictable execution, useful monitoring, and
templates that are easy to instantiate repeatedly.


## Implementation Scope

The implementation agent may change only the following scope unless the smart
agent explicitly records a narrow exception in the implementation slice:

`docs/functional-spec, .agent-grounds/rhei/templates, examples`

## Focus Areas
- template usability
- monitoring clarity
- predictable execution

## Validation Criteria
- user value is explicit
- evidence or assumption is stated
- scope fits one implementation pass
- conflicts with existing specs or roadmap are identified

## Agent Roles

| Role | Target |
|---|---|
| PM fan-out | `claude-code[yolo]:anthropic:claude-opus-4-7`, `codex[xhigh]:openai:gpt-5.5` |
| Aggregation and validation | `codex[xhigh]:openai:gpt-5.5` |
| Implementation | `codex[medium]:openai:gpt-5.4-mini` |