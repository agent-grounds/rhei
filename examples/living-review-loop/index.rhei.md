# Rhei: Living Review Loop with Orchestrated Expansion

## Overview
This directory workspace models a living Rhei where the orchestrator is
allowed to append new task files while work is in progress.

## Notes

- The seed review task starts in the `review` state, which declares
  `all_models: [claude, codex]`.
- The runtime calls the `write-review` callback twice — once per model —
  each time with `RHEI_MODEL` set to the current model identifier.
- Each model writes its findings to `runtime/findings/<model>-findings.md`.
- After all model reviews complete the task advances to `consolidate`, where
  `codex` merges findings and appends verification tasks.
- The `prove` state is pinned to `codex` so every verification task uses one
  prover.
- Only findings marked relevant cause the orchestrator to append new fix task files.
