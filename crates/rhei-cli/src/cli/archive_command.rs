// `rhei archive`: pack one finished run's evidence into one file `rhei verify`
// checks away from its workspace, and print what was written or why nothing
// was. §FS-rhei-archive

/// `rhei archive`'s arguments. §FS-rhei-archive.1
#[derive(Args, Debug)]
struct ArchiveOptions {
    /// Path to the markdown plan file (.rhei.md) or workspace directory
    #[arg(value_name = "RHEI_PLAN_OR_WORKSPACE", add = ArgValueCompleter::new(complete_rhei_plan_path))]
    input: Option<PathBuf>,
    /// The run to pack: its directory under runtime/evidence/runs/
    #[arg(long, value_name = "ID")]
    run: String,
    /// The file to write; refused when it exists or lies under runtime/evidence/
    #[arg(long, value_name = "FILE", add = ArgValueCompleter::new(complete_any_path))]
    output: PathBuf,
    /// Print one JSON object instead of text
    #[arg(long)]
    json: bool,
}

/// `rhei archive --help`'s long description, with what an archive carries in
/// clear. §FS-rhei-archive.5 §FS-rhei-archive.6
const ARCHIVE_LONG_ABOUT: &str = "\
Pack one finished run's evidence into one file that `rhei verify <FILE>` checks away from its workspace

The archive is an uncompressed tar holding the run's manifest, every file it names, the reset
events that list the run, and the snapshot generations it names. The run is verified in place
first, and each member again as it is written; rhei writes only --output, never replacing a file.
Exits 0 when the archive was written; 1 when the run is unknown or not finalized, a check fails,
the output exists, or a write fails, and nothing is written then; 2 on a usage error.

An archive is meant to leave the machine, so know what it carries in clear:
  - hash-only and the redactor reach only retained values; logs are never redacted, and they often
    repeat the prompt;
  - start records carry argv unredacted and the absolute cwd;
  - seal generations name the user settings path;
  - a transcript is redacted only if the redactor ran when its generation was emitted;
  - \"no absolute path enters the archive\" is true of member names only.";

/// `--output` made absolute against the working directory.
fn archive_output_path(output: &Path) -> PathBuf {
    if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| output.to_path_buf(), |cwd| cwd.join(output))
    }
}

/// Whether `output` lies under the root's `runtime/evidence/`, which a usage
/// error refuses. §FS-rhei-archive.1
fn archive_output_under_evidence(root: &Path, output: &Path) -> bool {
    let parent = output.parent().unwrap_or(output);
    let parent =
        rhei_core::platform::canonical_path(parent).unwrap_or_else(|_| parent.to_path_buf());
    parent.starts_with(evidence_dir(root))
}

/// The written archive's line, or the refusal: the run's block and findings as
/// `rhei verify` prints them, then a line saying nothing was written.
/// §FS-rhei-archive.5
fn render_archive_text(
    outcome: &VerifyOutcome,
    written: Option<&WrittenArchive>,
    output: &Path,
) -> String {
    let (Some(written), Some(run)) = (written, outcome.runs.first()) else {
        return format!("{}nothing written to {}\n", render_verify_text(outcome), output.display());
    };
    format!(
        "archived run {}  manifest sha256:{}  {}  {}\n  wrote {}  sha256:{}\n",
        run.run_id,
        run.digest.as_deref().unwrap_or_default(),
        plural(written.members, "member", "members"),
        plural(written.generations, "snapshot generation", "snapshot generations"),
        written.path.display(),
        written.sha256
    )
}

/// The `rhei verify --json` object plus `archive`, the file written or `null`.
/// §FS-rhei-archive.5.1
fn render_archive_json(
    outcome: &VerifyOutcome,
    written: Option<&WrittenArchive>,
) -> serde_json::Value {
    let mut json = render_verify_json(outcome);
    json["verified"] = serde_json::Value::Bool(written.is_some());
    json["archive"] = written.map_or(serde_json::Value::Null, |written| {
        serde_json::json!({ "path": written.path.display().to_string(), "sha256": written.sha256 })
    });
    json
}

/// Resolve the run's root as `rhei verify` does, pack the run, print, and exit
/// `1` when nothing was written. §FS-rhei-archive.1 §FS-rhei-archive.5
fn archive_command(options: ArchiveOptions) -> MietteResult<()> {
    let ArchiveOptions { input, run, output, json } = options;
    let root = evidence_root_for(resolve_plan_target(input)?.path());
    let output = archive_output_path(&output);
    if archive_output_under_evidence(&root, &output) {
        let message =
            format!("--output {} lies under the run's runtime/evidence/", output.display());
        cli_command().error(ErrorKind::ValueValidation, message).exit();
    }
    let (outcome, written) = pack_archive(&root, &run, &output);
    if json {
        println!("{}", render_archive_json(&outcome, written.as_ref()));
    } else {
        print!("{}", render_archive_text(&outcome, written.as_ref(), &output));
    }
    if written.is_none() {
        std::process::exit(1);
    }
    Ok(())
}
