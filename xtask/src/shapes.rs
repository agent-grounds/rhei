//! The shape pairs under `examples/shape/`, found on disk rather than listed in
//! `EXAMPLES`, so a pair joins `cargo xtask examples` by its directories alone.

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use crate::{workspace_root, Example};

/// The shape pairs: the same work authored two ways, both runnable with the mock
/// agent, so the difference a run shows is the shape. Every
/// `examples/shape/<pair>/<shape>/` is `shape-<pair>-<shape>`, found on disk so
/// a pair is added by its directories alone. A shape that is a project, with an
/// `index.panta.md`, names no machine: each of its rheis finds its own
/// (§FS-rhei-panta.1). The pairs are found once per process and their names
/// leaked then, so they read like the static table's. §FS-rhei-shape.4
pub(crate) fn shape_examples() -> &'static [Example] {
    static FOUND: OnceLock<Vec<Example>> = OnceLock::new();
    FOUND.get_or_init(|| discover(&workspace_root()))
}

fn discover(root: &Path) -> Vec<Example> {
    let leak = |text: String| -> &'static str { Box::leak(text.into_boxed_str()) };
    let mut found = Vec::new();
    for pair in subdirectories(&root.join("examples/shape")) {
        for shape in subdirectories(&root.join("examples/shape").join(&pair)) {
            let path = format!("examples/shape/{pair}/{shape}");
            let project = root.join(&path).join("index.panta.md").is_file();
            found.push(Example {
                name: leak(format!("shape-{pair}-{shape}")),
                state_machine: (!project).then(|| leak(format!("{path}/states.yaml"))),
                path: leak(path),
                runnable: true,
            });
        }
    }
    found
}

/// The names of the directories directly under `dir`, sorted; none when it is absent.
fn subdirectories(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find;

    /// A pair is added by its directories alone, so every shape on disk is
    /// listed and runnable under `shape-<pair>-<shape>`, on the machine beside
    /// it or, for a project, on its rheis' own, and the eight names the first
    /// four pairs shipped under still resolve. §FS-rhei-shape.4
    #[test]
    fn every_shape_directory_is_a_listed_runnable_example() {
        let root = workspace_root();
        let shapes = root.join("examples/shape");
        let mut seen = 0;
        for pair in fs::read_dir(&shapes).expect("examples/shape is readable") {
            let pair = pair.expect("pair entry");
            if !pair.file_type().expect("file type").is_dir() {
                continue;
            }
            for shape in fs::read_dir(pair.path()).expect("pair is readable") {
                let shape = shape.expect("shape entry");
                if !shape.file_type().expect("file type").is_dir() {
                    continue;
                }
                let pair = pair.file_name().to_string_lossy().into_owned();
                let shape = shape.file_name().to_string_lossy().into_owned();
                let name = format!("shape-{pair}-{shape}");
                let example = find(&name).unwrap_or_else(|| panic!("{name} is listed"));
                assert!(example.runnable, "{name} runs with the mock agent");
                assert_eq!(example.path, format!("examples/shape/{pair}/{shape}"));
                assert!(root.join(example.path).is_dir(), "{name} runs from its directory");
                let project = root.join(example.path).join("index.panta.md").is_file();
                match example.state_machine {
                    None => assert!(project, "{name} names its machine unless it is a project"),
                    Some(machine) => {
                        assert!(!project, "{name} is a project, whose rheis find their own");
                        assert!(root.join(machine).is_file(), "{name} runs on {machine}");
                    }
                }
                seen += 1;
            }
        }
        assert!(seen >= 8, "the shape pairs are found on disk, {seen} shapes");
        for landed in [
            "shape-reproducer-flat",
            "shape-reproducer-nested",
            "shape-review-against-spec-flat",
            "shape-review-against-spec-nested",
            "shape-cve-category-flat",
            "shape-cve-category-nested",
            "shape-parts-of-a-feature-flat",
            "shape-parts-of-a-feature-nested",
        ] {
            assert!(find(landed).is_some(), "{landed} still resolves");
        }
    }
}
