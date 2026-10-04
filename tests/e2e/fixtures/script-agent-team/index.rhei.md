# Rhei: Bash Agent Team Workflow

## Overview
This directory workspace models a small agent team handoff pipeline that can be
executed end to end with `rhei run`.

## Notes
- The workflow is intentionally script-based.
- The first transition runs a mock kickoff command.
- Each callback writes logs and artifacts into `runtime/` inside this example.
