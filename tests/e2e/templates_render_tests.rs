//! Instantiation *rendering*: the restricted MiniJinja environment — what a
//! template author may write in a skeleton, and what comes out the other side.
//! The `rhei instantiate` / `rhei templates` surface is in `templates_tests.rs`.

// §FS-rhei-templates.5

use std::fs;

use super::templates_tests::run_raw;
use super::*;

/// §FS-rhei-templates.5: `range()`, arithmetic, and `~` unroll a counted
/// structure into one task per round, with the per-round `**Prior:**` metadata
/// that a `visits:` loop cannot express because it repeats one task.
#[test]
fn instantiate_unrolls_rounds_with_range_and_arithmetic() {
    let dir = unique_temp_dir("templates-range");
    let template_dir = dir.join("rounds-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: rounds-template
version: 1.0.0
description: Template that unrolls its rounds
inputs:
  - name: review_rounds
    description: How many review rounds to unroll
    type: number
    default: 3
"#,
    );
    write_fixture_file(
        &template_dir,
        "states.yaml",
        r#"name: rounds-template
version: 1.0.0
states:
  review:
    initial: true
    description: Review round
    agent: claude-code
    agent_timeout: 30m
    visits: {{ 2 * review_rounds + 1 }}
    instructions: Review round {visit_count}.
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: review, to: completed, description: Round done }
  - { from: "*", to: cancelled, description: Dropped }
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Rounds
**States:** rounds-template

## Tasks
{% for k in range(1, review_rounds + 1) %}
### Task {{ "review-" ~ k }}: Review round {{k}} of {{review_rounds}}
**State:** review
{% if k > 1 %}**Prior:** Task review-{{ k - 1 }}
{% endif %}
{%- endfor %}
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);

    let rendered = fs::read_to_string(output_dir.join("plan.rhei.md")).expect("read rendered plan");
    // `range(1, n + 1)` is half-open at the top, so the rounds are 1..=n.
    for k in 1..=3 {
        assert!(
            rendered.contains(&format!("### Task review-{k}: Review round {k} of 3")),
            "round {k} should be unrolled; got:\n{rendered}"
        );
    }
    assert!(!rendered.contains("review-4"), "range stops before the bound; got:\n{rendered}");
    // `k - 1` names the previous round, so each round waits on the one before
    // it — the per-task metadata a counted `visits:` loop has no place to put.
    assert!(
        rendered.contains(
            "### Task review-2: Review round 2 of 3\n**State:** review\n**Prior:** Task review-1"
        ),
        "round 2 waits on round 1; got:\n{rendered}"
    );
    assert!(
        rendered.contains("### Task review-1: Review round 1 of 3\n**State:** review\n\n"),
        "the first round has no prior; got:\n{rendered}"
    );

    // Arithmetic sizes the state's own budget from the same input.
    let machine =
        fs::read_to_string(output_dir.join("states.yaml")).expect("read rendered machine");
    assert!(machine.contains("visits: 7"), "2 * 3 + 1; got:\n{machine}");
}

#[test]
fn instantiate_renders_structured_inputs_with_minijinja_loops() {
    let dir = unique_temp_dir("templates-structured");
    let template_dir = dir.join("structured-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: structured-template
version: 1.0.0
description: Template with structured inputs
inputs:
  - name: targets
    description: Target list
    type: array
    items:
      type: object
      properties:
        id:
          type: string
        selector:
          type: string
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Structured

## Tasks

### Task analysis: Review targets
**State:** pending

{% for target in targets %}
- {{ target.id }} => {{ target.selector|slug }}
{% endfor %}
"#,
    );
    write_fixture_file(
        &dir,
        "values.yaml",
        r#"targets:
  - id: claude
    selector: claude-code[yolo]:anthropic:claude-opus-4-7
  - id: gemini
    selector: gemini[yolo]:google:gemini-3.1-pro-preview
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--values",
            dir.join("values.yaml").to_str().expect("values path"),
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);
    // §FS-rhei-templates.6.1.3: the repro command renders the output path
    // relative to the working directory it is pasted from, and quotes both
    // absolute ones for the shell it will be pasted into.
    assert!(
        result.stdout.contains(&format!(
            "rhei instantiate {} --values {} --output output",
            shell_quote(&template_dir.display().to_string()),
            shell_quote(&dir.join("values.yaml").display().to_string()),
        )),
        "expected values-file instantiate command in output; got:\n{}",
        result.stdout
    );

    let rendered = fs::read_to_string(output_dir.join("plan.rhei.md")).expect("read rendered plan");
    assert!(rendered.contains("- claude => claude-code-yolo-anthropic-claude-opus-4-7"));
    assert!(rendered.contains("- gemini => gemini-yolo-google-gemini-3.1-pro-preview"));
}

// Bundled files reach the output as written — §FS-rhei-templates.5 — and a
// malformed template is told what is wrong with it in the words of its own
// text — §FS-rhei-templates.5.3.

/// A minimal template — manifest plus a one-task plan — carrying the extra
/// files a case needs, instantiated into `output/`. Parent directories are
/// created, so a case can bundle `scripts/bad.sh`; a bundled `.sh` is made
/// executable, because keeping that bit is half of "arrives as written".
fn instantiate_bundling(prefix: &str, files: &[(&str, &str)]) -> (TestDir, PathBuf, CliRun) {
    let dir = unique_temp_dir(prefix);
    let template_dir = dir.join("bundle-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        "name: bundle-template\nversion: 1.0.0\ndescription: Carries bundled files\n",
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        "# Rhei: Bundle\n\n## Tasks\n\n### Task one: First\n**State:** pending\n",
    );
    for (name, contents) in files {
        let path = template_dir.join(name);
        fs::create_dir_all(path.parent().expect("a bundled file has a parent"))
            .expect("create bundled parent directory");
        fs::write(&path, contents).expect("write bundled file");
        #[cfg(unix)]
        if name.ends_with(".sh") {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .expect("make bundled script executable");
        }
    }

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    (dir, output_dir, result)
}

/// A fragment holding a path, spelled the way this platform writes one.
///
/// A diagnostic names the file it is about through a `PathBuf` the
/// materialization walk built by joining components, so `scripts/emit.sh` on
/// Unix is `scripts\emit.sh` on Windows. §FS-rhei-templates.5.3 pins the
/// `<path>:<line>` form and leaves the separator to the platform
/// (§REQ-cross-platform.2), so an assertion spelled with a slash pins what
/// nothing requires: it fails on Windows where it looks for the text, and —
/// worse — passes there vacuously where it looks for its absence.
fn native_path(fragment: &str) -> String {
    fragment.replace('/', std::path::MAIN_SEPARATOR_STR)
}

/// The negative half of [`assert_stderr_contains`]: the same whitespace-blind
/// comparison, asserting that the text is *not* there. A diagnostic that names
/// a construct the file does not contain is what this ticket is about, so the
/// absence has to be asserted rather than assumed.
fn assert_stderr_lacks(result: &CliRun, unwanted: &str) {
    fn collapse(text: &str) -> String {
        text.chars().filter(|c| c.is_ascii_graphic()).collect()
    }
    assert!(
        !collapse(&result.stderr).contains(&collapse(unwanted)),
        "expected stderr not to contain {:?}; got:\n{}",
        unwanted,
        result.stderr
    );
}

/// §FS-rhei-templates.5: the construct list is closed and holds no comment, so
/// a bundled shell script's `${#IR[@]}` is ordinary text. It arrives byte for
/// byte with its executable bit — the case agent-grounds/rhei#203 reports,
/// where `{#` opened a comment that never closed and instantiation aborted.
#[test]
fn instantiate_bundles_a_bash_array_length_verbatim() {
    let script = "#!/usr/bin/env bash\nset -euo pipefail\nIR=(one two three)\necho \"${#IR[@]}\"\n";
    let (_dir, output_dir, result) =
        instantiate_bundling("templates-bash-array", &[("scripts/bad.sh", script)]);
    assert_success(&result);

    let bundled = output_dir.join("scripts/bad.sh");
    let rendered = fs::read_to_string(&bundled).expect("read the bundled script");
    assert_eq!(rendered, script, "a bundled script arrives as written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&bundled).expect("stat the bundled script").permissions().mode();
        assert!(mode & 0o111 != 0, "the executable bit survives instantiation; mode {mode:o}");
    }
}

/// §FS-rhei-templates.5: the quieter half of the same defect. A file holding
/// `${#A[@]}` and a later `#}` does not fail today — it renders, and everything
/// between the two is cut out of the output without a word. The text arrives
/// complete, and §FS-rhei-templates.5.3's warning names the file and the line
/// so a template written against the old reading is told its output moved.
#[test]
fn instantiate_keeps_the_text_a_later_hash_brace_used_to_cut() {
    let script = "#!/usr/bin/env bash\nA=(x y)\necho \"${#A[@]}\"\necho keep-me\n\
                  # a trailing brace #}\necho tail\n";
    let (_dir, output_dir, result) =
        instantiate_bundling("templates-hash-brace", &[("scripts/silent.sh", script)]);
    assert_success(&result);

    let rendered =
        fs::read_to_string(output_dir.join("scripts/silent.sh")).expect("read the bundled script");
    assert_eq!(rendered, script, "nothing between `{{#` and `#}}` is dropped");
    assert!(rendered.contains("echo keep-me"), "the cut line survives; got:\n{rendered}");
    // The warning names the file and the line the first `{#` sits on, so the
    // change of meaning is detectable rather than silent.
    assert_stderr_contains(&result, &native_path("scripts/silent.sh:3"));
    assert_stderr_contains(&result, "{#");
}

/// §FS-rhei-templates.5.3: a parse failure is anchored on the opener that was
/// never closed, not on the later line where the parser ran out of input. The
/// `{{` here opens on line 2; today the message points at line 3.
#[test]
fn instantiate_blames_the_unclosed_interpolation_on_its_own_line() {
    let notes = "line one\nline two {{ foo\nline three\n";
    let (_dir, _output_dir, result) =
        instantiate_bundling("templates-unclosed-interp", &[("notes.md", notes)]);
    assert!(!result.status.success(), "an unclosed `{{{{` is a failure; got:\n{}", result.stdout);

    assert_stderr_contains(&result, "notes.md:2");
    assert_stderr_contains(&result, "}}");
    assert_stderr_lacks(&result, "comment");
}

/// §FS-rhei-templates.5.3: and it names the opener the file actually contains.
/// An unclosed `{%` is never reported as an invalid `{{ }}` expression — the
/// fixed help string that sent two agents hunting for braces that were not
/// there is the defect §FS-rhei-errors.6 already describes.
#[test]
fn instantiate_never_blames_an_interpolation_for_an_unclosed_block() {
    let notes = "line one\n{% if title %}\nline three\n";
    let (_dir, _output_dir, result) =
        instantiate_bundling("templates-unclosed-block", &[("notes.md", notes)]);
    assert!(!result.status.success(), "an unclosed `{{%` is a failure; got:\n{}", result.stdout);

    assert_stderr_contains(&result, "notes.md:2");
    assert_stderr_contains(&result, "{%");
    assert_stderr_lacks(&result, "invalid {{ }} expression");
    assert_stderr_lacks(&result, "comment");
}

/// §FS-rhei-templates.7: a GitHub Actions `${{ ... }}` is a real interpolation
/// and stays one, so an unfenced workflow fragment still fails — but
/// §FS-rhei-templates.5.3 makes the failure name the line, the expression, and
/// the `{% raw %}` remedy instead of pointing at the manifest.
#[test]
fn instantiate_blames_the_workflow_expression_and_offers_raw() {
    let workflow = concat!(
        "name: ci\n",
        "on: [push]\n",
        "jobs:\n",
        "  build:\n",
        "    runs-on: ubuntu-latest\n",
        "    steps:\n",
        "      - run: echo ${{ github.sha }}\n",
    );
    let (_dir, _output_dir, result) =
        instantiate_bundling("templates-workflow-expr", &[("workflows/ci.yml", workflow)]);
    assert!(!result.status.success(), "an undeclared input is a failure; got:\n{}", result.stdout);

    assert_stderr_contains(&result, "ci.yml:7");
    assert_stderr_contains(&result, "github");
    assert_stderr_contains(&result, "{% raw %}");
}

/// §FS-rhei-templates.5.2: the escapes that exist keep working. A `{# ... #}`
/// inside a raw block is emitted literally, `\{{` still consumes its backslash,
/// and a runtime `{name}` still passes through. A raw region is never blamed
/// and never warned about, so the four shared templates that fence their Bash
/// bodies render exactly as they do now. This one passes today: it is the guard
/// that the change above does not take the escapes with it.
#[test]
fn instantiate_preserves_raw_blocks_and_backslash_escapes() {
    let notes =
        "raw: {% raw %}{# x #}{% endraw %}\nesc: \\{{ not_an_input }}\nruntime: {task_id}\n";
    let (_dir, output_dir, result) =
        instantiate_bundling("templates-escapes", &[("notes.md", notes)]);
    assert_success(&result);

    let rendered = fs::read_to_string(output_dir.join("notes.md")).expect("read rendered notes");
    assert_eq!(rendered, "raw: {# x #}\nesc: {{ not_an_input }}\nruntime: {task_id}\n");
    assert_stderr_lacks(&result, "notes.md:");
}

/// §FS-rhei-templates.5.2: a raw region and a bundled script in one file. The
/// `{{` the region documents is literal text and pairs with nothing, so the
/// `${#A[@]}` below it is still hidden from the parser and the interpolation
/// past both still resolves. Reading that literal `{{` as an opener is how the
/// ticket's own failure came back: it paired with the `}}` two lines later and
/// left the `{#` between them for MiniJinja to read as a comment.
#[test]
fn instantiate_keeps_a_bundled_array_length_beside_a_raw_region() {
    let notes = "{% raw %}\nUse {{ to open an expression.\n{% endraw %}\n\
                 A=(x y); echo \"${#A[@]}\"\ncount: {{ 1 + 1 }}\n";
    let (_dir, output_dir, result) =
        instantiate_bundling("templates-raw-and-bash", &[("notes.md", notes)]);
    assert_success(&result);

    let rendered = fs::read_to_string(output_dir.join("notes.md")).expect("read rendered notes");
    assert_eq!(
        rendered, "\nUse {{ to open an expression.\n\nA=(x y); echo \"${#A[@]}\"\ncount: 2\n",
        "a raw region left the Bash line beside it unrendered"
    );
    // The `{#` is text, so the change of meaning is still announced — on the
    // line it sits on, and without the word the ticket was sent hunting for.
    assert_stderr_contains(&result, "notes.md:4");
    assert_stderr_lacks(&result, "comment");
}

/// §FS-rhei-templates.5.2: a `{%` inside a raw region is literal text like the
/// `{{` above it. `printf '{%s}'` in a fenced Bash body has no `%}` of its own,
/// so reading it as a tag hands it the `{% endraw %}` on the next line: the
/// region never ends, the `${#A[@]}` below it goes unwarned, and the `{#` in
/// the expression below that is rewritten instead of measured.
#[test]
fn instantiate_ends_a_region_that_fences_a_printf_format() {
    let script = "#!/usr/bin/env bash\n{% raw %}\nprintf '{%s}\\n' \"$1\"\n{% endraw %}\n\
                  A=(x y); echo \"${#A[@]}\"\nmarker: {{ \"a{#b\" | length }}\n";
    let (_dir, output_dir, result) =
        instantiate_bundling("templates-raw-printf", &[("scripts/emit.sh", script)]);
    assert_success(&result);

    let rendered =
        fs::read_to_string(output_dir.join("scripts/emit.sh")).expect("read the bundled script");
    assert_eq!(
        rendered,
        "#!/usr/bin/env bash\n\nprintf '{%s}\\n' \"$1\"\n\nA=(x y); echo \"${#A[@]}\"\nmarker: 4\n",
        "the fenced body moved, or the expression below it measured text it does not hold"
    );
    // The `{#` on line 5 is text outside the region, so it is warned about on
    // its own line. §FS-rhei-templates.5.3
    assert_stderr_contains(&result, &native_path("scripts/emit.sh:5"));
    assert_stderr_lacks(&result, "comment");
}

/// §FS-rhei-templates.5.3: and a parse failure below such a region names the
/// opener that is really unclosed, on its own line. The region ended on line 4,
/// so it is not blamed and the remedy does not ask for a `{% endraw %}` the
/// file already has.
#[test]
fn instantiate_never_blames_a_region_that_fences_a_printf_format() {
    let script = "#!/usr/bin/env bash\n{% raw %}\nprintf '{%s}\\n' \"$1\"\n{% endraw %}\n\
                  echo \"{{ title }\"\n";
    let (_dir, _output_dir, result) =
        instantiate_bundling("templates-raw-printf-unclosed", &[("scripts/emit.sh", script)]);
    assert!(!result.status.success(), "an unclosed `{{{{` is a failure; got:\n{}", result.stdout);

    assert_stderr_contains(&result, &native_path("scripts/emit.sh:5"));
    assert_stderr_lacks(&result, &native_path("scripts/emit.sh:2"));
    assert_stderr_lacks(&result, "is never closed by `{% endraw %}`");
}
