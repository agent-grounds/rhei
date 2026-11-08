// The words a help says right before a shell command it offers as the next
// action, one constant per offer, shared by the message that prints it and by
// the renderer that keeps that command whole on one line.
//
// Recognized by the sentence that offers the command, never by the command's
// spelling: `mkdir -p` and `ls -ld` are POSIX words, and a platform that spells
// the remedy differently must still have it kept whole.

// §AR-source-file-size.3 §FS-rhei-errors.2.1 §REQ-cross-platform.2

use rhei_core::budget::{
    RESTORE_LEAD_IN, SET_ASIDE_INSTEAD_LEAD_IN, SET_ASIDE_LEAD_IN, TRIM_LEAD_IN,
};

/// A name whose directory exists: list that directory. §FS-rhei-errors.6
const CHECK_SPELLING_LEAD_IN: &str = "Check the spelling: ";

/// A directory that does not exist: create it. §FS-rhei-errors.6
const CREATE_IT_LEAD_IN: &str = "Create it with: ";

/// A path the user cannot reach: show who owns it and how. §FS-rhei-errors.6
const INSPECT_IT_LEAD_IN: &str = "Inspect it with: ";

/// A path that is already there: show what is. §FS-rhei-errors.6
const REMOVE_OR_PICK_LEAD_IN: &str = "Remove it or pick another: ";

/// A description file that is not there: list where it was looked for.
/// §FS-rhei-new.1.1
const LOOK_WITH_LEAD_IN: &str = "Look with: ";

/// An `--output` that already exists: remove it. §FS-rhei-templates.6.2
const REMOVE_THAT_ONE_LEAD_IN: &str = "remove that one: ";

/// One link a rebind would write through: remove it, and not what it points to.
/// §FS-rhei-library.2.2
const UNLINK_ONE_LEAD_IN: &str = "remove the link (";

/// Several links a rebind would write through. §FS-rhei-library.2.2
const UNLINK_MANY_LEAD_IN: &str = "remove the links (";

/// Every lead-in above, which is the whole of what the renderer looks for.
const OFFERED_COMMAND_LEAD_INS: &[&str] = &[
    CHECK_SPELLING_LEAD_IN,
    CREATE_IT_LEAD_IN,
    INSPECT_IT_LEAD_IN,
    REMOVE_OR_PICK_LEAD_IN,
    LOOK_WITH_LEAD_IN,
    REMOVE_THAT_ONE_LEAD_IN,
    UNLINK_ONE_LEAD_IN,
    UNLINK_MANY_LEAD_IN,
    SET_ASIDE_LEAD_IN,
    SET_ASIDE_INSTEAD_LEAD_IN,
    TRIM_LEAD_IN,
    RESTORE_LEAD_IN,
];

/// Whether a diagnostic's text offers a shell command as the next action, which
/// the renderer then keeps on the physical line it starts on. §FS-rhei-errors.2.1
///
/// It reads the text rather than asking who built it, because a detached run's
/// refusal reaches its launcher as text. A false positive only leaves prose
/// unwrapped, for the terminal to soft-wrap.
fn offers_a_command(text: &str) -> bool {
    OFFERED_COMMAND_LEAD_INS.iter().any(|lead_in| text.contains(lead_in))
}
