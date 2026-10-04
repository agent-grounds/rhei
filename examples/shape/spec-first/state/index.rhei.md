# Rhei: Stream the CSV export

## Overview

Issue 412 asks for an export that streams its rows instead of building the file
in memory. Write the spec point and the failing test first, then build to them,
review the change against them, and run the gate.

The contract is a handoff between two states of one task: `specify` writes it
and `implement` reads it.
