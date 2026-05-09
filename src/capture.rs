//! High-level "give me an editor" entry point.
//!
//! Resolves which editor to use (Git's precedence), spawns it if external,
//! or falls back to the built-in TUI editor.

use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use crate::EditorMode;

/// Open the best available editor with comment-prompt instructions.
///
/// Uses Git's editor resolution order:
///   1. `$GIT_EDITOR`
///   2. `git config core.editor`
///   3. `$VISUAL`
///   4. `$EDITOR`
///   5. Built-in TUI editor
///
/// `prompt` lines are prefixed with `#` and stripped from the result.
/// Returns `Some(content)` on save, `None` if cancelled or empty.
pub fn capture(prompt: &str) -> Result<Option<String>> {
    capture_with_initial(prompt, "")
}

/// Like [`capture`], but pre-fills the buffer with `initial` content.
pub fn capture_with_initial(prompt: &str, initial: &str) -> Result<Option<String>> {
    match resolve_editor() {
        Some(editor) => capture_external(&editor, prompt, initial),
        None => capture_builtin(prompt, initial),
    }
}

/// Spawn an external editor via a tempfile.
fn capture_external(editor: &str, prompt: &str, initial: &str) -> Result<Option<String>> {
    let mut tf = tempfile::Builder::new()
        .prefix("soe-")
        .suffix(".md")
        .tempfile()
        .context("creating editor tempfile")?;

    if !initial.is_empty() {
        write!(tf, "{initial}").context("writing initial content to tempfile")?;
        if !initial.ends_with('\n') {
            writeln!(tf).context("terminating initial content")?;
        }
        writeln!(tf).ok();
    }

    for line in prompt.lines() {
        writeln!(tf, "# {line}").context("writing prompt to tempfile")?;
    }
    writeln!(tf).ok();
    tf.flush().context("flushing prompt")?;

    let path = tf.path().to_path_buf();
    let status = spawn_editor(editor, &path)
        .with_context(|| format!("spawning editor `{editor}`"))?;
    if !status.success() {
        anyhow::bail!("editor `{editor}` exited with status {status}");
    }

    let mut contents = String::new();
    std::fs::File::open(&path)
        .context("re-opening editor tempfile")?
        .read_to_string(&mut contents)
        .context("reading editor tempfile")?;

    Ok(strip_comments(&contents))
}

/// Use the built-in TUI editor.
fn capture_builtin(prompt: &str, initial: &str) -> Result<Option<String>> {
    let content = if initial.is_empty() {
        format!("\n# {}", prompt.lines().collect::<Vec<_>>().join("\n# "))
    } else {
        format!(
            "{initial}\n\n# {}",
            prompt.lines().collect::<Vec<_>>().join("\n# ")
        )
    };

    let result = crate::edit("soe", &content, EditorMode::PlainText)?;

    Ok(result.and_then(|text| strip_comments(&text)))
}

/// Strip `#`-prefixed comment lines and trim.
fn strip_comments(text: &str) -> Option<String> {
    let cleaned: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Spawn the editor through the shell so compound values like
/// `"code --wait"` work correctly (matching Git's behavior).
fn spawn_editor(editor: &str, path: &Path) -> Result<std::process::ExitStatus> {
    let path_str = path.display().to_string();
    if cfg!(windows) {
        Command::new("cmd")
            .args(["/C", &format!("{editor} \"{path_str}\"")])
            .status()
            .map_err(Into::into)
    } else {
        Command::new("sh")
            .args(["-c", &format!("{editor} \"$@\""), "--", &path_str])
            .status()
            .map_err(Into::into)
    }
}

/// Resolve the editor using Git's precedence order:
///   1. `$GIT_EDITOR`
///   2. `git config core.editor`
///   3. `$VISUAL`
///   4. `$EDITOR`
///
/// Returns `None` when no external editor is configured (→ use built-in).
fn resolve_editor() -> Option<String> {
    fn non_empty_var(name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    }

    non_empty_var("GIT_EDITOR")
        .or_else(git_config_editor)
        .or_else(|| non_empty_var("VISUAL"))
        .or_else(|| non_empty_var("EDITOR"))
}

fn git_config_editor() -> Option<String> {
    let output = Command::new("git")
        .args(["config", "core.editor"])
        .output()
        .ok()?;
    if output.status.success() {
        let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !val.is_empty() {
            return Some(val);
        }
    }
    None
}
