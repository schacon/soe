//! Exercises `soe::edit`'s external-editor path by pointing `$GIT_EDITOR`
//! at throwaway shell scripts. One test function: the editor env var is
//! process-global, so the scenarios must not run in parallel.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

fn write_script(dir: &std::path::Path, name: &str, body: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path.display().to_string()
}

#[test]
fn external_editor_save_cancel_and_missing() {
    let dir = tempfile::tempdir().unwrap();

    // Editor appends a line and exits 0 → saved content comes back raw,
    // with markdown `#` headings intact (edit() must not strip comments).
    let append = write_script(dir.path(), "append.sh", r#"echo appended >> "$1""#);
    std::env::set_var("GIT_EDITOR", &append);
    let out = soe::edit("note.md", "# Heading\nbody\n", soe::EditorMode::PlainText).unwrap();
    assert_eq!(out.as_deref(), Some("# Heading\nbody\nappended\n"));

    // Editor exits non-zero (vim's `:cq`) → cancelled.
    let cancel = write_script(dir.path(), "cancel.sh", "exit 1");
    std::env::set_var("GIT_EDITOR", &cancel);
    let out = soe::edit("note.md", "body\n", soe::EditorMode::PlainText).unwrap();
    assert_eq!(out, None);

    // Editor binary doesn't exist → error, not a silent cancel.
    std::env::set_var("GIT_EDITOR", dir.path().join("no-such-editor").display().to_string());
    let err = soe::edit("note.md", "body\n", soe::EditorMode::PlainText).unwrap_err();
    assert!(err.to_string().contains("could not be run"), "got: {err:#}");

    std::env::remove_var("GIT_EDITOR");
}
