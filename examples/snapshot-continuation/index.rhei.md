# Rhei: Snapshot Continuation Example

## Overview

This workspace demonstrates the snapshot debugging loop:

1. `implement` emits a named `implementation` snapshot.
2. `review` inherits that same-agent snapshot.
3. An operator can use `rhei snapshot continue` to inspect either generation
   interactively without changing task state.
