//! ANSI-C quoting ($'...') escape sequence processing.
//!
//! Re-exported as `pub(super)` so `word.rs` can access it.

/// Process ANSI-C escape sequences inside `$'...'`.
/// `chars` is the character array, `pos` points to the first char of content.
/// Returns the processed content (without surrounding quotes).
/// Advances `pos` past the content.
#[allow(clippy::too_many_lines)]
pub(super) fn process_ansi_c_content(chars: &[char], pos: &mut usize) -> String {
    // Delegate to the implementation in mod.rs for now.
    // This will be moved fully here in a later step.
    super::process_ansi_c_content(chars, pos)
}
