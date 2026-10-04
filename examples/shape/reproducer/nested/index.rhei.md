# Rhei: Fix the integer-literal overflow

## Overview

Issue 87 reports that the parser panics on an integer literal wider than 64
bits. Triage the report, reproduce the panic, fix the parser, and run the gate
against the reproduction.

The reproduction is a step of triage, under it: the fix and the gate reach
into the triage subtree to read it.
