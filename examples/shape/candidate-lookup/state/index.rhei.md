# Rhei: Triage and fix issue 87

## Overview

Issue 87 says the parser panics on an integer literal wider than 64 bits.
Decide whether it duplicates an earlier issue, then fix it.

The lookup is a program state of triage, feeding the verdict state after it.
