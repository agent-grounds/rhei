// Source guard scaffolding only: no production diagnostic behavior. §FS-rhei-errors.6
mod diagnostic_guidance_guard {
    /// Direct diagnostics and wrapper callers supply their own help before the message argument;
    /// help inherited from a macro definition or a later call does not count.
    fn uncovered_in_source(source: &str, macros: &[&str]) -> (usize, Vec<usize>) {
        let lines: Vec<_> = source.lines().collect();
        let mut total = 0;
        let mut uncovered = Vec::new();
        let explicit_help = regex::Regex::new(r"^\s*help\s*=").unwrap();
        for (index, line) in lines.iter().enumerate() {
            for name in macros {
                let pattern = format!("{name}!(");
                for (offset, _) in line.match_indices(&pattern) {
                    total += 1;
                    let window = lines[index..lines.len().min(index + 3)].join("\n");
                    let arguments = &window[offset + pattern.len()..];
                    let covered = explicit_help.is_match(arguments);
                    if !covered {
                        uncovered.push(index + 1);
                    }
                }
            }
        }
        (total, uncovered)
    }

    pub(super) fn check_sources(macros: &[&str]) {
        let mut pending = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        let mut uncovered = Vec::new();
        let mut total = 0;
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).expect("read source dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
                if !name.ends_with(".rs") || name.starts_with("tests_") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).expect("read source");
                let (count, missing) = uncovered_in_source(&source, macros);
                total += count;
                uncovered.extend(missing.into_iter().map(|line| format!("{name}:{line}")));
            }
        }
        assert!(total > 300, "the scan found only {total} sites; has the pattern changed?");
        uncovered.sort();
        assert!(uncovered.is_empty(), "{} of {total} diagnostic sites carry no caller help:\n  {}", uncovered.len(), uncovered.join("\n  "));
    }

    #[test]
    #[ignore = "red until #557: wrapper callers require failure-derived help; implement removes this"]
    fn diagnostic_guidance_all_wrapper_callers_carry_help() {
        check_sources(&["miette", "diagnostic"]);
    }

    /// Negative fixtures exercise the same scanner the repository guard uses.
    #[test]
    fn diagnostic_guidance_guard_rejects_uncovered_wrapper_and_direct_sites() {
        let source = r#"macro_rules! diagnostic {
    ($($tokens:tt)*) => { miette!(
        help = "generic fallback",
        $($tokens)*
    ) };
}
let refusal = diagnostic!("worker may not retry");
let forwarded = diagnostic!("{err}");
let direct = miette!("no guidance");
let covered = miette!(help = "write runtime/ruling.md", "missing output");
let explicit = diagnostic!(help = "ask the operator", "worker refusal");
"#;
        let (count, missing) = uncovered_in_source(source, &["miette", "diagnostic"]);
        assert_eq!(count, 6);
        assert_eq!(missing, vec![7, 8, 9], "macro help must not cover its callers");
    }
}
