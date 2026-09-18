# e2e

Black-box, user-scenario proof of the spec: every test here runs the real
`rhei` binary as a subprocess and asserts on what it prints and writes, the
way a user or an agent would invoke it — never on the crates behind it, across
the whole command surface and the roles and patterns it serves. [§FS-rhei-usage](../../docs/functional-spec/rhei-usage.spec.md#fs-rhei-usage-how-rhei-is-used)

This directory is the home of the non-citable `e2e` kind (`grund.toml`,
`[[kinds]] kind = "e2e"`): a scenario is exercised by being run, never cited,
so no test here declares an ID. `[citations.e2e]` says the home must cite `FS`
and should not cite `AR` — a scenario proves the What as a user sees it, and
one that reads the design is not black-box. Every top-level test file carries
the `§FS-…` citation for the behavior it proves, at the top of the file or on
the specific test that needs the narrower section.

## Layout

This directory is also the workspace member `rhei-e2e-tests` (never
published). `mod.rs` is the crate's one `[[test]]` harness root — the module
list `cargo test --workspace` and `cargo test -p rhei-e2e-tests` both build —
and every sibling `*_tests.rs` file is a `mod` of it, one file per command or
behavior area, matching the split `AR-source-file-size` asks for. Shared
fixture and process helpers live in `../support/`, pulled in with `#[path]`
because they are reached from both this crate and `rhei-integration-tests`.

`fixtures/` holds multi-file workspace scenarios (`living-review-loop/`,
`script-agent-team/`) that tests copy into a temporary directory before
running `rhei` against them. It is test-input data, not a citable or scanned
document, so `[scan] exclude` keeps it out of the host scan the same way
`templates/` is excluded.

## Reading stderr

Captured stderr is never a tty, so miette renders every diagnostic wrapped at
eighty columns. It breaks only at spaces, which is exactly what
[§FS-rhei-errors.2](../../docs/functional-spec/rhei-errors.spec.md#2-copy-paste-safety)
asks of it, so the sentence arrives whole and the binary is right. But a
`contains` reads a rendered line rather than the sentence, and a phrase that
straddles the break is invisible to it. Where that break lands moves with a
pid, a temporary path, a run id — so the same assertion passes on one machine
and fails on the next, which is the worst shape a test failure can take.

**No assertion here may depend on where the renderer wrapped.** Turning
captured stderr into text is the harness's job and happens at one seam,
`stderr(&output)` in `mod.rs`, which undoes the soft wrap. Do not convert
`output.stderr` in a test file: `diagnostic_wrap_tests.rs` fails and names the
file that did. Redirected stderr goes through `stderr_from_file`; when physical
rendering is itself the subject, use `raw_stderr` for output or
`raw_stderr_from_file` for a redirected file.

Undoing a wrap is not repairing one. A continuation is rejoined with the single
space the wrap removed and never with nothing, so a token broken mid-word stays
broken and the suite can still catch it. Rendered blocks are never merged: a
message and its `help:` stay apart, because a phrase matched across the two was
never printed. A false pass costs more than the false failure it replaces.

The inverse is close but not exact, and the gap is worth knowing before you
write the assertion. A diagnostic may carry a newline of its own, and when the
text before that newline happens to fill the column the rendered result is
identical to a wrap — so those two lines are joined, and a `contains` can match
across them. The renderer cannot be asked which it was: miette splits the
message at its own newlines before wrapping each piece. Assert a phrase that
lives on one side of a newline the message wrote itself.
`diagnostic_wrap_tests` runs the two shipped diagnostics that meet this and
records it as a limit rather than a promise.

## Rust

`cargo test --workspace --all-targets` builds and runs these; `cargo test -p
rhei-e2e-tests` runs them alone and builds the `rhei` binary on demand
(`../support/binaries.rs`) when the workspace build has not already produced
it.

Unit tests live with the code they test (`crates/*/src/`); a claim about one
module stays there. A test belongs here once it drives the built binary as a
subprocess rather than calling the crates directly — that boundary is what
`../integration/` and `crates/rhei-core/tests/` sit on the other side of.
