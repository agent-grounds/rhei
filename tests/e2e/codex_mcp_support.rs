//! Isolated real Rhei runs and optional native Codex configuration validation.
//! §FS-rhei-agents.1.1.2.1

use super::*;
use serde_json::{json, Value};

pub const TOKEN: &str = "space \"quote\" \\slash\nUnicode: λ";
pub const ARGUMENTS: &[&str] =
    &["serve", "", "space and \"quote\"", "C:\\server\\path", "line\nbreak", "λ"];

pub struct CodexRun {
    pub dir: TestDir,
    pub run: CliRun,
    pub plan: String,
    pub record: Option<Value>,
}

pub fn command_server() -> Value {
    json!({
        "command": ["lookup-server", "serve", "", "space and \"quote\"", "C:\\server\\path", "line\nbreak", "λ"],
        "env": {"TOKEN": "${RHEI_E2E_CODEX_TOKEN}", "PATH": "${RHEI_E2E_CODEX_PATH}", "EMPTY": ""},
        "working_directory": "server home",
        "startup_timeout": "37s"
    })
}

pub fn run_case(name: &str, mut settings: Value, state_fields: &str, agent: &str) -> CodexRun {
    let dir = unique_temp_dir(&format!("codex-mcp-{name}"));
    // Scratch may itself be under a checkout; the server cwd belongs to this one.
    assert!(git_command()
        .current_dir(&dir)
        .arg("init")
        .output()
        .expect("initialize fixture checkout")
        .status
        .success());
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).expect("private bin");
    fs::create_dir_all(dir.join("server home")).expect("server cwd");
    let fixture = PathBuf::from(env!("CARGO_BIN_EXE_codex-mcp-fixture"));
    for name in ["codex", "lookup-server"] {
        fs::copy(&fixture, bin.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
            .expect("stage native fixture");
    }
    if settings["agents"].get("cdx").is_some() {
        settings["agents"]["cdx"]["command"] = json!([fixture, "exec"]);
    }
    let config = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&config).expect("project settings");
    write_fixture_file(&config, "settings.json", &settings.to_string());
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Native Codex MCP\n\n## Tasks\n\n### Task 1: Read dashboards\n**State:** work\n",
    );
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            "name: codex-mcp\nversion: 1\nstates:\n  work:\n    initial: true\n    agent: {agent}\n    agent_timeout: {FIXTURE_MARGIN}\n{state_fields}    instructions: \"lookup available: {{mcp.lookup.available}}\"\n  blocked:\n    gating: true\n  completed:\n    final: true\ntransitions:\n  - {{from: work, to: completed}}\n  - {{from: work, to: blocked, mcp_unavailable: [lookup]}}\n  - {{from: blocked, to: work}}\n"
        ),
    );
    let mut paths = vec![bin.clone()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    let home = dir.join("home");
    let codex_home = home.join(".codex");
    fs::create_dir_all(&codex_home).expect("isolated Codex home");
    let copy = dir.join("native.json");
    let output = rhei_command(&home)
        .current_dir(&dir)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("CODEX_HOME", &codex_home)
        .env("PATH", std::env::join_paths(paths).expect("fixture PATH"))
        .env("RHEI_E2E_CODEX_RECORD", &copy)
        .env("RHEI_E2E_CODEX_TOKEN", TOKEN)
        .env("RHEI_E2E_CODEX_PATH", &bin)
        .arg("--state-machine")
        .arg(machine)
        .arg("run")
        .arg(&plan)
        .args(["--no-tui", "--no-callbacks"])
        .output()
        .expect("real Rhei run");
    let record =
        fs::read_to_string(copy).ok().map(|text| serde_json::from_str(&text).expect("record JSON"));
    CodexRun {
        plan: fs::read_to_string(plan).expect("read plan"),
        dir,
        run: CliRun::from(&output),
        record,
    }
}

pub fn assert_completed<'a>(case: &'a CodexRun, selected: &str) -> &'a Value {
    let record = case.record.as_ref().unwrap_or_else(|| {
        panic!("Codex was not spawned; stdout:\n{}\nstderr:\n{}", case.run.stdout, case.run.stderr)
    });
    assert!(
        record["error"].is_null(),
        "Codex refused Rhei's argv: {}; argv: {}",
        record["error"],
        record["argv"]
    );
    assert_success(&case.run);
    assert!(case.plan.contains("**State:** completed"), "{}", case.plan);
    assert_eq!(record["selected"], selected);
    record
}

/// When installed, ask the real client's parser AND configuration loader.
/// `mcp list` reads configuration without starting servers or contacting a model.
pub fn native_servers(case: &CodexRun) -> Option<Value> {
    let binary = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join(format!("codex{}", std::env::consts::EXE_SUFFIX)))
        .find(|path| path.is_file())?;
    let args: Vec<String> =
        serde_json::from_value(case.record.as_ref()?["argv"].clone()).expect("argv");
    let base = || {
        let mut command = std::process::Command::new(&binary);
        command
            .current_dir(&case.dir)
            .env("HOME", case.dir.join("home"))
            .env("CODEX_HOME", case.dir.join("home/.codex"))
            .env("XDG_CONFIG_HOME", case.dir.join("home/.config"));
        command
    };
    let end = args.iter().position(|arg| arg == "--").expect("separator");
    let help = base().args(&args[..end]).arg("--help").output().expect("native exec parser");
    assert!(help.status.success(), "native exec rejected argv: {}", stderr(&help));
    let mut command = base();
    for pair in args[..end].windows(2).filter(|pair| pair[0] == "-c" || pair[0] == "--config") {
        command.args(pair);
    }
    let output = command.args(["mcp", "list", "--json"]).output().expect("native config loader");
    assert!(output.status.success(), "native config rejected: {}", stderr(&output));
    Some(serde_json::from_slice(&output.stdout).expect("native server listing"))
}
