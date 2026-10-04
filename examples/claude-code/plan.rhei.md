# Rhei: Patch a Service with Claude Code Under Restricted Permissions

## Context
This example keeps the workflow intentionally simple while still modeling a
realistic Claude Code task under least-privilege constraints. The agent can
inspect the repository, edit code, and run safe local checks, but must record
any blocked follow-up work in the plan content instead of relying on dedicated
approval or review states.

## Constraints
- Do not assume blanket permissions.
- Use the strongest checks already available locally.
- Record blocked verification or handoff needs in task content.

## Tasks

### Task 1: Reproduce the timeout regression locally
**State:** completed

Capture the failing request path and identify the smallest reproducible test.

#### Task 1.1: Document the failing endpoint
**State:** completed
Record the endpoint, expected timeout, and observed failure mode.

### Task 2: Patch timeout and retry handling
**State:** in-progress
**Prior:** Task 1

Implement the code fix using only currently allowed tools.

#### Task 2.1: Update timeout defaults
**State:** in-progress
Adjust the client timeout and retry policy in the application code.

#### Task 2.2: Note blocked follow-up verification
**State:** pending
Document any command that still requires explicit approval, plus the safe local
checks already attempted.

### Task 3: Run the strongest safe verification
**State:** pending
**Prior:** Task 2

Execute the best local checks available without new permissions and summarize
any residual risk in the task body.

#### Task 3.1: Execute unit and targeted integration checks
**State:** pending
Run only the checks already allowed in the current environment.

### Task 4: Prepare the final handoff note
**State:** pending
**Prior:** Task 3

Summarize the diff, completed checks, blocked follow-up work, and remaining
risk so a human can decide whether more verification is needed.
