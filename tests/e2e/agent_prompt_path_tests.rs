use std::fs;

use super::*;

/// A worker resolves the paths in its prompt against the artifact root the
/// prompt names, and against nothing else. A root spelled relatively on the
/// command line must therefore not reach the prompt as a path only the
/// launching shell could resolve: the `rhei run` working directory is not in
/// the prompt, is not the worker's own working directory, and is no longer in
/// the environment.
///
/// The layout is the one that tells the two bases apart. The workspace is
/// `nested/ws` under a git repository, `rhei run` is launched from `nested`,
/// and the checkout root is therefore the repository root — a third directory,
/// so a path that resolves from the launcher does not resolve from the worker
/// and the reverse.
// §FS-rhei-agents.4.1
#[test]
fn prompt_paths_resolve_against_the_artifact_root_when_the_run_root_is_relative() {
    let dir = unique_temp_dir("agent-prompt-path-base");
    if !git_init_or_absent(&dir) {
        eprintln!("skipping: git is not on this machine");
        return;
    }

    let launch = dir.join("nested");
    let workspace = launch.join("ws");
    fs::create_dir_all(workspace.join("tasks")).expect("create workspace");

    // Two workers, two readings a bare relative path admits here: the declared
    // output against the artifact root, the export against the worker's own
    // directory. They agree only where the path arrives absolute. §FS-rhei-agents.4.1
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"prompt = agent_prompt()
root = pathlib.Path(env('RHEI_ROOT'))
task = pathlib.Path(env('RHEI_RESULT_PATH')).stem
write(root / 'runtime' / ('prompt-%s.md' % task), 'CWD=%s\n%s' % (pathlib.Path.cwd(), prompt))

line = next((l for l in prompt.splitlines() if l.startswith('Findings output: ')), None)
if line:
    findings = pathlib.Path(line[len('Findings output: '):])
    if not findings.is_absolute():
        findings = root / findings
    write(findings, '# Findings\n\nThe worker wrote where the prompt said.\n')

if '\n## Exports to Publish\n' in prompt:
    section = prompt.split('\n## Exports to Publish\n', 1)[1]
    named = re.search(r'^- `[^`]+` \S+ `([^`]+)`$', section, re.MULTILINE)
    export = pathlib.Path(named.group(1))
    if not export.is_absolute():
        export = pathlib.Path.cwd() / export
    write(export, 'FINDINGS EXPORT BODY\n')

if '\n## Consumed Exports\n' in prompt:
    write(root / 'runtime' / 'consumed.md', prompt.split('\n## Consumed Exports\n', 1)[1])

result('## Result\n\nWorker finished at the path the prompt named.\n')
"#,
    );
    let agent_command =
        serde_json::to_string(&vec![python_command().to_string(), agent.display().to_string()])
            .expect("serialize mock agent command");
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "30s" }},
  "agents": {{
    "mock": {{ "command": {agent_command}, "stdin_prompt": true, "timeout": "30s" }}
  }}
}}"#
        ),
    )
    .expect("write settings");

    fs::write(
        workspace.join("index.rhei.md"),
        r#"# Rhei: Prompt Path Base

## Overview

Two agent tasks and one hand-off, so the paths the prompt composes are the
whole subject.
"#,
    )
    .expect("write index");
    fs::write(
        workspace.join("tasks/01-work.md"),
        r#"### Task 1: Write where the prompt says
**State:** work
**Provides:** findings
"#,
    )
    .expect("write producer task file");
    fs::write(
        workspace.join("tasks/02-read.md"),
        r#"### Task 2: Read what the first task published
**State:** work
**Prior:** 1
**Consumes:** 1:findings
"#,
    )
    .expect("write consumer task file");
    fs::write(
        workspace.join("states.yaml"),
        r#"name: prompt-path-base
version: 1
states:
  work:
    initial: true
    agent: mock
    instructions: |
      Findings output: {output.findings.path}
    outputs:
      - name: findings
        path: runtime/findings/{task_id}.md
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#,
    )
    .expect("write state machine");

    let output = rhei_command(dir.join(".home"))
        .current_dir(&launch)
        .args(["run", "ws", "--no-tui", "--no-callbacks"])
        .output()
        .expect("run the workspace from its parent directory");
    let stdout = stdout(&output);
    let stderr = stderr(&output);
    let evidence = fs::read_to_string(workspace.join("runtime/prompt-ws.1.md")).unwrap_or_default();

    let findings = workspace.join("runtime/findings/ws.1.md");
    let result = workspace.join("runtime/results/ws.1.md");
    assert!(
        output.status.success()
            && fs::read_to_string(&findings).is_ok_and(|body| !body.trim().is_empty())
            && fs::read_to_string(&result).is_ok_and(|body| !body.trim().is_empty()),
        "a worker that resolves its prompt paths against the artifact root must satisfy the run\n\
         declared output at {}: {}\n\
         result at {}: {}\n\
         stdout:\n{stdout}\nstderr:\n{stderr}\nprompt the worker read:\n{evidence}",
        findings.display(),
        findings.exists(),
        result.display(),
        result.exists(),
    );

    // The hand-off, not the spelling: a bare export path lands in the checkout,
    // the consumer is composed with no `## Consumed Exports` at all, and the run
    // still reports every task completed. So exit 0 proves nothing. §FS-rhei-agents.4.1
    let export = workspace.join("runtime/exports/ws.1/findings.md");
    let consumed = fs::read_to_string(workspace.join("runtime/consumed.md")).unwrap_or_default();
    assert!(
        export.is_file() && consumed.contains("FINDINGS EXPORT BODY"),
        "the export must reach the consumer's prompt, not the checkout root\n\
         export under the artifact root at {}: {}\n\
         `## Consumed Exports` the consumer read:\n{consumed}\n\
         stdout:\n{stdout}\nstderr:\n{stderr}\nproducer prompt:\n{evidence}",
        export.display(),
        export.exists(),
    );
    let published = evidence
        .lines()
        .find(|line| line.contains("findings") && line.contains("runtime/exports"))
        .unwrap_or_default();
    assert!(
        published.contains(&workspace.display().to_string()),
        "the export path must name its own base while the checkout root differs from the \
         artifact root, and this one does not: {published}\nproducer prompt:\n{evidence}",
    );
}

/// The name a caller gives the artifact root is the name the prompt shows.
/// Resolving the root makes it absolute; it does not re-spell it. A workspace
/// reached through a symlink is one directory under two names — the shape
/// macOS's own temporary directory has, and the shape a Windows 8.3 short path
/// has — and a worker handed the name it did not type is told about a place it
/// cannot find in anything else it was given.
///
/// The same run pins the other half of the rule. The checkout root arrives
/// from `git rev-parse` already resolved, so asking whether it *is* the
/// artifact root has to compare the directories rather than their spellings:
/// answered on the spellings, this prompt would render every path absolute
/// where the table in §FS-rhei-agents.4.1 says relative.
// §FS-rhei-agents.4.1
#[cfg(unix)]
#[test]
fn an_artifact_root_reached_through_a_symlink_keeps_the_name_it_was_given() {
    let dir = unique_temp_dir("agent-prompt-path-symlink");
    let workspace = dir.join("real/ws");
    fs::create_dir_all(workspace.join("tasks")).expect("create workspace");
    std::os::unix::fs::symlink("real", dir.join("link")).expect("symlink beside the workspace");
    let given = dir.join("link/ws");

    // The repository is the workspace itself, so the checkout root and the
    // artifact root are one directory reached by its two names. §FS-rhei-agents.4.1
    if !git_init_or_absent(&workspace) {
        eprintln!("skipping: git is not on this machine");
        return;
    }

    let agent = write_python_agent(
        &dir,
        "symlink-agent.py",
        r#"prompt = agent_prompt()
root = pathlib.Path(env('RHEI_ROOT'))
write(root / 'runtime' / 'prompt.md', prompt)
line = next(l for l in prompt.splitlines() if l.startswith('Findings output: '))
findings = pathlib.Path(line[len('Findings output: '):])
write(findings if findings.is_absolute() else root / findings, '# Findings\n')
result('## Result\n\nWorker finished under the name it was given.\n')
"#,
    );
    let agent_command =
        serde_json::to_string(&vec![python_command().to_string(), agent.display().to_string()])
            .expect("serialize mock agent command");
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "30s" }},
  "agents": {{
    "mock": {{ "command": {agent_command}, "stdin_prompt": true, "timeout": "30s" }}
  }}
}}"#
        ),
    )
    .expect("write settings");
    fs::write(
        workspace.join("index.rhei.md"),
        "# Rhei: Symlinked Root\n\n## Overview\n\nOne task, so the \
         spelling of the paths is the whole subject.\n",
    )
    .expect("write index");
    fs::write(
        workspace.join("tasks/01-work.md"),
        "### Task 1: Report the name the prompt gave\n**State:** work\n",
    )
    .expect("write task file");
    fs::write(
        workspace.join("states.yaml"),
        r#"name: symlinked-root
version: 1
states:
  work:
    initial: true
    agent: mock
    instructions: |
      Findings output: {output.findings.path}
    outputs:
      - name: findings
        path: runtime/findings/{task_id}.md
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#,
    )
    .expect("write state machine");

    let output = rhei_command(dir.join(".home"))
        .args(["run", &given.display().to_string(), "--no-tui", "--no-callbacks"])
        .output()
        .expect("run the workspace through the symlink");
    let stdout = stdout(&output);
    let stderr = stderr(&output);
    let prompt = fs::read_to_string(workspace.join("runtime/prompt.md")).unwrap_or_default();
    assert!(
        output.status.success() && !prompt.is_empty(),
        "the run must reach its worker:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    assert!(
        prompt.contains(&format!("rhei-managed plan at `{}`.", given.display())),
        "the prompt must name the artifact root the way the run was given it, `{}`:\n{prompt}",
        given.display(),
    );
    assert!(
        !prompt.contains(&workspace.display().to_string()),
        "no path in the prompt may be re-spelled through `{}`, which the caller never typed:\n\
         {prompt}",
        workspace.display(),
    );
    assert!(
        prompt.contains("Findings output: runtime/findings/ws.1.md"),
        "the checkout root is this artifact root under its resolved name, so a declared output \
         renders relative:\n{prompt}",
    );
}
