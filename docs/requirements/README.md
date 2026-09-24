# Requirements

Cross-cutting requirements live here as grund declarations: rules every
feature is held to from the moment it is specified, as opposed to the
user-visible behaviour of one feature (`FS-`) or an outcome the project is
trying to achieve (`GOAL-`). Each file declares one `REQ-<slug>` ID at its H1
and cites the goal it serves; specs and architecture cite the requirement at
the point they realize it.

| ID | Subject |
|---|---|
| [§REQ-cross-platform](cross-platform.md#req-cross-platform-one-tool-on-linux-macos-and-windows) | One tool on Linux, macOS, and Windows: parity, tested on all three, portable fixtures, paths as data |
| [§REQ-bounded-neural-work](bounded-neural-work.spec.md#req-bounded-neural-work-every-unit-of-neural-work-is-bounded-before-it-starts) | Every unit of neural work is bounded before it starts: the four levels, bounded by default, and where the default numbers came from |

This index is navigational. Normative citations should target the specific
declaration ID rather than this file.
