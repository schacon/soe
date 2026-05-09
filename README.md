# soe — Scott's Own Editor

A built-in TUI text editor for CLI tools. Drop-in fallback when no external
editor (`$EDITOR`, `$VISUAL`, `core.editor`) is configured.

Built on [ratatui](https://ratatui.rs) and [crossterm](https://github.com/crossterm-rs/crossterm).

## Features

- Menu bar (File / Help), line numbers, word navigation
- Mouse support (click to position cursor, scroll)
- Two modes: `PlainText` and `CommitMessage` (72-char guide line + soft wrap)
- Tab expansion (4 spaces)
- RAII terminal guard — restores terminal state even on panic
- Save (`Ctrl+S`) returns content, cancel (`Esc`) returns `None`

## Usage

```rust
use soe::{edit, EditorMode};

// Open with initial content — returns Some(content) on save, None on cancel
let result = edit("commit message", "fix: resolve panic on empty input", EditorMode::CommitMessage)?;

// Edit a file on disk
soe::edit_file(std::path::Path::new("README.md"))?;
```

## License

MIT
