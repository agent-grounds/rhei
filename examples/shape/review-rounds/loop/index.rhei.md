# Rhei: Stream the CSV export
**States:** shape-review-rounds

## Overview

Implement the streaming CSV export, review and fix it until two rounds have
passed, and ship it.

The rounds are a counted loop of states inside one review task: only the code
the last fix leaves behind is read after it.
