# Temporary macOS evidence for issue 301

Preparation for the approved proposal round 2, under
§AR-ci-release.1 docs/architecture/ci-release.spec.md:8. No product correction
or contract acceptance occurs here. Normal CI stays unchanged. Remove every
file inventoried below before shipping PR #303, after retaining the artifacts.

**One investigation, four slots**

The `Issue 301 macOS evidence` workflow is limited to PR #303 on
`fix/issue-301`. Job key `paired-watch-trace` has display name
`issue-301 full-suite macOS evidence`. The first preparation run
(`35428542945`, attempt 1, head `d868c7f97c319790cde3a1893d5a2df56171fc0a`)
spent zero slots because its checkout lacked the historical object. Supervisor
checkpoint 11 authorizes exactly one continuation after verifying and retaining
that run's exact artifact. The continuation runs this unchanged protocol once:

1. Full suite on unchanged historical `b0e3f86ad7ef37e75732beaa0adfa9f154b60a5b`.
2. Full suite on unchanged current baseline `c841d36fe5f650ba3352c89724df59a2509e8f0a`.
3. Only if slot 2 reproduces the named test's recovery-command assertion
   (`left: 2`, `right: 1`): full suite on instrumented current source, in-root.
4. One full suite on that same instrumented source with only the named E2E's
   stderr path changed to its outer temporary root.

The named E2E is
`export_prior_migration_implementation_tests::omitted_validate_watch_renders_copyable_migration_help`.
Its own libtest status and captured failure block open the pair, independently
of other tests' failures. A historical failure or another test's assertion
cannot open it. Default test parallelism/capture and full workspace scope stay:

```text
cargo test --workspace --all-targets --locked --no-fail-fast --offline --target-dir <scratch>/target
```

Each invocation, including compilation, has a 45-minute process-group timeout.
There are no focused executions, retries, repeated pairs or extended polling.
A suite's normal exit 101 is retained and does not discard the named outcome.
A build/timeout/access failure stops collection as inconclusive, retaining all
completed slots. Incomplete or contrary in-root measurement also stops before
spending the outside-root slot; the unused slot does not authorize a restart. Setup commands share an aggregate 30-minute allowance; each
fetch allows at most 10 minutes. Collection allows 220 minutes; the job allows
270, with separately bounded setup actions and 10 minutes for artifact upload.
A job killed before a final result is explicitly incomplete.

`execution-budget.json` spends each slot **before** launch; an interrupted slot
is not refunded. Command records are written before spawn and after completion.
The workflow serializes runs without canceling an active collector. The
collector rejects attempts other than 1 and queries earlier workflow runs for
this branch. The first commit introducing `round-2-full-suite-v1` must remain
the original diagnostic head. Collection proceeds only when the sole earlier
protocol run is the predecessor named above and its completed run API record
and unexpired, digest-verified artifact prove the exact missing-object failure
and an empty four-slot ledger. The predecessor artifact and authorization proof are retained
under `authorized-continuation/`. A later head sees both protocol runs and is
refused, so this exception cannot grant a second continuation. Missing,
unreadable, expired or inconsistent run/artifact evidence is inconclusive.
There is no general setup-failure exemption, automatic recovery or retry.

**Source, environment and path fidelity**

Source archives, explicit build targets and diagnostic artifacts live under
`~/ag/tmp`. One target directory reuses dependencies sequentially; Cargo and the
existing harness freshness build validate binaries against each source copy.
Before any suite spends a slot, the collector directly fetches both exact
approved commits, resolves their commit and tree identities, and rejects any
identity other than historical
tree `cc582280de5b0521df4570d2a12537c61dbb05a1` or current tree
`fd8c4cf1eccc54685dff8fd80ff3e75e9957d963`. Acquisition commands, raw streams
and `source-identity.json` remain under `source-acquisition/`. Each preparation
resolves the identity again immediately before archiving it. The present PR
head, main or an archive of unverified provenance cannot substitute.
Every source directory records and enforces Rust/Cargo 1.82.0 and the same host.
Dependency preparation remains `cargo fetch --locked --target <recorded-host>`;
subsequent builds use the offline cache. Each revision retains its own lockfile,
with source hashes before and after execution. No prebuild runs a test.

`TMPDIR` is inherited unchanged, restoring native macOS fixture paths. Unexpected
thread/capture/fixture-retention/probe overrides fail setup. Environment records
retain the original temp spelling, OS image, kernel, toolchain, commands, cwd,
SHA, run/attempt and numeric job identity. Disposable instrumentation records
both lexical and canonical fixture/root paths; raw events keep their original
paths/kinds. Canonical aliases are used only to check containment, never to
rewrite raw evidence. Unchanged baselines cannot disclose successful fixture
paths without instrumentation: their actual paths are available only when
emitted in raw failure output. This limit is explicit in each baseline outcome.

**Actual E2E measurement**

`fixture.patch` instruments the real named test only, in a disposable current
source after baseline reproduction. It retains omitted discovery, plan cwd,
isolated home/state, null stdout, the same raw-stderr helper, 25 ms polling,
10-second deadline, snapshot, ChildGuard stop and original assertions. No
measurement I/O is inserted in the poll loop or between its successful read
and kill/wait. The only post-spawn/pre-deadline addition is an in-memory PID
assignment. Snapshot and final stderr bytes are saved **after the existing
stop**, before assertions and fixture cleanup. A guard declared before
ChildGuard retains partial files on panic after the child guard has reaped.
Missing normal-stop or snapshot evidence is incomplete. A process-group timeout
can prevent guard unwinding; already-written artifacts still survive.

`watch-trace.patch` and `trace.rs` preserve event filtering, registration,
250 ms debounce, original eprintln and rendering. Only the named child's
explicit env enables a trace; parallel unrelated watch tests cannot write it.
The trace filename includes the child PID and its fixture directory includes
the harness PID. All traces/retained files live outside registered roots.

The trace records successful roots/modes, processed event path/kind/filter
results (including debounce), debounce termination/wait observations, pass
begins/ends and stderr byte offsets. The accepted receive-event sequence is
retained across debounce and links to the following pass. Analysis extracts
`pass-NNN.stderr`, complete physical command lines with the discovered quoted
target, raw admission records, and whether each pass was wholly in the exact
assertion snapshot. Candidate support requires the instrumented in-root E2E
also to fail with 2 != 1 and at least two complete passes inside its snapshot,
with the outside-root E2E passing and having only its initial complete pass.
Raw events still require human causal inspection; directory-level events alone
do not prove which write generated them.

Before/after plan and states bytes establish read-only behavior under
§FS-rhei-migrate.5 docs/functional-spec/rhei-migrate.spec.md:145. Per-pass help
checks serve §FS-rhei-validate.5 docs/functional-spec/rhei-validate.spec.md:201
and §FS-rhei-errors.1.2 docs/functional-spec/rhei-errors.spec.md:37.

Tracing adds file metadata, JSON, locking and write overhead. Records report a
cumulative lower bound for prior probe I/O; the final record's cost and JSON
construction/initialization are not fully measured. Fixture setup and post-stop
copy durations are recorded separately, also as lower bounds. This cannot
measure how instrumentation changes scheduling. Fixed in-root/outside-root
order and cache warmth are additional limits. Queued events at kill are unseen;
partial JSON, incomplete pass intervals, bytes outside intervals, missing
identity/roots/attribution or failed retention remain measurement failures.
No extension waits for a pass-end record merely to complete the trace.

**Outcomes and retrieval**

| Exit | Diagnostic meaning |
| --- | --- |
| 0 | Candidate support: current named baseline failure plus the required attributed location-dependent snapshot passes. Supervisor inspection and a revised contract are still required. |
| 2 | Inconclusive setup/access/build/timeout/measurement failure or budget refusal. Preserve partial evidence. |
| 3 | Bounded non-reproduction or incomplete support, including a passing current baseline or no location difference. |
| 4 | Contrary evidence: malformed/duplicate help within a complete pass, changed authored bytes, or extra outside-root passes. |

Contrary observations remain visible even when other measurements are missing;
exit 4 takes precedence over incomplete measurement in completed analysis.
Other suite failures remain in their raw logs and outcome records. No exit
means the product is fixed or shippable, and none releases implementation.

Artifacts upload with `always()` as `issue-301-macos-<run-id>-<attempt>` for
30 days. Start with `summary.txt`, `result.json`, `execution-budget.json`,
`baseline-outcomes.json`, `pair-condition.json` and `paired-outcomes.json`.
Each slot directory retains `suite.command.json`, raw `suite.stdout/.stderr`,
`named-test-failure.txt`, `outcome.json`, source hashes, and available setup logs.
Instrumented slots add `measurement/case-<harness-pid>/` with `initial.json`,
`case.json`, `trace-<child-pid>.jsonl`, raw snapshot/final stderr, per-pass stderr
and before/after authored inputs. `analysis.json` records measurement limits.
`collector/` preserves the exact collector used. `jobs.json` is captured during
initialization; retrieve final run/job metadata separately without overwriting it.

After the exit program publishes this commit, verify the remote head and locate
its workflow/run/attempt. Download into a new runtime evidence directory,
preserving all earlier evidence and the spent-slot ledger:

```text
gh run list --repo agent-grounds/rhei --workflow issue-301-macos-evidence.yml --branch fix/issue-301
gh run view <run-id> --repo agent-grounds/rhei --json headSha,jobs,attempt,url
gh run download <run-id> --repo agent-grounds/rhei \
  --name issue-301-macos-<run-id>-<attempt> --dir <new-runtime-evidence-directory>
```

No run ID or reproduction verdict is supplied by this preparation. Do not
request a rerun, dispatch a probe, or treat a missing final result as a pass.

**Temporary inventory to remove before shipping**

- `.github/workflows/issue-301-macos-evidence.yml`
- `scripts/issue-301-macos/README.md`
- `scripts/issue-301-macos/cases.py`
- `scripts/issue-301-macos/evidence.py`
- `scripts/issue-301-macos/run.py`
- `scripts/issue-301-macos/trace.rs`
- `scripts/issue-301-macos/watch-trace.patch`
- `scripts/issue-301-macos/execution.py` (new)
- `scripts/issue-301-macos/fixture.rs` (new)
- `scripts/issue-301-macos/fixture.patch` (new)
