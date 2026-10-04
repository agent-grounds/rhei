# Rhei: Stream the CSV export

## Overview

Issue 412 asks for an export that streams its rows instead of building the file
in memory. Write the spec point and the failing test first, then build to them,
review the change against them, and run the gate.

The contract is a task of its own: implement, review and the gate all name it.
