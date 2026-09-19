# Temporary macOS evidence for issue 301

Remove this directory and `.github/workflows/issue-301-macos-evidence.yml`
before shipping PR #303, after downloading the evidence into the plan's
runtime workspace. Nothing here implements the fixture correction. The normal
`CI` workflow and all required platform jobs remain unchanged.

This diagnostic investigates one complete recovery command per failed pass
under §FS-rhei-validate.5 docs/functional-spec/rhei-validate.spec.md:201,
read-only recovery under §FS-rhei-migrate.5 docs/functional-spec/rhei-migrate.spec.md:145,
and runnable physical help lines under §FS-rhei-errors.1.2 docs/functional-spec/rhei-errors.spec.md:37.
The supervisor must still accept the contract against the observed evidence.

**Execution and provenance**

The `Issue 301 macOS evidence` workflow runs only for PR #303 on
`fix/issue-301`. Job `paired-watch-trace` (`issue-301 paired macOS trace`) checks
out the exact PR head and records its SHA separately from the fixed,
uncorrected source revision `c841d36fe5f650ba3352c89724df59a2509e8f0a`.
The run URL, attempt, numeric job ID/URL, OS image, kernel, Python, Rust and
Cargo versions, source hashes, command arrays, working directories, selected
environment variables, timestamps, timeouts and process exit statuses are
retained. `jobs.json` comes from the attempt-specific Actions jobs endpoint;
`jobs-api.exit` and `jobs-api.stderr` preserve access failures.

Fresh sources, test temporary directories and explicit Cargo targets live
under `~/ag/tmp`. Rust and Cargo 1.82.0 are checked and recorded from both the
baseline and instrumented source directories. The host triple parsed from the
baseline's `rustc -Vv` scopes the online cache preparation:

```text
cargo fetch --locked --target <recorded-host-triple>
```

This keeps the pinned lockfile while excluding locked dependencies for
unobserved targets whose manifests require a newer Cargo. Every build and test
continues to use Rust/Cargo 1.82.0 and runs offline; no second toolchain, shared
cache, or earlier CI job is used. Exact toolchain and fetch command records are
retained. The workflow allows 45 minutes, collection allows 35, setup/build
commands allow 15 each, and each baseline execution allows 3 minutes. An
interrupted job with no final `result.json` is incomplete, never a reproduction
verdict.

**Bound and observations**

There are exactly 12 unchanged baseline attempts, followed by exactly 12
paired diagnostic attempts, unless a setup failure/timeout interrupts
collection. Success or failure does not shorten or extend either bound.
All completed attempts and partial raw files remain in the artifact.

Baseline execution uses the pinned, uninstrumented source and the existing
test, with its original assertions and timing:

```text
cargo test --offline --locked --package rhei-e2e-tests --test e2e \
  --target-dir <scratch>/baseline-target \
  export_prior_migration_implementation_tests::omitted_validate_watch_renders_copyable_migration_help \
  -- --exact --nocapture --test-threads=1
```

Each Cargo invocation and raw stream is retained. Only exit 101 accompanied
by the reported recovery assertion and `left: 2, right: 1` is classified as
`reported_assertion_failure`; a passing baseline stays `pass`. A different
failure is recorded as `other_failure` and prevents confirmation.

The diagnostic source is a separate archive of that same baseline. Only this
copy receives `watch-trace.patch` and the copied `trace.rs` helper. The
original `eprintln!("{err:?}")`, logical filtering, debounce decisions and
watch registration behavior are preserved. Instrumentation records:

- Every successful root registration, including later registrations.
- Every processed event's kind, paths and actual filter result, both in the
  main receive loop and inside debounce. The accepted receive event's sequence
  number identifies the event that admitted each later pass.
- Pass IDs, begin/end timestamps and stderr file lengths at those boundaries.
  Analysis slices the actual stderr byte ranges into `pass-NNN.stderr` and
  associates their complete physical help lines with each pass and event.

Both capture cases extract the original plan and state-machine constants and
long directory name from the baseline test. They retain omitted-target
discovery, the plan working directory, isolated `.home/state`, and the test's
removal of forced-color variables. Fixtures have equivalent layouts in fresh
sibling directories (`a`/`b`); pair order alternates to reduce order bias.
Only stderr placement differs in command setup: beside the plan or in its
outer temporary root. Trace and stdout are always outside all registered
roots, verified against the recorded registrations. Before/after contents and
hashes of both authored files are retained.

The diagnostic copies the test's 25 ms poll and 10 s deadline and saves exactly
the first buffer containing the command as `poll-snapshot.stderr`. It then
observes for **one additional second**, stops/reaps the child, and separately
saves the full stderr. This extended observation is not an unchanged E2E
execution and cannot establish that a second pass occurred before the test's
snapshot. Both snapshots are checked for the exact complete shell-quoted help
line and exactly one command occurrence, without joining wrapped lines.

File metadata/trace writes add measurement overhead. Events still queued at
termination are not processed or reported. An incomplete pass, bytes outside
pass intervals, missing roots/events, premature exit, or trace contamination
is a measurement failure. These limitations and a bounded non-reproduction
must not be described as proof of absence or as a fix.

**Outcomes and retrieval**

| Diagnostic exit | Meaning |
| --- | --- |
| 0 | The unchanged baseline reproduced the reported assertion, at least one pair had extra in-root passes with one complete command per pass, all outside-root observations had one pass, and measurement checks found no contradiction. Raw admitting events still require supervisor cause review. |
| 2 | Setup, access, unrelated baseline failure, timeout, or incomplete/invalid measurement. Partial evidence is retained. |
| 3 | No baseline reproduction or no paired location-dependent support within the declared bound. Read both outcome files; neither is a passing regression claim. |
| 4 | Contrary evidence: malformed/duplicate help within a pass, changed authored inputs, or extra outside-root passes. The proposal needs review. |

Codes 2–4 make this separate diagnostic job fail. Exit 0 means the diagnostic
observed evidence, not that the product fix passed. No outcome is a shipping
verdict; the required Linux/macOS/Windows CI and supervisor acceptance remain.

Artifacts upload with `always()`, including after a failed command, as
`issue-301-macos-<run-id>-<attempt>`, retained for 30 days. `summary.txt` and
`result.json` are the compact entry points. `baseline-*.stdout/.stderr`,
`baseline-outcomes.json`, and each `pair-NN/<mode>/` directory are the raw
evidence. `paired-outcomes.json` includes exact help lines and admitting events.
If setup was interrupted, read the last `*.command.json` and available streams;
missing final outcomes cannot be inferred from the job color.

After the exit hook publishes the preparation commit, the supervisor verifies
the remote head, finds this workflow for that SHA (not an earlier normal CI
run), and downloads the matching artifact into runtime:

```text
gh run list --repo agent-grounds/rhei --workflow issue-301-macos-evidence.yml --branch fix/issue-301
gh run view <run-id> --repo agent-grounds/rhei --json headSha,jobs,attempt,url
gh run download <run-id> --repo agent-grounds/rhei \
  --name issue-301-macos-<run-id>-<attempt> --dir <runtime-evidence-directory>
```

No run ID or outcome exists until publication and hosted execution. This
preparation step must not dispatch, wait for, or read those test results.
