# Rhei: Fix the integer-literal overflow
**States:** shape-running-the-gate

## Overview

Issue 87 says the parser panics on an integer literal wider than 64 bits. Fix
the parser, run the gate, and ship the fix.

The gate is the last state of implement, `build -> gate`.
