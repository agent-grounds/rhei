# FS-rhei-provider-refusal-rationale: Why Provider Refusal Recognition Is Closed

This explanation accompanies the recognition contract in §FS-rhei-agents.2.3.
The contract owns the accepted grammars, provider set, and event decoding.

**The provider set is closed**: `openai` and `anthropic` are the whole of it,
and a provider joins it by a change to the specification rather than to a
project's configuration. The set is what carries the claim being made — that
this line names a reset instant worth sleeping on. A registry id cannot carry
that claim, because an entry may be named anything and wrap anything
(§FS-rhei-agents.2.1); an entry named `cld1` resolving `anthropic` is therefore
recognized, and one named `codex` resolving `acme` is not.

**The time-of-day grammar's period vocabulary is closed on the same terms.**
`session` and `weekly` are the whole of that vocabulary, and a
third period word joins it by a change to the specification rather than by a
project's configuration — the rule the provider set is already held to. An open
token slot would rest recognition on the provider set and a sentence shape
alone, so a recognized provider printing `You've hit your disk limit · resets
6am (Europe/Zurich)` would park. A **dated** reset — `resets Oct 1, 5:59am`, the
yearless Claude form a reset further than a day out takes — is deliberately
not recognized either: it would have to infer a year the line does not print.
The Codex grammar in §FS-rhei-agents.2.3 supplies its year explicitly. Unknown
period words and yearless Claude dated resets stay ordinary process results;
neither grammar is configurable through project settings.

Two cases stay ordinary process results whatever the invocation printed. A
resolved provider **outside the set**, including one differing only in case: a
profile written `"provider": "Anthropic"` does not park. And a state that
resolves an agent but **no provider at all** — a bare `agent:` with no model
profile and no `<provider>` in its target (§FS-rhei-agents.1.4). Recognition
keys on the resolved provider, so there is nothing to match and no execution
identity to key a wait on, and the invocation is routed as the agent failure it
appears to be. Program states resolve no agent and never park.
