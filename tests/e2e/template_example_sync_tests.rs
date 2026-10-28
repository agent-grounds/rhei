//! Drift gate: every committed example under `examples/<name>-example/` must be
//! exactly what `rhei instantiate <template> --values .example-values.yaml`
//! produces. Editing a template without regenerating its example fails here.
//!
//! The example owns two files that are NOT part of the rendered template output
//! and are therefore excluded from the comparison:
//!   * `README.md` — a hand-written, example-specific doc (the template ships its
//!     own different README, which instantiation emits and the example overrides).
//!   * `instantiation-values.yaml` — the checked-in copy of the input values. The
//!     test asserts it is byte-identical to the template's `.example-values.yaml`
//!     so the documented regenerate command actually reproduces the example.
//!
//! To fix a failure here, regenerate the example (see its README's "Regenerate"
//! section) and commit the result.
//!
//! §FS-rhei-templates.6.1.2: step 6 ("Write output") is what produces a
//! committed, checked-in example; this gate proves the checked-in copy stays
//! what that command actually produces.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// (template dir under `crates/rhei-cli/templates`, example dir under `examples/`).
const TEMPLATE_EXAMPLES: &[(&str, &str)] = &[
    ("analyze-and-dispatch", "analyze-and-dispatch-example"),
    ("parallel-worktrees", "parallel-worktrees-example"),
    ("multi-model-analysis", "multi-model-analysis-example"),
    ("spec-review", "spec-review-example"),
    ("supervised-delivery", "supervised-delivery-example"),
    ("agora", "agora-example"),
    ("changeset-review", "changeset-review-example"),
    ("product-management", "product-management-example"),
    ("spec-implementation", "spec-implementation-example"),
    ("spec-implementation-discrepancy-audit", "spec-implementation-discrepancy-audit-example"),
    // Not hourly-human-intervention: its `type: path` defaults render as absolute
    // paths under the directory it is instantiated from, so its example differs
    // with every checkout location.
];

/// Example-owned files that the rendered template output does not include.
/// The spec-review fixture is demo data for the committed example — a user's
/// instantiation reviews the user's own spec, so the template must not ship it.
/// Spelled a segment at a time: `relative_files` compares against
/// `strip_prefix`'s output, which uses the host's separator.
fn example_only_files() -> Vec<String> {
    [
        PathBuf::from("README.md"),
        PathBuf::from("instantiation-values.yaml"),
        Path::new("specs").join("template-review-fixture.spec.md"),
    ]
    .iter()
    .map(|path| path.display().to_string())
    .collect()
}

/// Sorted, root-relative paths of every file under `root`, skipping any whose
/// relative path appears in `skip`.
fn relative_files(root: &Path, skip: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    collect_files(root, root, skip, &mut out);
    out.sort();
    out
}

fn collect_files(root: &Path, dir: &Path, skip: &[&str], out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("read directory") {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        if entry.file_type().expect("file type").is_dir() {
            collect_files(root, &path, skip, out);
            continue;
        }
        let rel =
            path.strip_prefix(root).expect("strip root prefix").to_string_lossy().into_owned();
        if !skip.contains(&rel.as_str()) {
            out.push(rel);
        }
    }
}

/// A machine's fence lines without what names the commit the example was laid
/// from: the `laid` line goes, and an inclusion fence loses its `git:` field and
/// state word. Both move with every commit of this repository, and neither is
/// what the template renders. §FS-rhei-library.7.1
fn without_source_fields(rel: &str, bytes: Vec<u8>) -> Vec<u8> {
    if !rel.ends_with("states.yaml") {
        return bytes;
    }
    const STATES: [&str; 5] = ["clean", "dirty", "unversioned", "unknown", "builtin"];
    let text = String::from_utf8_lossy(&bytes);
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with("# --- laid ") {
            continue;
        }
        if !line.starts_with("# --- ") {
            out.push_str(line);
            continue;
        }
        let fields: Vec<&str> = line.split(' ').collect();
        for (index, field) in fields.iter().enumerate() {
            let state = STATES.contains(field)
                && fields.get(index + 1).is_some_and(|next| next.starts_with("inputs:"));
            if field.starts_with("git:") || state {
                continue;
            }
            if index > 0 && !out.ends_with('\n') {
                out.push(' ');
            }
            out.push_str(field);
        }
    }
    out.into_bytes()
}

#[test]
fn committed_examples_match_template_instantiation() {
    let templates_root = repo_root().join("crates/rhei-cli/templates");
    let examples_root = repo_root().join("examples");

    for (template_name, example_name) in TEMPLATE_EXAMPLES {
        let template_dir = templates_root.join(template_name);
        let example_dir = examples_root.join(example_name);
        let values = template_dir.join(".example-values.yaml");

        // The example's checked-in values must match the template's, so the
        // documented `rhei instantiate ... --values .example-values.yaml`
        // regeneration reproduces the committed example.
        let template_values =
            fs::read_to_string(&values).expect("read template .example-values.yaml");
        let example_values = fs::read_to_string(example_dir.join("instantiation-values.yaml"))
            .expect("read example instantiation-values.yaml");
        assert_eq!(
            template_values, example_values,
            "{example_name}/instantiation-values.yaml must equal \
             {template_name}/.example-values.yaml so the example can be regenerated"
        );

        // Instantiate the template into a throwaway workspace with a clean HOME
        // so global settings cannot leak into the rendered output.
        let scratch = unique_scratchpad_dir(&format!("tmpl-sync-{template_name}"));
        let out = scratch.join("out");
        let home = scratch.join(".home");
        fs::create_dir_all(&home).expect("create isolated home");
        let result = super::rhei_command(&home)
            .args([
                "instantiate",
                template_dir.to_str().expect("template path is utf8"),
                "--values",
                values.to_str().expect("values path is utf8"),
                "--output",
                out.to_str().expect("output path is utf8"),
            ])
            .output()
            .expect("rhei instantiate should run");
        assert!(
            result.status.success(),
            "instantiate {template_name} failed:\nstdout:\n{}\nstderr:\n{}",
            stdout(&result),
            stderr(&result)
        );

        // Same set of rendered files (the template README is excluded; the
        // example overrides it and ships its own instantiation-values.yaml).
        let generated = relative_files(&out, &["README.md"]);
        let example_only = example_only_files();
        let example_only: Vec<&str> = example_only.iter().map(String::as_str).collect();
        let committed = relative_files(&example_dir, &example_only);
        assert_eq!(
            generated, committed,
            "rendered file set for {example_name} drifted from template \
             {template_name}; regenerate the example (see its README)"
        );

        // Byte-for-byte equality on every rendered file, fences aside.
        for rel in &generated {
            let generated_bytes =
                without_source_fields(rel, fs::read(out.join(rel)).expect("read generated file"));
            let committed_bytes = without_source_fields(
                rel,
                fs::read(example_dir.join(rel)).expect("read committed file"),
            );
            assert!(
                generated_bytes == committed_bytes,
                "{example_name}/{rel} differs from the template instantiation; \
                 regenerate the example (see its README)"
            );
        }

        fs::remove_dir_all(&scratch).ok();
    }
}
