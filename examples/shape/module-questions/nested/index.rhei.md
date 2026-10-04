# Rhei: Can a crash lose a committed write?

## Overview

A crash at any point must not lose a committed write. Ask that one question of
each module a write passes through, and report what their answers add up to.

The report is the parent of the three questions: it is made out of their
answers.
