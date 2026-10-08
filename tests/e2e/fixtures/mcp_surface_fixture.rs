// Recording CLI double: native mail is additive unless strict is an option.
// Only attachment IDs are modeled; this does not start a real MCP server.
// §FS-rhei-states.7.3

use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let options = &args[..args.iter().position(|arg| arg == "--").unwrap_or(args.len())];
    let strict = options.iter().any(|arg| arg == "--strict-mcp-config");
    let config_path = options.windows(2).find(|pair| pair[0] == "--mcp-config");
    let config: Option<serde_json::Value> = config_path.map(|pair| {
        serde_json::from_str(&fs::read_to_string(&pair[1]).expect("read generated MCP JSON"))
            .expect("parse generated MCP JSON")
    });
    let mut servers = Vec::new();
    if !strict {
        servers.push("mail".to_string());
    }
    if let Some(config) = &config {
        servers
            .extend(config["mcpServers"].as_object().expect("mcpServers object").keys().cloned());
    }
    servers.sort();
    let record = serde_json::json!({
        "argv": args, "strict": strict, "config": config, "servers": servers,
        "selected": env::var("RHEI_MCP_SERVERS").unwrap_or_default(),
    });
    if let Ok(dir) = env::var("RHEI_MCP_RECORD_DIR") {
        fs::create_dir_all(&dir).expect("create record directory");
        fs::write(PathBuf::from(dir).join("record.json"), record.to_string())
            .expect("write record");
    }
    println!("{record}");

    let mut prompt = String::new();
    io::stdin().read_to_string(&mut prompt).expect("read stdin prompt");
    if let Some(result) = prompt.split_once("\n## Result\n").map(|(_, result)| result) {
        if let Some(path) = result
            .lines()
            .find_map(|line| line.trim().strip_prefix("- `")?.strip_suffix('`').map(PathBuf::from))
        {
            let root = PathBuf::from(env::var("RHEI_CHECKOUT_ROOT").expect("checkout root"));
            let path = if path.is_absolute() { path } else { root.join(path) };
            fs::create_dir_all(path.parent().expect("result parent"))
                .expect("create result parent");
            fs::write(path, "Recording fixture completed.\n").expect("write result");
        }
    }
}
