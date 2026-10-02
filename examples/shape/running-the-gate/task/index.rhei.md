# Rhei: Fix the integer-literal overflow
**States:** shape-running-the-gate

## Overview

Issue 87 says the parser panics on an integer literal wider than 64 bits. Fix
the parser, run the gate, and ship the fix.

The gate is a task of its own, and its verdict is the export the ship step
reads.
