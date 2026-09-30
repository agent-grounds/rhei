# Changelog

*Every pull request adds a bullet under `## Unreleased`. Do not write the
pull request number: the release stamps `(PR #N)` onto it. See
[CONTRIBUTING.md](../CONTRIBUTING.md).*

## Unreleased

- Say whose ancestor a nested run inherits, and pin it. An ancestry descriptor
  named *which* reservation and never *whose*, so a `rhei run` charging a
  different project's journal looked the name up where it was never recorded and
  refused every admission it reached — which is every run of rhei's own suite
  from inside an agent, and every `rhei run` of a plan outside the exporting
  agent's project. The descriptor now carries the account that minted it
  (`RHEI_BUDGET_PARENT_ACCOUNT` beside the existing
  `RHEI_BUDGET_PARENT_RESERVATION`), identity is tested before the journal is
  consulted, a run whose account is not the ancestor's is admitted against its
  own account and says so once per run — an `info` note, not a warning, naming
  the other project by the directory this machine has witnessed for it and by
  its account uuid where it has witnessed none — and a refusal names the
  variable the value came from. The receipt such a run writes records
  `parent_reservation: null`, so replay keeps reading the chain as sound rather
  than as corrupt. Inside one account nothing moves: forging a name still buys
  nothing, a same-project child is still placed under its ancestor and still
  bounded by its envelope, and a descriptor with no account beside it is taken as
  an ancestor exactly as before, so a parent too old to name its account loses
  nothing. §FS-rhei-budgets.7.1 and a new §FS-rhei-budgets.7.2 say so, and
  §AR-neural-admission.6 scopes "anything else is `ancestor_unavailable`" to a
  descriptor of this project.

- Recognize a Claude weekly limit, and a reset that names no minutes, as the
  same provider refusal a session limit already is. `You've hit your weekly
  limit · resets 2pm (Europe/Zurich)` was an ordinary failure — the attempt was
  charged, no wait was recorded, and two refusals spent a visit's budget for a
  limit that clears on its own — while the identical line saying `session`
  parked. A provider limit is by construction something every concurrent task
  hits at once, so one weekly limit halted every ticket in flight in a
  non-terminal state, with recovery by hand. The recognized sentence is now
  `You've hit your <period> limit · resets <h>[:<mm>]<am|pm> (<zone>)`, and an
  absent `:<mm>` means `00` through the same arithmetic, so `resets 6am` and
  `resets 6:00am` are the same instant to the byte. Two things stay refused, now
  as recorded decisions rather than as accidents of the sentence: the period
  vocabulary is closed at `session` and `weekly`, a third word joining it by a
  change to the specification exactly as a provider does, and a dated reset
  (`resets Oct 1, 5:59am`) does not park, because it would have to infer a year
  the line does not print. §FS-rhei-agents.2.3

- Stop the end-to-end suite writing into the checkout it is run from. One test
  wanted the repository as its working directory and said nothing about where the
  binary's state should go, and the helper it called derived the spawned `rhei`'s
  `HOME` and `XDG_STATE_HOME` from that working directory — so a full
  `cargo test --workspace --all-targets` left an untracked
  `.home/state/rhei/root-guards/<sha>.lock` at the checkout root, and one more
  file there per run, because the lock is named for a project root that is a
  fresh temporary directory each time. Every gate that reads the working tree —
  this repository's own pre-commit and pre-push hooks among them — then saw the
  checkout as modified when it was not, and an artifact under a dot directory
  nobody wrote cost whoever found it the work of proving their own command had
  not written it. §REQ-test-isolation now says that a test writes only inside its
  own directory, that the checkout is not one, and that where a spawned process
  writes is chosen rather than derived from where it stands.

- Let a machine name its price book once. `defaults.prices` in the machine or
  project settings file holds the path to a `rhei.accounting.prices.v1` book and
  supplies the default value of `rhei run --prices` — nothing more: the flag
  still wins, a selected book is still taken wholesale and still bypasses
  profile-book construction, and `rhei summary` is untouched. The key was
  accepted and silently dropped before, so a machine that wrote it ran on the
  built-in book's one `claude-sonnet-4-6` entry and recorded every other model
  `unpriced`. The path is fixed when settings merge — `~` expands, a relative
  path resolves beside the settings file that declared it — and opened only when
  a run prices, so `rhei roster`, `rhei validate` and `rhei list` keep working on
  a machine with a misspelled path and `roster` shows it verbatim with its tier.
  One accepted regression comes with it: because a selected book replaces the
  built-in one rather than composing with it, a book that omits
  `anthropic` / `claude-sonnet-4-6` makes Sonnet runs unpriced that are priced
  today. Add the entry to your own book.
  §FS-rhei-agents.1.1.1 §FS-rhei-agents.1.3 §FS-rhei-cost-accounting.5.1

- Refuse an unbounded agent state at the preflight, not one line into the run.
  `rhei validate` and `rhei run --dry-run` now resolve every non-gating,
  non-final state's effective agent invocations and reject one that resolves to
  no finite `agent_timeout` through `state.agent_timeout >
  models.<id>.agents.<agent>.timeout > agents.<id>.timeout >
  defaults.agent_timeout`, naming the state and the agent in the sentence
  `rhei run` already gave it — so the two commands run before an unattended
  launch stop predicting a spawn the launch refuses on its first pass. The one
  line the error names, `"defaults": {"agent_timeout": "30m"}` in
  `settings.json`, satisfies all three. The check resolves through the same
  function execution spawns through, per task identity, so a `**Target:**`
  override is judged as it would be spawned and a fan-out member by member;
  `rhei run --no-agent` resolves no invocation and stays exempt; and the real
  run's refusal moves from the first pass to admission, so the sentence is
  unchanged but what surrounds it is not: it now arrives inside a validation
  error rather than as a pass-1 diagnostic, the four places a timeout may be
  set ride along inside it as a parenthetical rather than on a `help:` line of
  their own, and no `runtime/run-report.md` is written for a refusal that
  happens before the first pass. The shipped `code-review` and `fix` templates
  gained an `agent_timeout` on each of their agent states — per state rather
  than through `defaults`, because that key's ownership is exclusive at mount
  time and two composed blocks owning it collide when `changeset-review` is
  instantiated.
  §FS-rhei-validate.4 §FS-rhei-agents.3.2.2 §FS-rhei-run.4

- Say what a plan with an unbounded agent state must satisfy, before anything
  is written to enforce it. §FS-rhei-agents.3.2.2 already called a state that
  resolves to no finite `agent_timeout` a validation error, but only the spawn
  path refused one, and both its call sites were behind `if !opts.dry_run()` —
  so `rhei validate` and `rhei run --dry-run`, the two commands run before an
  unattended launch, predicted a spawn the launch refused on its first pass.
  The rule is now stated where it can be checked: it ranges over every
  non-gating, non-final state that resolves to an agent invocation, all three
  commands refuse such a plan with the same sentence, `rhei run --no-agent`
  stays exempt, and §FS-rhei-validate.4 step 3 is where the refusal is decided.
  §FS-rhei-state-machine-writer.4 step 9 now says which half of its rule the
  engine checks and which stays the writer's own, and
  §REQ-bounded-neural-work.2 records the strain that level 1 is the one bound
  with no built-in default. End-to-end tests pin all of it, and a new guard
  instantiates every shipped template and validates it, which nothing did
  before.

- Say which of two things a reused path means, and give the operator a way to
  answer. A project laid down at a path another project used before it resolves
  to that project's account through the witness index, finds no journal of its
  own, and is refused as a rolled-back one — the sequence
  `examples/subtree-supervision/README.md` documents, on its second run. The
  refusal stays, because path reuse and a rolled-back journal leave identical
  state and the check exists for the second. What changes is everything the
  refusal did with it. Where the journal is **wholly absent** it no longer
  instructs a copy of the witness over it: that instruction succeeds, and hands
  a brand-new project the entire spend of the one that held the path before it,
  with nothing to say it happened. It now states both readings and names a
  runnable command for each. `rhei budget show`, the one diagnostic the refusal
  points at, works in the state it is offered for rather than failing to open
  an account it was asked to describe, reports the account's `health` as
  `damaged` with which of the three sub-cases it is — `journal_absent`,
  `journal_truncated`, `chain_broken` — and under `--format json` emits the
  machine-readable error object a harness can parse instead of rendered prose.
  Where a journal is present its tail is what was lost, so the truncated and
  chain-broken sub-cases keep the restore remedy unchanged. And `rhei budget
  forget <TARGET> --reason <TEXT>` retires a stale root under an audited
  receipt, keeping the receipts byte for byte under a `retired/` name and
  retracting the root's entry so the next admission establishes a new identity
  at zero consumed. It **refuses** an account whose journal verifies, which is
  what keeps it a recovery command rather than a way to reset a working
  balance: the residual §FS-rhei-budgets.5.3 always named is now a door with a
  name on it rather than an `rm -rf` nobody recorded. §FS-rhei-budgets.5.4
  splits the damaged remedy by sub-case, §FS-rhei-budgets.10 carries `forget`'s
  contract and the closed `health` vocabulary, §FS-rhei-errors.5 permits the
  named detail members that carry the facts, and §AR-neural-admission.4 no
  longer states the defect's cause as a plain fact. (PR #TBD)

- Wait for the run journal's own line before reading it. The eight-way
  provider-limit parking regression read `runtime/transitions.log` once, the
  instant the durable `nextAttemptAt:` waits became visible, and asserted that
  all eight `end@working … outcome=provider_limited` lines were already in it.
  They need not be: a parked invocation's wait and its retained spawn record are
  both observable before the release carrying its journal line is emitted, a
  window of a few milliseconds here and wide enough on a loaded required runner
  to fail the test having seen seven of eight. The observation now polls for the
  eighth line on the same patience and grid as the wait beside it, and names a
  run that died rather than reporting it as a wait that timed out.
  §FS-rhei-run-tui.1.7 states that ordering, which the engine already had, so an
  observer knows the line is owed. (PR #343)

- Give a test its own directory by construction rather than by luck. The test
  harnesses named a private directory from the wall clock alone, and
  `create_dir_all` succeeds on a directory that is already there, so two tests
  that started inside one clock tick shared a tree: each wrote its fixtures into
  the other's, and whichever finished first deleted the tree the other was still
  reading. The failure then surfaced somewhere else entirely — a cross-root
  identity conflict reported twice, or a member directory refused as a
  single-file plan — and it surfaced only where the clock reads coarsely, so a
  suite green on Linux went red on macOS. The name now carries a per-call
  sequence and the process id beside the clock reading, the rule the CLI already
  uses for the artifacts it publishes. The rule is a function that reads none of
  the three for itself, so the unit tests beside the harness hold two of them
  still and vary the third, once for each: dropping the clock reading, the
  process id or the sequence from the name fails a named test, rather than
  passing on whichever ingredient the platform happened to move. A name that
  leans on any one of them alone therefore cannot pass on one platform and fail
  on the next. (PR #344)

- Ask a contributor for a changelog bullet, not for a pull request number they
  cannot know yet. The `## Unreleased` check now requires at least one bullet
  that the section does not already hold at the branch's merge base with its
  base, and checks a written number only where a number is known and only on the
  bullets the branch itself added — so the pre-push hook runs it whether or not a
  pull request exists, and a first push learns the rule on the contributor's own
  machine instead of from a red CI run. Rewrapping a bullet somebody else wrote
  is not a bullet of your own, and neither is writing your number onto one. The
  release then stamps `(PR #N)` onto each bullet it can resolve to exactly one
  pull request, replacing a `(PR #TBD)` placeholder where it stands, leaving a
  bullet it cannot resolve as written with a warning, and never failing a release
  over one. Both halves of the check and the stamper read one definition of a
  bullet, in `scripts/changelog_bullets.py`, because two gates disagreeing about
  `docs/changelog.md` was the defect. Where the base is not named, it is the
  candidate — the push remote's `main`, `origin/main`, `upstream/main`, `main` —
  whose merge base with the branch is the most recent, so a fork's unsynced
  `origin/main` no longer makes the bullets a merge brought along read as the
  branch's own; where it is named, a base the checkout does not hold is refused
  by name instead of quietly checked less. A new `CONTRIBUTING.md` and pull request
  template say the rule up front, and `SKIP=changelog-pr-entry git push` is the
  documented way to push a branch that is not becoming a pull request.

- Let a workspace task file carry its own metadata. A file under `tasks/` may
  now open with a metadata-only frontmatter block holding
  `metadata.tasks.<id>` entries for the tasks it defines, so a task's custom
  field lives beside that task's `**State:**` rather than in the one file every
  other run also writes. The block and `index.rhei.md`'s entry for the same
  task are merged and **disjoint by key** — the index may hold `priority` while
  the task's own file holds `context`, and both reach `{meta.<key>}`, callback
  task metadata and `rhei render`; the same key in both places is a validation
  error naming both files and the key. A key in a task file names the task its
  own text spells, so `1.2` and `"1.2"` are one task while `1.10` is task
  `1.10` rather than the `1.1` its float form rounds to. Nothing new is written: `stateVisits`,
  `pollNextAttemptAt`, `providerLimits`, `budgetTicketId` and the supervision
  block still go to `index.rhei.md` whatever a task file carries, and `rhei
  reset` leaves an authored block byte-identical. What the block may not do is
  now refused by name instead of accepted and silently discarded: a top-level
  key other than `metadata`, an entry for a task another file defines, two of
  one file's keys spelling one id,
  malformed YAML, the same key as the index, a key under `metadata` other than
  `tasks`, a block in a `basin/` ticket file (the basin's metadata document is
  the project manifest), and a block in a mounted block's task file, which
  composition would have to rewrite ids inside. `rhei list` now skips a basin it
  cannot load and warns naming the ticket file, rather than failing the whole
  project as it did for any basin ticket error before. (PR #331)

- Read a fenced code block in a plan the way the language defines one, so a plan
  may quote the plan format. The structural scan used to decide what was code by
  flipping a flag on any line starting with three backticks: it kept no run
  length and did not know `~~~` at all, so a `~~~markdown` fence quoting
  `## Tasks` authored a real chapter, and a ``` ```console ``` line *closed* a
  four-backtick fence and let the headings after it author chapters too. Either
  way the plan's own `## Tasks` landed mid-document and the plan was refused with
  "Tasks section must be the final '##' chapter" — and because one unparseable
  plan fails the project load, `rhei validate <store>` and every `rhei run` over
  that store went down with it, however healthy the plans beside it were. Whether
  a plan was refused at all turned on how many inner fence lines a paste happened
  to carry, which made the failure look like a property of the text rather than of
  the reader. A fence is now one thing everywhere — the structural scan, the
  tokenizer, the link checker and the result-block scan read the same rule
  (§FS-rhei-plan-language.2.1): a run of three or more backticks or tildes, closed
  only by a bare run of the same character at least as long, and running to the end
  of the file when nothing closes it. Two consequences are worth knowing: a bare
  `~~~` line now opens a fence where it used to open nothing, and a run carrying an
  info string never closes one. (PR #337)

- Carry each task's authored frontmatter metadata on `rhei list --json`, so a
  caller reads one custom field off the query surface instead of re-deriving
  rhei's id keying. The `metadata` object is the task's `metadata.tasks.<id>`
  map, keyed by the qualified id the listing prints and spelled exactly as the
  author wrote it, and it is omitted rather than empty where there is nothing to
  publish. What rhei writes into the same map stays out of it: the keys rhei
  writes are now named once in a register the specification carries, which is
  also what `rhei reset` reads to decide what it deletes. `rhei render --format
  json`'s top-level `frontmatter` key is documented for the first time — the
  whole parsed document, deliberately unfiltered — and a YAML value JSON cannot
  hold is now named on both surfaces, every one of them in one run, where a
  non-scalar key used to drop the entire document and a non-finite float used to
  become `null`. (PR #330)

## 2. [0.5.1] - 2026-09-28

- Let an `agents.<id>` profile name the built-in family it belongs to. A wrapper
  around a coding agent — a second account, a proxy, a `nix run`, a sandbox — is
  an entry under an id of its own, and every capability rhei selects per
  built-in agent used to be keyed on that id, so a wrapped Claude Code ran and
  was never measured: no `runtime/accounting/`, nothing for `rhei summary` to
  count, and a session log nothing had asked to structure. Write
  `"family": "claude-code"` and the profile is that built-in with its own fields
  laid over it: it inherits every field it does not write, and rhei reads the
  resolved family for the usage extractor and the launch arguments that
  extractor requires, for whether a record is written at all, for the
  stream-json stdin transport, and for which session-report stream the log is.
  The record gains optional `agent_family` beside `agent`, which is still always
  the profile's own id, and the agent log gains a `family:` line when the two
  differ. `rhei roster` reports such a profile resolved; a family that is not a
  built-in is a settings error naming the six that are, and a key that is not a
  field is now a warning rather than silence. A profile that declares no family
  is unchanged in every respect. (PR #335)

- Park a session limit for any provider Rhei recognizes, not only
  `codex`/`openai`. Recognition now keys on the resolved provider in a closed
  set — `openai` and `anthropic` — and the agent entry's name no longer decides
  it: an entry a machine calls `cld1` resolving `anthropic` parks on the same
  terms as the built-in `codex` entry on `openai`, so its refusal writes a
  `providerLimits` record with a `nextAttemptAt`, costs the visit none of its
  `attempts:` budget, and resumes at the reset without a person. Before this,
  the identical reset-bearing line from a Claude Code invocation was an ordinary
  failure: two of them spent a state visit's whole budget and the ticket halted
  until someone forced a transition. Nothing needs declaring — the set is closed
  and a provider joins it by a change to the specification rather than to a
  project's configuration — and a state that resolves an agent but no provider
  still does not park. (PR #334)

- Read a plan where the account's state directory is not writable. A root guard
  keeps its lock outside the project, in the account's state directory, so that
  reading a read-only project needs no file inside it; where that directory
  cannot hold the lock either, every command that takes a shared root guard —
  the readers `rhei validate`, `list` and `render` and the viz and dashboard
  readers, and the commands that claim or move a task, `rhei next`,
  `transition`, `complete` and `run` alike — refused before it read the first
  byte of the plan. A shared acquisition now proceeds without the lock, giving
  up only mutual exclusion between rhei processes on that root and saying so in
  one `warning:` line on stderr per root; stdout and the exit status are
  unchanged. The recovery-marker interlock is untouched: a root that holds
  `.rhei/forced-recovery.json` is refused exactly as before, on the same
  machine. Exclusive acquisition never degrades, so `rhei recover` and a forced
  `rhei transition` still refuse there — but the refusal, and the warning, now
  name the lock path, the execution root, the underlying error and
  `XDG_STATE_HOME` as the lever, instead of a bare `Permission denied (os error
  13)` that read like a defect in the plan being validated. Nothing changes on a
  machine whose state directory is writable. (PR #329)

- Fire the edge an exit code selected — a program's, and an agent's whose poll
  budget ran out — whichever order the edges for that pair are declared in, and
  run that edge's callbacks. A `(from, to)` pair does not name a rule, so where a
  state declared two edges to one target — the
  ordinary shape of a poll state, a condition-only exhaustion edge beside an
  exit-coded one — the engine resolved the pair again after choosing and applied
  whichever rule came first in `states.yaml`. A conditional edge declared ahead
  of an exit-coded one made the target unreachable by exit code and halted the
  run, and where the leading edge's condition was met it quietly ran the wrong
  `on_enter`. `rhei transition`, `rhei complete`, and an `on_leave` `nextState`
  redirect are unchanged: each names only a state pair, so the pair's first
  declared exact rule still governs, and a refusal now says so rather than
  listing the target it just refused as somewhere to go instead. (PR #328)

- Add `rhei show <ticket>`, the read that prints one task's body without the
  document around it. It prints one `## Task <id>: <title>` heading, a blank
  line, and the body as stored — an appended lifecycle record included — and
  nothing else; `--json` emits one object with exactly `id`, `title`, and
  `content`. Any ticket resolves in any state, because reading is not claiming:
  the ticket nobody may claim was the one read rhei had no verb for, and
  reaching it meant rendering the whole plan. The positional takes a ticket id
  or a plan with `--task`, resolving the way `rhei complete`'s does, and
  `rhei next`'s ready-set refusal now points at `rhei show <ticket>` rather than
  at `rhei list`, which prints no body at all. (PR #327)
- Say which spelling keeps a `rhei new` description that begins with `-`. A
  bullet list is the ordinary shape of a ticket body, and `--description
  '- Context: x.'` is taken for another flag and refused during argument
  parsing. The value stays refused and nothing is written — accepting it would
  accept a following flag as the body too — but the parser's advice to pass it
  after a bare `--` is written for a positional and fails again when it is
  followed, so the refusal now replaces that advice in its own slot with the
  spelling that works: `--description=<text>`, or `--description-file -` for a
  body read from standard input. A hyphen-leading path to `--description-file`
  gets the attached `--description-file=<path>` form, since a bare `-` there
  already means standard input. A hyphen-leading `TITLE` renders identically to
  a refused option value, so the two are told apart by the command line rather
  than by the message, and a title keeps exactly the refusal it printed before.
  The new tip is printed only where the option's own value is what the parser
  refused, judged by the name the parser gives a token rather than by the token
  itself — a long option truncated at its first `=`, a short cluster cut to its
  first character — and only where no earlier token carries that same name, since
  the parser stops at the first token it cannot take. So a doubled letter in
  `--kindd=task`, an unknown short cluster, an unknown flag elsewhere on the
  line, and a command that declares no such option at all all keep the parser's
  own message, spelling suggestions and all. (PR #326)

- Bound a project's measured spend by default. Every project may be charged at
  most $400.00 of provider cost per UTC day, a built-in measured from the
  accounting archive the same way the two counts were, raised for a machine
  with `defaults.spend_per_day` or lowered by a project. The amount is charged
  from the cost accounting record an invocation already produces: each spawn
  reserves a flat $20.00 worst case before the request, and the settle replaces
  it with what the request actually cost, so the day carries the measured
  amount rather than the estimate. An invocation nobody could price, and one
  that produced no accounting record at all, are charged that worst case rather
  than nothing, and every surface says how much of the day was estimated rather
  than measured. Nothing needs declaring: a plan with no spend field runs under
  the built-in, and `spend_per_day` is not a plan or profile field at all. When
  the day is spent the ticket stops exactly where it stands with its artifacts
  intact, and the halt names the dimension, the numbers in the account's
  currency, and the instant the day renews. `rhei validate`, `rhei budget show`,
  the run report, the TUI and `--json` all carry the third dimension. (PR #322)

- Bound a ticket's travel and a project's invocations by default. Every ticket
  may make at most 80 applied moves over the lifetime of its identity, and every
  project may be admitted at most 200 agent starts per UTC day; both numbers are
  built-in defaults measured from real run records, both are visible with their
  source before the first agent starts, and both are raised by one machine
  settings key or lowered by a project, a plan, or a profile. A machine's value
  is a ceiling: a plan that asks for more still validates, gets the machine's
  number, and is told so. Nothing that ran before is refused — no plan needs a
  new field and no machine needs a new file. When a bound is spent the ticket
  stops exactly where it stands with its artifacts intact, the run carries on
  with other work, and it exits non-zero naming the bound, the numbers, and the
  one thing that would raise it. A ticket's identity is resolved from the
  project's own ledger where its plan does not carry one, so a lost or
  hand-deleted `budgetTicketId` recovers the travel it already spent instead of
  buying a second bound. (PR #315)

- Add `rhei migrate export-priors` as the explicit recovery for plans laid
  before consumed-export producers had to be direct dependencies. Validate and
  run refusals now name the copyable migration command; migration previews or
  writes only the required authored `Prior` diff, while newly authored plans
  remain subject to the same strict rule. (PR #294)
- Let block authors expose selected state, task, agent, model, MCP-server, and
  skill identities under stable public keys. Mounted parents can use only that
  typed surface, wrappers must explicitly re-expose it at each boundary, and
  blocks without exposure declarations keep their existing private identities.
  (PR #295)

- Compile block graphs over shared state and task types, retain compiled settings
  and support files without rendering twice, and restore extracted review/fix
  stage instructions and artifact contracts. Keep the built-in bundles loadable,
  preserve counted task states and registry-specific setting ownership, and retain
  endpoint sources and input help in composition diagnostics. Preserve cancellation
  with explicit flat-state roles and scoped wildcard sources; restore omitted
  preparation/commit stages through input-selected declarations and share checked
  equivalent review/fix terminals under the wrapper's public names. (PR #282)

### Added

- **`rhei run --until-idle` does the available work and then hands the process
  back.** Where a continuous run would sleep for a human gate, a future poll
  deadline or a recognized provider limit, a selected run performs the same
  final admission checkpoint and returns instead: it exits `3`, names the
  dominant kind of wait, and reports the earliest instant a next invocation
  could find work on the console, in the durable report and in
  `run_finished.summary.stop`. A timer can drive a plan without holding a
  process. The option implies line output and is refused beside `--tui` or
  `--headless`. One judgment of "is this ticket deliberately waiting" answers
  for both modes, so five readings it had wrong are corrected for the continuous
  run as well. A ticket that is both polled and claimed is now classified by the
  claim, so the run says `rhei release` instead of calling it a timed retry. A
  ticket held by a supervisor parked at a human gate is now classified by that
  gate, so a plan waiting on nothing else ends as quietly as any other gated
  one. A supervisor is classified by its own gate and its own `**Assignee:**`
  before its subtree, because it is ready while that subtree is open, so a
  claimed supervisor now halts naming `rhei release` where it used to end
  quietly on whatever waited beneath it. A plan whose only remaining work is a
  recognized provider limit now ends quietly instead of halting. And a
  continuous run no longer sleeps for a deadline whose expiry could release
  nothing: it reaches the same halt without the wait. (PR #316)

- **Generated block compositions now carry durable per-node provenance.** A
  canonical `.agent-grounds/rhei/composition.lock.json` traces every flattened
  state, task, profile, and routing rule through its declaration and mount to
  shipped, Git, local, or unavailable source metadata. Runtime commands keep
  consuming the ordinary flat workspace, and exact replay is promised only
  when the recorded source identity denotes immutable content. (PR #298)

- **Attended operators can recover a missing state-machine edge with `transition
  --force --reason`.** Fresh confirmation, preserved transition safeguards and a
  durable exceptional audit pair make the correction recoverable with `rhei
  recover`, including basin tasks and their shared project metadata; pending
  recovery blocks readers, writers and headless startup. (PR #297)

- **Self-looping agent states can continue their immediately preceding visit
  with `session: continue`.** The runtime selects the exact current `_state`
  snapshot for the same task, state, prior visit, and target, reuses native
  preload and lineage handling, and explains every cold fallback without
  relaxing named snapshot contracts. Newly instantiated `supervised-delivery`
  supervisors opt in automatically; delete the removed `supervisor_session`
  key from old values files. Existing workspaces are not rewritten, and the new
  syntax needs a supporting Rhei release or installed pin because older
  binaries ignore it. Foundation-template rollout remains tracked in
  agent-grounds/agent-grounds#5 after release/pin validation. (PR #293)

- **Tasks can explicitly continue a declared predecessor's native session.**
  State machines accept `snapshot.inherit.from: prior`, while task
  `**Inherits:**` metadata and `rhei new --inherits` provide per-ticket opt-in,
  overlay, and `none` opt-out controls without exposing arbitrary task
  addressing or changing artifact handoffs. (PR #290)

- **`rhei instantiate` now composes reusable, parameterized blocks into one
  ordinary workspace.** Direct `--mount` composition and recursive manifest
  `use` qualify owned states, tasks, profiles, settings, prompts, and runtime
  paths; completion seams preserve block-local fan-out and can pass declared
  state files or task exports. Existing single-template commands keep their
  visible identities, and the bundled review and fix blocks can be mounted
  independently. Catalog UX, a replacement authoring language, richer
  provenance/exposure modes, and conditional seam expressions remain deferred
  follow-ups. (PR #282)

- **Task exports are now checked handoffs instead of best-effort prompt
  context.** Validation requires every consumed name to be declared by a
  directly listed prior, agent startup batches missing or blank consumed
  exports, and successful producer completion refuses missing or blank
  declarations. Existing plans must add the producer directly to `**Prior:**`
  and publish nonblank files before the handoff runs. (PR #278)

- **Agent sessions now render readable Markdown reports, and workspaces can
  declare metrics whose trajectories Rhei records.** Every agent session log
  renders to `runtime/reports/<log stem>.md` — automatically at session end
  and via the new `rhei report` command — with the full prompt, structured
  tool calls, files produced, and outcome; tool outputs truncate at 10 KiB
  unless `--full`. A validated top-level `metrics:` mapping in `states.yaml`
  makes the engine bind each successful measurement, confirmed by
  boundary-artifact existence, to the sessions in its window as append-only
  records under `runtime/metrics/`, rendered into a metrics summary and
  per-session strips; measuring program states receive `RHEI_ITERATION`.
  (PR #283)

- **Claude Code and Codex sessions now render readable reports too.** Session
  reports read the Claude Code `stream-json` and Codex `--json` streams beside
  Pi's, and a log with no event stream renders verbatim as session output
  instead of an empty report. Claude Code launches now request `stream-json`
  output so their logs carry every tool call; the live display shows the
  assistant's text rather than raw JSON. Report paths read relative to the
  session's root, metric trajectories name sessions by visit (`cover #2`), and
  the agent prompt warns that headings in a task body break the plan.
  (PR #311)

- **Tasks can declare explicit read exclusions for blind work.** `**Excludes:**`
  accepts checkout paths, runtime files or directories, and declared exports;
  validation rejects malformed, escaping, duplicate, unresolved, or
  required-input conflicts. Rhei filters matching payloads from serial,
  parallel, retry, fan-out, and `rhei next` context while retaining navigation.
  Built-in profiles are composition-only; custom profiles may declare a
  `deny_read.path_flag` adapter for process-tree enforcement, preserving exact
  file versus recursive directory semantics for future paths. Selected prompt
  templates and required artifacts across visits and execution identities
  cannot be excluded. (PR #274)

- **Per-session reports and declared metrics are now specified.**
  §FS-rhei-session-reports defines one readable Markdown report rendered from
  each agent session log under `runtime/reports/`, and §FS-rhei-metrics defines
  the optional `metrics:` declaration in `states.yaml` with engine-recorded
  iteration bindings under `runtime/metrics/`. Documentation only; no runtime
  behavior changes in this entry. (PR #272)

- **Plans that declare `Consumes` now receive a non-fatal validation and run
  advisory that the field selects export prompt injection, not filesystem
  visibility.** Run frontends retain the warning in their native output, and
  authoring guidance now explains that workers can still read undeclared
  sibling exports. Scripts that consume successful validation or run output
  for such plans must allow the additive warning. (PR #269)

- **`rhei complete` now accepts result messages from a file or standard
  input.** `--result-file <PATH>` safely carries multiline or Markdown-heavy
  text, with `-` selecting stdin, while the existing inline `--result` form
  and its literal dash behavior remain unchanged. (PR #270)

- **The new `rhei roster [RHEI_PLAN] [--json]` command prints the effective
  agent, model, and defaults registry.** Its machine-readable form includes
  source metadata and per-merge-unit provenance, so dispatchers can use Rhei's
  built-ins and precedence rules without copying or reimplementing them. (PR #271)

- **`rhei run` now parks supported Codex/OpenAI session limits and resumes
  automatically.** A reset-bearing refusal keeps the task in its authored
  state, releases its worker slot, persists the safe retry deadline across
  interruption or restart, and leaves foreground/headless runs waiting while
  unrelated work continues. Journals and JSON streams expose the additive
  `provider_limited` outcome instead of reporting an ordinary agent failure.
  (PR #277)

- **States can select reasoning effort independently of their agent target,
  model, and permission mode.** Built-in and custom supporting profiles map the
  canonical effort to native arguments, while unsupported profiles ignore it;
  an existing authored `effort` key that was previously ignored now takes
  effect or fails validation when its value is invalid. (PR #276)

- **Unrestricted project runs now admit members instantiated while the run is
  live.** A validated member is published atomically, initialized with its own
  machine, callbacks, execution root, lock, and accounting context, then enters
  the existing sequential or parallel scheduler without starting another run.
  Publication refuses a destination created during staging. Runs narrowed with
  `--rhei` keep their startup candidate set fixed. (PR #275)

- **Model profiles can now declare their static accounting rates.** Selecting
  a priced profile creates one deterministic effective book before execution,
  persists current and immutable archived snapshots in every participating
  root, and records the selected profile with each new invocation. Explicit
  `--prices` books still take precedence, while unpriced profiles retain the
  existing built-in fallback. (PR #291)
- **Completed runs can be summarized against a later price book without
  changing their recorded accounting.** Paired `rhei summary --run <ID>
  --prices <BOOK>` inputs select one exact finished run, keep ordinary summary
  and cost defaults unchanged, and show priced, partial-price lower-bound, or
  unpriced results with the selected book's provenance. (PR #292)

- **Transition callbacks now receive a stable identity for each firing and an
  explicit pending ledger status.** Canonical JSON exposes `firingId` and
  `ledgerStatus`, and CLI callbacks receive equal environment values, so an
  `on_enter` integration can include the in-progress transition without
  inferring it from file-write timing. Both ledger formats remain unchanged;
  strict callback JSON decoders must permit additive fields. (PR #262)

### Fixed

- **Live runs now validate every scheduling reread before appended work can
  execute.** Tasks added inside an existing Panta member can no longer bypass
  startup declaration and execution-reference checks merely because the
  member set stayed unchanged; malformed appends stop through the existing
  diagnostic and explicit migration help, while valid appends still run in
  the same invocation. (PR #308)

- **Re-entered agent states now run each invocation again before advancing.**
  Manual and automatic loop-backs no longer reuse an earlier visit's static
  outputs or required result, while deliberately pre-seeded first visits and
  successful same-visit restarts retain their existing reuse behavior. (PR #304)

- **Windows claim and provider-parking regressions now wait for witnessed
  synchronization events and all eight durable provider waits.** Slow required
  runners no longer fail otherwise-correct behavior on fixed observation
  clocks, while genuine stalls retain bounded diagnostics. (PR #302)

- **Watch migration-help E2E coverage now validates each complete failed pass
  independently.** Ordered output and controlled input changes permit multiple
  legitimate revalidation passes while still rejecting duplicate, missing,
  wrapped, or wrong-target recovery commands within any pass. (PR #303)

- **Exact transitions from final states now fail machine loading.** Terminal
  exits require attended operator recovery; ordinary wildcard behavior is
  unchanged. (PR #297)

- **Explicit `rhei next --task` claims no longer reject ready non-initial work
  as a concurrent-writer conflict.** A ready passive state may advance one
  applicable non-terminal edge while state, ownership, metadata, callbacks,
  and ledger entry remain in the existing atomic claim transaction. Automatic
  selection still considers initial-state work only. (PR #288)

- **Run-level `--agent` and `--model` overrides now compose with ordinary
  explicit state and task targets instead of being silently ignored.** Each
  flag replaces only its named identity dimension, preserving the target's
  provider and optional mode; incompatible composed identities are refused
  before spawn. Timeout callbacks retain those overrides in serial and parallel
  runs. Selector fanout keeps its authored identities. (PR #289)

- **Plan rewrites now use their permanent sibling sidecar as the sole writer
  lock.** The replaceable plan pathname stays readable through callbacks and
  atomic replacement on Windows, while the sidecar remains held through
  commit or rollback. Creation establishes the same identity before first
  publication; failed creates and dry runs retain empty sidecars and necessary
  parent directories while rolling back plan data. Before upgrading a shared
  plan directory, stop every older writer, upgrade them all, then resume; live
  mixed-version writing is unsupported. (PR #279)

- **Resumed program polls now invoke their next subprocess attempt after the
  persisted retry deadline.** A future deadline no longer sends the run into
  callback-only mode, where an exit-zero route could fire without a matching
  program exit; ordinary readiness still prevents an early spawn. (PR #266)

- **Structured accounting extraction failures now explain why capture parsing
  failed.** Capture and invocation records preserve concise diagnostics, and
  `rhei cost --task` exposes them in both text and JSON without quoting raw
  agent output. (PR #267)

- **Plan commands now classify existing directories without a Rhei manifest.**
  The diagnostic identifies the path as neither a Panta Project nor a Directory
  Workspace, presents `index.panta.md` and `index.rhei.md` as alternatives, and
  points `rhei run DIRECTORY --rhei ID` at a recognized `DIRECTORY/ID` workspace
  instead of suggesting permissions or free-space checks. (PR #268)

## 3. Older releases

- [0.5.0](changelog/0.5.0.md) - 2026-09-14: - **Price books now accept and preserve the extension metadata their published v1 schema permits.** Additional document and entry properties retain their JSON values in every participating run-owned copy without affecting price selection, validation, coverage, or calculated amounts.
- [0.4.1](changelog/0.4.1.md) - 2026-09-07: - **Parallel refills preserve each task's requested execution identity.** When a freed `--parallel` slot schedules newly ready work, the reloaded task's full `**Target:**` override now continues to select its agent, mode, provider, and model instead of silently falling back to the state's target.
- [0.4.0](changelog/0.4.0.md) - 2026-09-05: - **The fissile config lives at `.agent-grounds/fissile.toml`, where an agent working in the repository can still reach it.** `.agents/` is where agent *instructions* live, and a managed permission profile mounts it read-only inside a checkout, so an agent that hit a size finding could not adjust a budget or record an exception without leaving its sandbox — tool config had ended up in the one directory it was least able to repair.
- [0.3.3](changelog/0.3.3.md) - 2026-08-31: - **`dir_template` can now name a per-working-directory session store.** A `FlatById` layout's `dir_template` may contain the placeholder `{cwd_dashed}`, which expands to this spawn's own canonicalized working directory with every character outside `[A-Za-z0-9-]` replaced by `-` — the convention Claude Code uses for its own per-project session directories — so a template like `~/.claude/projects/{cwd_dashed}` names the directory a supervised checkout actually writes into, instead of one literal path shared across every checkout.
- [0.3.2](changelog/0.3.2.md) - 2026-08-30: - **The re-spawn note on a poll state names its own `poll.max_attempts` instead of an internal sentinel.** A poll state is exempt from the visit attempt budget — `poll.max_attempts` already bounds it — and that exemption was encoded internally as `u64::MAX`, which `rhei run` then printed verbatim: `attempt 4 of 18446744073709551615`.
- [0.3.1](changelog/0.3.1.md) - 2026-08-30: - **The release commit stages `xtask/Cargo.toml`.** `Auto bump` had failed on its last three runs, always at `release.yml`'s version check and always before the publish step, with `xtask/Cargo.toml internal dependency requirement is stale: ...
- [0.3.0](changelog/0.3.0.md) - 2026-08-23: - Give a cold invocation the project's **mid-term memory**.
- [0.2.0](changelog/0.2.0.md) - 2026-08-22: - Separate a run from the surface that watches it.
- [0.1.0](changelog/0.1.0.md) - 2026-05-21: - Initial alpha release line for the Rhei CLI, Rust crates, npm wrappers, and PyPI wrappers.
