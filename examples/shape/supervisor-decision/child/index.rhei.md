# Rhei: Fix the crash on a 64-bit literal
**States:** shape-supervisor-decision

## Overview

Issue 87 says the toolchain panics on an integer literal wider than 64 bits.
A supervising parent reproduces the crash, decides where the fix goes, and has
it fixed there.

The decision is a child of its own, between the reproducer and the fix.
