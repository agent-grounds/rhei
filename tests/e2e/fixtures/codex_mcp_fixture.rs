//! Provider-free Codex parser/configuration double. No MCP process is started.
//! Native TOML is parsed rather than accepting any string after `-c`.
//! §FS-rhei-agents.1.1.2.1

use serde_json::json;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

fn apply(into: &mut toml::Value, path: &[&str], value: toml::Value) {
    if !into.is_table() {
        *into = toml::Value::Table(Default::default());
    }
    let table = into.as_table_mut().expect("override table");
    if path.len() == 1 {
        table.insert(path[0].to_string(), value);
    } else {
        let child = table.entry(path[0]).or_insert_with(|| toml::Value::Table(Default::default()));
        apply(child, &path[1..], value);
    }
}

fn configuration(args: &[String]) -> Result<toml::Value, String> {
    let end = args.iter().position(|arg| arg == "--").ok_or("missing stdin separator")?;
    if end + 1 != args.len() {
        return Err("configuration or prompt text after stdin separator".into());
    }
    let mut config = toml::Value::Table(Default::default());
    let mut i = 1;
    while i < end {
        let arg = &args[i];
        if arg == "--mcp" {
            return Err("unexpected argument '--mcp' found".into());
        }
        if arg == "-c" || arg == "--config" {
            i += 1;
            let value = args.get(i).ok_or("missing configuration override")?;
            let (key, value) = value
                .split_once('=')
                .ok_or_else(|| format!("Invalid override (missing '='): {value}"))?;
            // Codex splits override paths on literal dots; quoting a path component
            // does not protect dots. TOML table values can preserve complete ids.
            let path: Vec<&str> = key.split('.').map(str::trim).collect();
            let mut parsed: toml::Value = format!("value={value}")
                .parse()
                .map_err(|err| format!("invalid TOML value: {err}"))?;
            let value =
                parsed.as_table_mut().expect("value wrapper").remove("value").expect("value");
            apply(&mut config, &path, value);
        }
        i += 1;
    }
    Ok(config)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--version") {
        println!("codex MCP fixture");
        return;
    }
    assert_eq!(args.first().map(String::as_str), Some("exec"), "MCP server must never start");
    let mut prompt = String::new();
    io::stdin().read_to_string(&mut prompt).expect("read stdin prompt");
    let parsed = configuration(&args);
    let record = json!({
        "argv": args,
        "config": parsed.as_ref().ok(),
        "error": parsed.as_ref().err(),
        "selected": env::var("RHEI_MCP_SERVERS").unwrap_or_default(),
        "prompt": prompt,
    });
    fs::write(env::var_os("RHEI_E2E_CODEX_RECORD").expect("record path"), record.to_string())
        .expect("record native invocation");
    if let Err(error) = parsed {
        eprintln!("error: {error}");
        std::process::exit(2);
    }
    if let Some(result) = prompt.split_once("\n## Result\n").map(|(_, result)| result) {
        if let Some(path) = result
            .lines()
            .find_map(|line| line.trim().strip_prefix("- `")?.strip_suffix('`').map(PathBuf::from))
        {
            let root = PathBuf::from(env::var("RHEI_CHECKOUT_ROOT").expect("checkout root"));
            let path = if path.is_absolute() { path } else { root.join(path) };
            fs::create_dir_all(path.parent().expect("result parent")).expect("result directory");
            fs::write(path, "Codex configuration fixture completed.\n").expect("write result");
        }
    }
}
