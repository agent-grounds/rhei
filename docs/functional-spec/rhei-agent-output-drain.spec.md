# FS-rhei-agent-output-drain: Agent Output at Exit

An agent invocation ends at its direct subprocess's exit
(§FS-rhei-agents.3.2.1), and what judges it afterwards reads the stdout and
stderr rhei captured from it: provider-limit recognition
(§FS-rhei-agents.2.3), the invocation's usage capture
(§FS-rhei-cost-accounting.4), and the log, whose exit footer closes the
transcript (§FS-rhei-agents.8). This point says how much of that output they
read. The exit is the boundary: what was written before it is read whole, and
what a descendant writes after it is read only if it arrives in time.

## 1. Output Written Before the Exit Is Captured Whole

Everything the streams already held when the subprocess exited is captured
whole before anything reads it. Every line of it reaches provider-limit
recognition, the usage capture, and the log ahead of its exit footer, and an
unterminated last line is read as a line of its own. No timer cuts that capture
short. It is rhei's own work on bytes already written, so a slow machine can
delay it but never shorten it.

## 2. Output Written After the Exit Is Best-Effort

Once that output is captured, each stream gets a short drain grace, and a
reader still open when the grace runs out is detached. A descendant that still
holds the stdout or stderr it inherited therefore does not delay completion,
and what it writes after the exit reaches the log only best-effort.
