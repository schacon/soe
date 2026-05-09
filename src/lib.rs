//! A built-in TUI text editor for CLI tools.
//!
//! Drop-in fallback editor when no external editor (`$EDITOR`, `$VISUAL`,
//! `core.editor`) is configured. Also usable as a standalone file editor.
//!
//! ```no_run
//! use tui_edit::{edit, EditorMode};
//!
//! // Open with initial content, returns Some(content) on save, None on cancel
//! let result = edit("commit message", "fix: resolve panic on empty input", EditorMode::CommitMessage)?;
//!
//! // Edit a file on disk
//! tui_edit::edit_file(std::path::Path::new("README.md"))?;
//! # Ok::<(), anyhow::Error>(())
//! ```

mod editor;
mod terminal;

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
