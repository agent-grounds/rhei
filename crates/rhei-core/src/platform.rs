//! What the three supported platforms disagree about, in one place.
//!
//! A command line goes to a different shell on each, and a canonical path is
//! spelled differently on one of them. Both are facts about the host rather
//! than about plans, callbacks, or programs, and both were getting decided at
//! the point of use — which is how one of them stayed Unix-only and the other
//! leaked a spelling nothing else in the run used.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The platform's own shell, holding one command line.
///
/// A string-form command runs under `/bin/sh -c` on Unix and `cmd /c` on
/// Windows, and a `cli:` callback is the same kind of value — a command line,
/// not an argument vector — so it takes the same shell. Only the Unix half was
/// ever spawned, so on Windows every string-form program and every `cli:`
/// callback died looking for a program named `sh` before its first own
/// instruction.

// §FS-rhei-programs.1.1 §REQ-cross-platform.2
pub fn system_shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        // `raw_arg`, not `arg`: `cmd /C` takes a command *line*, and the
        // escaping `arg` applies quotes the whole thing and rewrites every `"`
        // inside it as `\"` — which `cmd` does not understand, so the quotes
        // reached the program as characters and `python -c "…"` was handed a
        // string literal that never closed.
        //
        // `/S` with the whole line in one pair of quotes is the only shape
        // `cmd` reads back unchanged: given `/S`, it strips the first and last
        // quote and runs the rest verbatim. Without the wrapping quotes, `cmd`
        // applies its own rule — strip the quotes around the *program* when
        // the line looks a certain way — and a quoted program path such as
        // `"C:\Program Files\Python\python.exe" -c "..."` comes apart.
        use std::os::windows::process::CommandExt as _;
        let mut cmd = Command::new("cmd");
        cmd.arg("/S").arg("/C");
        cmd.raw_arg(format!("\"{command}\""));
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}

/// Quote one word so a printed command survives a paste into an interactive
/// shell — the platform's own, since that is the shell the user is holding.
///
/// Both halves are compiled everywhere and chosen at runtime, so the platform
/// that runs the whole suite pins the other one's spelling too.
// §FS-rhei-errors.2 §REQ-cross-platform.2
pub fn shell_quote(value: &str) -> String {
    Dialect::of_this_platform().quote(value)
}

/// The platform shell's own copy of one file over another, both paths quoted.
///
/// A printed remedy is a command line for the shell the operator is holding, so
/// which words it is spelled in is a fact about the host rather than about what
/// the remedy is for: `cp` is not a program `cmd` has, and `copy` is not one a
/// POSIX shell does. §FS-rhei-budgets.10 §REQ-cross-platform.2
pub fn copy_command(from: &Path, to: &Path) -> String {
    Dialect::of_this_platform().copy(from, to)
}

/// The platform shell's own creation of a directory and every missing parent.
///
/// `mkdir -p` is the POSIX spelling; `cmd` makes the whole chain with its own
/// `mkdir` under command extensions, which are on by default, and would read a
/// `-p` as another directory to create. §FS-rhei-budgets.10 §REQ-cross-platform.2
pub fn make_directory_command(directory: &Path) -> String {
    Dialect::of_this_platform().make_directory(directory)
}

/// Which shell a printed command line is written for.
///
/// Named rather than left to `cfg!` at each point of use, so both halves are
/// compiled everywhere and the platform that runs the whole suite pins the other
/// one's spelling too — the alternative is a Windows command line no test on
/// Linux can read. §REQ-cross-platform.2
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dialect {
    Posix,
    Cmd,
}

impl Dialect {
    fn of_this_platform() -> Self {
        if cfg!(windows) {
            Self::Cmd
        } else {
            Self::Posix
        }
    }

    fn quote(self, value: &str) -> String {
        match self {
            Self::Posix => quote_for_posix(value),
            Self::Cmd => quote_for_cmd(value),
        }
    }

    /// A path as one shell word. Quoted through the same rule a bare value
    /// takes, so a path holding a space is one argument rather than two.
    /// §FS-rhei-budgets.10
    fn path(self, path: &Path) -> String {
        self.quote(&path.display().to_string())
    }

    fn copy(self, from: &Path, to: &Path) -> String {
        let verb = match self {
            Self::Posix => "cp",
            Self::Cmd => "copy",
        };
        format!("{verb} {} {}", self.path(from), self.path(to))
    }

    fn make_directory(self, directory: &Path) -> String {
        let verb = match self {
            Self::Posix => "mkdir -p",
            Self::Cmd => "mkdir",
        };
        format!("{verb} {}", self.path(directory))
    }
}

/// POSIX single quotes, with an embedded `'` spliced as `'"'"'`.
fn quote_for_posix(value: &str) -> String {
    // zsh expands `[`/`]` before the command runs, so an unquoted
    // `agent=codex[yolo]:openai:gpt-5.5` dies with `no matches found`.
    if value.is_empty() {
        return "''".to_string();
    }
    // A word that *begins* with `=` is subject to zsh's EQUALS expansion
    // (`=less` becomes the path to `less`), so it has to be quoted even though
    // `=` is safe everywhere else in a word.
    if !value.starts_with('=')
        && value.bytes().all(|byte| {
            matches!(
                byte,
                b'a'..=b'z'
                    | b'A'..=b'Z'
                    | b'0'..=b'9'
                    | b'_'
                    | b'-'
                    | b'.'
                    | b'/'
                    | b':'
                    | b'@'
                    | b'%'
                    | b'+'
                    | b'='
                    | b','
            )
        })
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// `cmd`'s double quotes, with an embedded `"` doubled.
///
/// `cmd` has no single-quote form at all — `'C:\a b\p.exe'` is a program name
/// beginning with an apostrophe — and no backslash escape either, since a
/// backslash is its path separator. Doubling is how a literal `"` is written
/// inside a quoted argument.
fn quote_for_cmd(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_string();
    }
    // `\` joins the safe set — it is the separator every Windows path is full
    // of — and `%` leaves it, being what `cmd` expands variables with.
    if value.bytes().all(|byte| {
        matches!(
            byte,
            b'a'..=b'z'
                | b'A'..=b'Z'
                | b'0'..=b'9'
                | b'_'
                | b'-'
                | b'.'
                | b'/'
                | b'\\'
                | b':'
                | b'@'
                | b'+'
                | b'='
                | b','
        )
    }) {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// `path` without the `\\?\` verbatim prefix Windows canonicalization adds.
///
/// The verbatim form is a second spelling of one location, and it does not stay
/// inside the process that made it: a report prints it where every other line
/// prints the plain form, and a worker handed one writes its artifacts where
/// the engine does not look for them.
///
/// For a drive path it also fixes the working directory: `cmd.exe` refuses to
/// start in `\\?\C:\work`, says so on stderr, and silently uses the Windows
/// directory instead, where `C:\work` is a directory it starts in happily. That
/// half is the drive case only — `cmd.exe` supports no UNC current directory at
/// all, so `\\server\share` is no more startable than `\\?\UNC\server\share`;
/// stripping the prefix there buys the printing and the artifacts, not the
/// callback's cwd.

// §REQ-cross-platform.5
pub fn plain_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(text) = path.to_str() {
            if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
                return PathBuf::from(format!(r"\\{rest}"));
            }
            if let Some(rest) = text.strip_prefix(r"\\?\") {
                // A drive path only; `\\?\Volume{…}` has no plain spelling.
                if rest.as_bytes().get(1) == Some(&b':') {
                    return PathBuf::from(rest);
                }
            }
        }
    }
    path
}

/// [`Path::canonicalize`] with the verbatim prefix taken back off.
// §REQ-cross-platform.5
pub fn canonical_path(path: &Path) -> std::io::Result<PathBuf> {
    path.canonicalize().map(plain_path)
}

/// Whether a path has a platform root or drive prefix. §FS-rhei-states.1.3
pub fn path_is_rooted(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .components()
        .any(|part| matches!(part, std::path::Component::Prefix(_) | std::path::Component::RootDir))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The program and arguments a shell command is spawned with.
    fn argv(command: &str) -> (String, Vec<String>) {
        let cmd = system_shell_command(command);
        let program = cmd.get_program().to_string_lossy().into_owned();
        let args = cmd.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();
        (program, args)
    }

    /// The command line a fixture is most likely to hand a shell: a quoted
    /// program path, and quoted arguments of its own.
    const QUOTED_PROGRAM_LINE: &str = r#""C:\Program Files\p.exe" -c "print(1)""#;

    /// Unix takes the line as one argument to `sh -c`, unchanged: `sh` is
    /// handed an argument vector, not a line to re-parse.
    // §FS-rhei-programs.1.1
    #[cfg(not(windows))]
    #[test]
    fn a_string_command_goes_to_sh_dash_c_unchanged() {
        let (program, args) = argv(QUOTED_PROGRAM_LINE);
        assert_eq!(program, "sh");
        assert_eq!(args, vec!["-c".to_string(), QUOTED_PROGRAM_LINE.to_string()]);
    }

    /// Windows takes `/S` and the whole line inside one added pair of quotes,
    /// which is the shape `cmd` hands back verbatim — so a quoted program path
    /// reaches the program as it was written.
    // §FS-rhei-programs.1.1 §REQ-cross-platform.2
    #[cfg(windows)]
    #[test]
    fn a_string_command_goes_to_cmd_s_c_inside_one_pair_of_quotes() {
        let (program, args) = argv(QUOTED_PROGRAM_LINE);
        assert_eq!(program, "cmd");
        assert_eq!(
            args,
            vec!["/S".to_string(), "/C".to_string(), format!("\"{QUOTED_PROGRAM_LINE}\""),]
        );
    }

    /// The POSIX half: bare when it can be, single-quoted when it cannot, and
    /// quoted for zsh's two expansions even where `sh` would not need it.
    // §FS-rhei-errors.2
    #[test]
    fn a_printed_value_is_posix_quoted_for_a_unix_shell() {
        assert_eq!(quote_for_posix("/tmp/plan.rhei.md"), "/tmp/plan.rhei.md");
        assert_eq!(quote_for_posix(""), "''");
        assert_eq!(quote_for_posix("=less"), "'=less'");
        assert_eq!(
            quote_for_posix("agent=codex[yolo]:openai:gpt-5.5"),
            "'agent=codex[yolo]:openai:gpt-5.5'"
        );
        assert_eq!(quote_for_posix("a b"), "'a b'");
        assert_eq!(quote_for_posix("it's"), r#"'it'"'"'s'"#);
    }

    /// The Windows half: `cmd` reads no single quotes, a backslash is a
    /// separator rather than an escape, and a literal `"` is written twice.
    // §FS-rhei-errors.2 §REQ-cross-platform.2
    #[test]
    fn a_printed_value_is_cmd_quoted_for_a_windows_shell() {
        assert_eq!(quote_for_cmd(r"C:\work\plan.rhei.md"), r"C:\work\plan.rhei.md");
        assert_eq!(quote_for_cmd(""), r#""""#);
        assert_eq!(
            quote_for_cmd(r"C:\Program Files\plan.rhei.md"),
            r#""C:\Program Files\plan.rhei.md""#
        );
        assert_eq!(quote_for_cmd(r#"say "hi""#), r#""say ""hi""""#);
        // A backslash before the closing quote stays a separator, not an
        // escape: `cmd` never reads one as escaping the quote.
        assert_eq!(quote_for_cmd(r"C:\a b\"), r#""C:\a b\""#);
        assert_eq!(quote_for_cmd("50%"), r#""50%""#);
    }

    /// The two commands a printed remedy is made of, in both dialects, on
    /// whichever platform runs the suite. A path holding a space is one word in
    /// each, and each names a program its own shell actually has.
    // §FS-rhei-budgets.10 §REQ-cross-platform.2
    #[test]
    fn a_printed_remedy_is_spelled_in_the_shell_it_is_offered_to() {
        let spaced = Path::new("/tmp/my projects/ss/journal.jsonl");
        assert_eq!(
            Dialect::Posix.copy(Path::new("/w/history.jsonl"), Path::new("/w/journal.jsonl")),
            "cp /w/history.jsonl /w/journal.jsonl"
        );
        assert_eq!(
            Dialect::Posix.copy(Path::new("/w/history.jsonl"), spaced),
            "cp /w/history.jsonl '/tmp/my projects/ss/journal.jsonl'"
        );
        assert_eq!(
            Dialect::Posix.make_directory(spaced),
            "mkdir -p '/tmp/my projects/ss/journal.jsonl'"
        );
        assert_eq!(
            Dialect::Cmd.copy(Path::new(r"C:\w\history.jsonl"), Path::new(r"C:\w\journal.jsonl")),
            r"copy C:\w\history.jsonl C:\w\journal.jsonl"
        );
        assert_eq!(
            Dialect::Cmd
                .copy(Path::new(r"C:\w\history.jsonl"), Path::new(r"C:\my projects\journal.jsonl")),
            r#"copy C:\w\history.jsonl "C:\my projects\journal.jsonl""#
        );
        // `-p` is a directory name to `cmd`, which makes the whole chain anyway.
        assert_eq!(Dialect::Cmd.make_directory(Path::new(r"C:\w\b")), r"mkdir C:\w\b");
    }

    /// The public pair is this platform's dialect and nothing else, so a caller
    /// never has to ask which one it got.
    // §FS-rhei-budgets.10
    #[test]
    fn the_printed_remedy_takes_this_platforms_dialect() {
        let from = Path::new("from");
        let to = Path::new("to");
        assert_eq!(copy_command(from, to), Dialect::of_this_platform().copy(from, to));
        assert_eq!(make_directory_command(to), Dialect::of_this_platform().make_directory(to));
        assert!(copy_command(from, to).starts_with(if cfg!(windows) { "copy " } else { "cp " }));
    }

    /// The verbatim prefix is stripped only where a plain spelling exists, and
    /// both halves are pinned on the platform that runs the whole suite.
    // §REQ-cross-platform.5
    #[test]
    fn a_canonical_windows_path_loses_its_verbatim_prefix() {
        let drive = plain_path(PathBuf::from(r"\\?\C:\work\plan.rhei.md"));
        let unc = plain_path(PathBuf::from(r"\\?\UNC\server\share\plan.rhei.md"));
        let volume = plain_path(PathBuf::from(r"\\?\Volume{2eca078d}\plan.rhei.md"));
        if cfg!(windows) {
            assert_eq!(drive, PathBuf::from(r"C:\work\plan.rhei.md"));
            assert_eq!(unc, PathBuf::from(r"\\server\share\plan.rhei.md"));
            // A volume GUID path has no plain form, so it is left alone.
            assert_eq!(volume, PathBuf::from(r"\\?\Volume{2eca078d}\plan.rhei.md"));
        } else {
            // Nothing to strip: these are ordinary relative names elsewhere.
            assert_eq!(drive, PathBuf::from(r"\\?\C:\work\plan.rhei.md"));
            assert_eq!(unc, PathBuf::from(r"\\?\UNC\server\share\plan.rhei.md"));
            assert_eq!(volume, PathBuf::from(r"\\?\Volume{2eca078d}\plan.rhei.md"));
        }
    }
}
