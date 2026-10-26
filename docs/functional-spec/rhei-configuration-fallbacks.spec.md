# FS-rhei-configuration-fallbacks: Codex Configuration Fallbacks

What [§FS-rhei-agents.2.4](rhei-agents.spec.md#24-configuration-fallbacks) names: when a Codex worker's output counts as a
configuration fallback, and what Rhei does with one.

A Codex worker can be told after it starts that a setting Rhei asked for is not
allowed — a managed workspace's requirements refuse `danger-full-access`, say —
and carry on with a weaker one, exit `0`, and write nothing. All Rhei would see
then is the missing outputs. So Rhei recognizes a **configuration fallback** in
the worker's output and keeps it as a diagnostic of that invocation.

Recognition requires all of the following:

- the resolved family is `codex` — the declared `family`, or the profile's own
  id when it declares none. Neither the provider nor the registry id is tested,
  and no other family's output is read for it;
- one captured stdout line is a single JSON object that is either a top-level
  event with `type: "error"` and a textual `message`, or an `item.completed`
  event whose `item` has `type: "error"` and a textual `message`;
- that message, trimmed, begins with
  ``Configured value for `<field>` is disallowed by requirements; falling back``,
  where `<field>` is any non-empty backtick-quoted configuration key —
  `permission_profile`, `approval_policy`, `web_search_mode` or another.

The diagnostic is the whole trimmed message. An invocation keeps its distinct
messages in the order first seen; a repeated message is kept once. Nothing else
contributes one: an `agent_message` or any other item quoting the text, an error
event carrying another message, a line that is not one JSON object, stderr, and
every other family's output.

The diagnostic is informational only. It never parks the run, retries, changes
the exit status, the charge of the attempt or the task's state, and it is not a
provider refusal: the list of [§FS-rhei-agents.2.3](rhei-agents.spec.md#23-recognized-provider-refusals) stays closed. An invocation that
exits `0` with every required output present is a success whether or not it
carries one. A diagnostic belongs to the one invocation whose output carried it,
and a later attempt or a later state never inherits it. It is shown beside the
missing outputs it explains and on the console ([§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout),
[§FS-rhei-run-report.3.4](rhei-run-report.spec.md#34-plain--non-tty-mode)), and travels on the slot-release event ([§FS-rhei-run-json.2.1](rhei-run-json.spec.md#21-records)).

Whether Codex falls back or refuses to start depends on the release, the field
and what the requirements allow. Given `danger-full-access` with
`approval_policy="never"` under requirements that refuse `danger-full-access`,
Codex rust-v0.159.3 refuses at startup if they still allow `never`, which is an
ordinary non-zero process result. If they refuse `never` too, it reports two
fallbacks, `approval_policy` and then `permission_profile`, and runs read-only,
and Rhei names both. The diagnostic covers whatever Codex does report as a
fallback, in that release and in older ones.

On an account whose requirements refuse `never`, every Codex invocation, in the
`workspace` mode as in any other, carries an `approval_policy` fallback, because
`codex exec` asks for `never` on its own whatever the mode says. That diagnostic
is expected and does not by itself explain a halt: Codex falls back to the first
policy the requirements allow, and under `on-request` a write inside the granted
roots needs no approval. Where `untrusted` is the first allowed policy, such a
write asks for an approval that `codex exec` cannot give.
