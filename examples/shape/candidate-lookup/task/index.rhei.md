# Rhei: Triage and fix issue 87
**States:** shape-candidate-lookup

## Overview

Issue 87 says the parser panics on an integer literal wider than 64 bits.
Decide whether it duplicates an earlier issue, then fix it.

The lookup is a task of its own, and the verdict is the task after it.
