//! The language's one reading of a fenced code block.
//!
//! Every reader of a plan resolves a fence the same way — the structural scan
//! that decides which `##` lines are chapters and which `### <kind> <id>:`
//! lines are task nodes, the tokenizer, the link checker and the result-block
//! scan alike. A plan read by two fence rules is a plan where the same line is
//! structure to one reader and content to another, so the rule lives here once
//! and nowhere else. §FS-rhei-plan-language.2.1

/// The fence character and its run length, when a line is a code fence.
///
/// CommonMark: a fence is a run of at least three backticks or tildes; the
/// opening one may carry an info string, the closing one may not, and the
/// closing run must be at least as long as the opening one. The third element
/// is whether the line is bare — the condition a close is held to.
// §FS-rhei-plan-language.2.1
pub fn code_fence_run(line: &str) -> Option<(char, usize, bool)> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let run = trimmed.chars().take_while(|character| *character == marker).count();
    if run < 3 {
        return None;
    }
    let bare = trimmed[run..].trim().is_empty();
    Some((marker, run, bare))
}

/// The fence a line-by-line scan is inside, if any.
///
/// The state is the opening fence's character and run length rather than a
/// boolean: a close is accepted only from a bare run of the same character at
/// least as long, so a longer fence may contain shorter ones and a run
/// carrying an info string never closes the block it sits in. Nothing resets
/// it at the end of the input, which is how an unclosed fence runs to the end
/// of the text that opened it.
// §FS-rhei-plan-language.2.1
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FenceTracker {
    open: Option<(char, usize)>,
}

impl FenceTracker {
    /// Read one line, and say whether the line is itself a fence line — the
    /// run that opened the block, or the bare run that closed it.
    ///
    /// Every other line is content while [`FenceTracker::is_open`] holds, and
    /// a candidate for structure otherwise. Call this exactly once per line,
    /// in order: it is what advances the state.
    // §FS-rhei-plan-language.2.1
    pub fn read(&mut self, line: &str) -> bool {
        match self.open {
            Some((marker, opening_run)) => {
                // Only a bare run of the same character, at least as long,
                // closes; anything else is a line of the block.
                match code_fence_run(line) {
                    Some((character, run, bare))
                        if character == marker && run >= opening_run && bare =>
                    {
                        self.open = None;
                        true
                    }
                    _ => false,
                }
            }
            None => match code_fence_run(line) {
                Some((marker, run, _)) => {
                    self.open = Some((marker, run));
                    true
                }
                None => false,
            },
        }
    }

    /// True while a fenced block is open, so no production of the grammar is
    /// recognized on the line just read.
    // §FS-rhei-plan-language.2.1
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
}

#[cfg(test)]
mod tests;
