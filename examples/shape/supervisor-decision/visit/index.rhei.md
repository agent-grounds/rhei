# Rhei: Fix the crash on a 64-bit literal

## Overview

Issue 87 says the toolchain panics on an integer literal wider than 64 bits.
A supervising parent reproduces the crash, decides where the fix goes, and has
it fixed there.

The decision is the parent's own work: it is made in the parent's visit
between the two children and written into the fix's brief.
