# soe — Scott's Own Editor

A built-in TUI text editor for CLI tools. Resolves the best available editor
using Git's precedence order and falls back to a built-in TUI when none is
configured.

Built on [ratatui](https://ratatui.rs) and [crossterm](https://github.com/crossterm-rs/crossterm).

## Features

- **One-call editor resolution**: `$GIT_EDITOR` → `core.editor` → `$VISUAL` → `$EDITOR` → built-in TUI
- Menu bar (File / Help), line numbers, word navigation
- Mouse support (click to position cursor, scroll)
- Two modes: `PlainText` and `CommitMessage` (72-char guide line + soft wrap)
- Tab expansion (4 spaces)
- RAII terminal guard — restores terminal state even on panic
- Save (`Ctrl+S`) returns content, cancel (`Esc`) returns `None`

## Usage

```rust
// One call — resolves external editor or falls back to built-in TUI.
// Comment lines (starting with #) are stripped from the result.
let result = soe::capture("Enter your message")?;

// With pre-filled content
let result = soe::capture_with_initial("Edit the description", "existing text")?;

// Direct access to the built-in TUI editor (skips external editor resolution)
let result = soe::edit("filename", "initial content", soe::EditorMode::PlainText)?;
```

## License

MIT
