//! Driving `rhei archive` and `rhei verify <archive>`, and reading or rewriting
//! the tar files they work on: the members, the index, a tampered byte, and
//! hand-built entries no packer would write. §FS-rhei-archive §FS-rhei-verify.2.1

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::evidence_support::{run_dirs, EvidenceWorkspace};
use super::{rhei_command, CliRun};

/// The id of the one run under `root`'s evidence store.
pub fn only_run_id(root: &Path) -> String {
    let dirs = run_dirs(&root.join("runtime/evidence"));
    assert_eq!(dirs.len(), 1, "fixture: one run under {}", root.display());
    dirs[0].file_name().expect("run id").to_string_lossy().into_owned()
}

/// `rhei <args…>` from `cwd`, with `home` as the isolated home.
pub fn rhei_in(cwd: &Path, home: &Path, args: &[&str]) -> CliRun {
    let output = rhei_command(home).current_dir(cwd).args(args).output().expect("rhei runs");
    CliRun::from(&output)
}

/// `rhei archive <root> --run <run> --output <output> [extra…]` from the root.
pub fn archive(root: &Path, run: &str, output: &Path, extra: &[&str]) -> CliRun {
    let root_arg = root.display().to_string();
    let output_arg = output.display().to_string();
    let mut args = vec!["archive", root_arg.as_str(), "--run", run, "--output", &output_arg];
    args.extend_from_slice(extra);
    rhei_in(root, &root.join(".home"), &args)
}

/// [`archive`] of the workspace's one run, which must succeed.
pub fn archive_ok(ws: &EvidenceWorkspace, output: &Path) -> CliRun {
    let run = only_run_id(&ws.root);
    let result = archive(&ws.root, &run, output, &[]);
    assert!(
        result.status.success() && output.is_file(),
        "rhei archive writes {}:\n{}\n{}",
        output.display(),
        result.stdout,
        result.stderr
    );
    result
}

/// `rhei verify <archive> [extra…]` from `cwd`, a directory away from any
/// workspace.
pub fn verify_archive(cwd: &Path, archive: &Path, extra: &[&str]) -> CliRun {
    let archive_arg = archive.display().to_string();
    let mut args = vec!["verify", archive_arg.as_str()];
    args.extend_from_slice(extra);
    rhei_in(cwd, &cwd.join(".home"), &args)
}

/// [`verify_archive`] with `--json`, parsed.
pub fn verify_archive_json(
    cwd: &Path,
    archive: &Path,
    extra: &[&str],
) -> (CliRun, serde_json::Value) {
    let mut args = vec!["--json"];
    args.extend_from_slice(extra);
    let result = verify_archive(cwd, archive, &args);
    let json = serde_json::from_str(&result.stdout).unwrap_or_else(|err| {
        panic!("verify --json prints one object: {err}\n{}\n{}", result.stdout, result.stderr)
    });
    (result, json)
}

/// Every member of `archive`, in order, as `(path, bytes)`.
pub fn members(archive: &Path) -> Vec<(String, Vec<u8>)> {
    let file = fs::File::open(archive).expect("open archive");
    let mut reader = tar::Archive::new(file);
    let mut out = Vec::new();
    for entry in reader.entries().expect("tar entries") {
        let mut entry = entry.expect("tar entry");
        let path = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("member bytes");
        out.push((path, bytes));
    }
    out
}

/// The parsed index, which must be the first member.
pub fn index(archive: &Path) -> serde_json::Value {
    let all = members(archive);
    let (path, bytes) = all.first().expect("an archive has members");
    assert_eq!(path, "rhei-archive.json", "the index is the first member");
    serde_json::from_slice(bytes).expect("the index is JSON")
}

/// Flip one byte in the middle of `member`'s data, in place, keeping its length.
pub fn flip_member_byte(archive: &Path, member: &str) {
    let file = fs::File::open(archive).expect("open archive");
    let mut reader = tar::Archive::new(file);
    let mut at = None;
    for entry in reader.entries().expect("tar entries") {
        let entry = entry.expect("tar entry");
        if entry.path_bytes().as_ref() == member.as_bytes() {
            assert!(entry.size() > 0, "fixture: {member} is not empty");
            at = Some(entry.raw_file_position() + entry.size() / 2);
        }
    }
    let at = at.unwrap_or_else(|| panic!("fixture: no member {member}")) as usize;
    let mut bytes = fs::read(archive).expect("archive bytes");
    bytes[at] ^= 0x20;
    fs::write(archive, bytes).expect("write tampered archive");
}

/// Copy `src` to `dst` member by member: `edit` returns a member's new bytes
/// or `None` to drop it; `extra` members are appended after the rest.
pub fn rewrite(
    src: &Path,
    dst: &Path,
    mut edit: impl FnMut(&str, Vec<u8>) -> Option<Vec<u8>>,
    extra: &[(String, Vec<u8>)],
) {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in members(src) {
        if let Some(bytes) = edit(&path, bytes) {
            append(&mut builder, &path, &bytes);
        }
    }
    for (path, bytes) in extra {
        append(&mut builder, path, bytes);
    }
    fs::write(dst, builder.into_inner().expect("finish tar")).expect("write archive");
}

/// One regular member, through the GNU long-name extension when it needs one.
pub fn append(builder: &mut tar::Builder<Vec<u8>>, path: &str, bytes: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_entry_type(tar::EntryType::Regular);
    builder.append_data(&mut header, path, bytes).expect("append member");
}

/// A member whose name field is `name` verbatim — unchecked, so `../x` and
/// `/etc/x` go in as written — of `kind`, linking to `link` when one is given.
pub fn append_raw(
    builder: &mut tar::Builder<Vec<u8>>,
    name: &str,
    kind: tar::EntryType,
    link: Option<&str>,
    bytes: &[u8],
) {
    let mut header = tar::Header::new_ustar();
    let field = &mut header.as_old_mut().name;
    field[..name.len()].copy_from_slice(name.as_bytes());
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_entry_type(kind);
    if let Some(link) = link {
        header.set_link_name(link).expect("link name");
    }
    header.set_cksum();
    builder.append(&header, bytes).expect("append raw member");
}

/// `src`'s members followed by whatever `add` appends, written to `dst`.
pub fn with_appended(src: &Path, dst: &Path, add: impl FnOnce(&mut tar::Builder<Vec<u8>>)) {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in members(src) {
        append(&mut builder, &path, &bytes);
    }
    add(&mut builder);
    fs::write(dst, builder.into_inner().expect("finish tar")).expect("write archive");
}

/// A fresh directory, away from every workspace, to verify an archive from.
pub fn elsewhere(prefix: &str) -> (super::TestDir, PathBuf) {
    let dir = super::unique_temp_dir(prefix);
    let path = dir.to_path_buf();
    (dir, path)
}

/// Exit 1 with a finding of `code` in `json`; returns the finding.
pub fn assert_refused<'a>(
    result: &CliRun,
    json: &'a serde_json::Value,
    code: &str,
) -> &'a serde_json::Value {
    assert_eq!(result.status.code(), Some(1), "{code} fails: {json:#}\n{}", result.stderr);
    assert_eq!(json["verified"], false, "{json:#}");
    super::verify_findings_tests::finding(json, code)
        .unwrap_or_else(|| panic!("a {code} finding: {json:#}"))
}
