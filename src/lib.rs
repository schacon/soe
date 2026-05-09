//! Scott's Own Editor — a built-in TUI text editor for CLI tools.
//!
//! Provides a single entry point that resolves the best available editor
//! using Git's precedence (`$GIT_EDITOR` → `core.editor` → `$VISUAL` →
//! `$EDITOR`) and falls back to a built-in TUI editor when none is configured.
//!
//! ```no_run
//! // One call — handles external editors and built-in fallback automatically
//! let result = soe::capture("Enter your message (lines starting with # are ignored)")?;
//!
//! // Or with pre-filled content
//! let result = soe::capture_with_initial("Edit the description", "existing text here")?;
//!
//! // Direct access to the built-in TUI editor
//! let result = soe::edit("filename", "initial content", soe::EditorMode::PlainText)?;
//! # Ok::<(), anyhow::Error>(())
//! ```

mod capture;
mod editor;
mod terminal;

pub use capture::{capture, capture_with_initial};
pub use editor::EditorMode;

/// Open the built-in TUI editor with initial content.
///
/// - `filename` is shown in the UI (doesn't touch disk).
/// - `initial_content` is pre-loaded into the buffer.
/// - `mode` controls wrapping and guide lines.
///
/// Returns `Some(content)` if the user saved, `None` if cancelled.
pub fn edit(
    filename: &str,
    initial_content: &str,
    mode: EditorMode,
) -> anyhow::Result<Option<String>> {
    editor::run_builtin_editor(filename, initial_content, mode)
}

/// Open the built-in editor for a file on disk.
///
/// Reads the file (or starts empty if it doesn't exist), lets the user
/// edit, and writes it back on save.
pub fn edit_file(path: &std::path::Path) -> anyhow::Result<()> {
    editor::edit_file(path)
}
