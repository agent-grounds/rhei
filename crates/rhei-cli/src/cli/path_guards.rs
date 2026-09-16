// One question about a declared path — does it start at a root, or inside the
// place it is resolved against — asked by the state-artifact validator, the
// legacy result-link reader, and the TUI's artifact excerpt.
//
// Its own part because the answer is a platform fact rather than any of those
// three behaviors, and all three had spelled it themselves and got it wrong in
// the same direction.

// §AR-source-file-size.3 §FS-rhei-states.1.3

/// Whether `path` names a place by starting at a root — a filesystem root, a
/// drive, or a UNC share — rather than relative to whatever it is joined to.
///
/// Neither `is_absolute()` nor `has_root()` answers that on Windows, and the
/// two miss opposite cases. `/etc/passwd` there is rooted but not absolute: it
/// names no drive, so `is_absolute()` is false. `C:out.md` is the mirror — a
/// `Prefix` and no `RootDir`, drive-*relative*, so `has_root()` is false too,
/// and yet it resolves against the current directory of `C:` rather than
/// against the workspace. Walking the components asks the real question
/// instead of approximating it: does this path begin at a root of any kind?
///
/// A path that merely climbs (`../x`) is *not* rooted; escaping upward is a
/// separate check, and the callers that care make it separately.
// §FS-rhei-states.1.3
pub(crate) fn path_is_rooted(path: impl AsRef<Path>) -> bool {
    rhei_core::platform::path_is_rooted(path)
}
