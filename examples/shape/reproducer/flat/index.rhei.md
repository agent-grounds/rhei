# Rhei: Fix the integer-literal overflow
**States:** shape-reproducer

## Overview

Issue 87 reports that the parser panics on an integer literal wider than 64
bits. Triage the report, reproduce the panic, fix the parser, and run the gate
against the reproduction.

The reproduction is a task of its own, beside triage: the fix and the gate
both read it by name.
