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
//! // Raw editing (no comment prompt/stripping), same editor resolution
//! let result = soe::edit("filename.md", "initial content", soe::EditorMode::PlainText)?;
//!
//! // Direct access to the built-in TUI editor, ignoring $EDITOR etc.
//! let result = soe::edit_builtin("filename", "initial content", soe::EditorMode::PlainText)?;
//! # Ok::<(), anyhow::Error>(())
//! ```

mod capture;
mod editor;
mod terminal;

pub use capture::{capture, capture_with_initial};
pub use editor::EditorMode;

/// Edit `initial_content` in the best available editor, raw: no comment
/// prompt is added and nothing is stripped from the result (unlike
/// [`capture`], which treats `#` lines as comments).
///
/// Uses Git's editor resolution order: `$GIT_EDITOR` → `git config
/// core.editor` → `$VISUAL` → `$EDITOR` → the built-in TUI editor.
///
/// - `filename` names the buffer: it's shown in the built-in UI, and used
///   as the tempfile name for external editors (so syntax highlighting
///   matches). Nothing is written to `filename` itself.
/// - `mode` controls wrapping and guide lines in the built-in editor;
///   external editors ignore it.
///
/// Returns `Some(content)` if the user saved, `None` if cancelled (`Esc` in
/// the built-in editor, a non-zero exit like vim's `:cq` in external ones).
pub fn edit(
    filename: &str,
    initial_content: &str,
    mode: EditorMode,
) -> anyhow::Result<Option<String>> {
    match capture::resolve_editor() {
        Some(editor) => capture::edit_external(&editor, filename, initial_content),
        None => editor::run_builtin_editor(filename, initial_content, mode),
    }
}

/// Open the built-in TUI editor with initial content, ignoring any
/// configured external editor.
///
/// - `filename` is shown in the UI (doesn't touch disk).
/// - `initial_content` is pre-loaded into the buffer.
/// - `mode` controls wrapping and guide lines.
///
/// Returns `Some(content)` if the user saved, `None` if cancelled.
pub fn edit_builtin(
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
