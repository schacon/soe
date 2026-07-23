//! A built-in TUI text editor inspired by Microsoft's `edit`.
//!
//! Originally from [GitButler](https://gitbutler.com), extracted as a
//! standalone crate.

use std::time::Duration;

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

// ── Hard wrap utility for commit messages ───────────────────────────────────

/// Hard wraps commit message text at 72 characters.
/// First line is preserved as-is (the summary line), subsequent lines are wrapped.
fn hard_wrap_commit_message(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return String::new();
    }

    let mut result = Vec::new();

    if let Some(first) = lines.first() {
        result.push(first.to_string());
    }

    for line in lines.iter().skip(1) {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            result.push(line.to_string());
            continue;
        }

        let words: Vec<&str> = line.split_whitespace().collect();
        let mut current_line = String::new();

        for word in words {
            if current_line.is_empty() {
                current_line = word.to_string();
            } else if current_line.len() + 1 + word.len() <= 72 {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                result.push(current_line);
                current_line = word.to_string();
            }
        }

        if !current_line.is_empty() {
            result.push(current_line);
        }
    }

    result.join("\n")
}

// ── Colour palette ──────────────────────────────────────────────────────────

const MENU_BAR_BG: Color = Color::Rgb(90, 90, 140);
const MENU_BAR_FG: Color = Color::Rgb(220, 220, 220);
const MENU_ACTIVE_BG: Color = Color::Rgb(180, 200, 60);
const MENU_ACTIVE_FG: Color = Color::Rgb(30, 30, 30);
const EDITOR_BG: Color = Color::Rgb(40, 42, 54);
const EDITOR_FG: Color = Color::Rgb(200, 200, 200);
const LINE_NUM_FG: Color = Color::Rgb(100, 100, 140);
const LINE_NUM_BG: Color = Color::Rgb(30, 32, 44);
const STATUS_BAR_BG: Color = Color::Rgb(80, 80, 140);
const STATUS_BAR_FG: Color = Color::Rgb(220, 220, 220);
const DROPDOWN_BG: Color = Color::Rgb(70, 72, 90);
const DROPDOWN_FG: Color = Color::Rgb(210, 210, 210);
const DROPDOWN_HIGHLIGHT_BG: Color = Color::Rgb(100, 110, 160);
const CURSOR_BG: Color = Color::Rgb(200, 200, 200);
const CURSOR_FG: Color = Color::Rgb(30, 30, 30);

// ── Editor mode ─────────────────────────────────────────────────────────────

/// Controls editor behavior for different editing contexts.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum EditorMode {
    /// Hard wraps at 72 characters, shows guide line.
    CommitMessage,
    /// Plain text, no wrapping.
    PlainText,
}

// ── Menu definitions ────────────────────────────────────────────────────────

struct MenuItem {
    label: &'static str,
    shortcut: &'static str,
    action: MenuAction,
}

#[derive(Clone, Copy, PartialEq)]
enum MenuAction {
    Save,
    SaveAndQuit,
    Cancel,
    Undo,
    Redo,
    GotoEnd,
    NextLine,
    PrevLine,
    NextParagraph,
    PrevParagraph,
    DeleteParagraph,
    EmptyAll,
    ToggleWrap,
    ShowHelp,
    ShowAbout,
}

const MENU_TITLES: &[&str] = &["File", "Edit", "Help"];

fn menu_items(menu_index: usize) -> &'static [MenuItem] {
    match menu_index {
        0 => &FILE_MENU,
        1 => &EDIT_MENU,
        2 => &HELP_MENU,
        _ => &[],
    }
}

static FILE_MENU: [MenuItem; 3] = [
    MenuItem {
        label: "Save",
        shortcut: "Ctrl+S",
        action: MenuAction::Save,
    },
    MenuItem {
        label: "Save & Quit",
        shortcut: "Ctrl+Q",
        action: MenuAction::SaveAndQuit,
    },
    MenuItem {
        label: "Cancel",
        shortcut: "Esc",
        action: MenuAction::Cancel,
    },
];

static EDIT_MENU: [MenuItem; 10] = [
    MenuItem {
        label: "Undo",
        shortcut: "Ctrl+Z",
        action: MenuAction::Undo,
    },
    MenuItem {
        label: "Redo",
        shortcut: "Ctrl+Y",
        action: MenuAction::Redo,
    },
    MenuItem {
        label: "Go to End of Line",
        shortcut: "Ctrl+A",
        action: MenuAction::GotoEnd,
    },
    MenuItem {
        label: "Next Line",
        shortcut: "Ctrl+J",
        action: MenuAction::NextLine,
    },
    MenuItem {
        label: "Prev Line",
        shortcut: "Ctrl+K",
        action: MenuAction::PrevLine,
    },
    MenuItem {
        label: "Next Paragraph",
        shortcut: "Ctrl+L",
        action: MenuAction::NextParagraph,
    },
    MenuItem {
        label: "Prev Paragraph",
        shortcut: "Ctrl+P",
        action: MenuAction::PrevParagraph,
    },
    MenuItem {
        label: "Delete Paragraph",
        shortcut: "Ctrl+D",
        action: MenuAction::DeleteParagraph,
    },
    MenuItem {
        label: "Empty (Delete All)",
        shortcut: "Ctrl+E",
        action: MenuAction::EmptyAll,
    },
    MenuItem {
        label: "Toggle Word Wrap",
        shortcut: "Alt+Z",
        action: MenuAction::ToggleWrap,
    },
];

static HELP_MENU: [MenuItem; 2] = [
    MenuItem {
        label: "Keyboard Shortcuts",
        shortcut: "",
        action: MenuAction::ShowHelp,
    },
    MenuItem {
        label: "About",
        shortcut: "",
        action: MenuAction::ShowAbout,
    },
];

// ── Overlays ────────────────────────────────────────────────────────────────

#[derive(Default)]
struct HelpOverlay {
    active: bool,
}

#[derive(Default)]
struct AboutOverlay {
    active: bool,
}

#[derive(Default)]
struct TeddyOverlay {
    active: bool,
}

// ── Undo / Redo ────────────────────────────────────────────────────────────

/// A snapshot of the editor buffer at a point in time.
#[derive(Clone)]
struct Snapshot {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

const MAX_UNDO: usize = 200;

// ── Soft wrap ──────────────────────────────────────────────────────────────

/// One screen row when soft-wrap is active. Maps back to a buffer line and
/// a byte offset into that line.
#[derive(Clone)]
struct VisualRow {
    line_idx: usize,
    byte_start: usize,
    is_continuation: bool,
}

/// Colour for the wrap continuation indicator in the gutter.
const WRAP_INDICATOR_FG: Color = Color::Rgb(120, 120, 160);

fn build_visual_rows(lines: &[String], text_width: usize) -> Vec<VisualRow> {
    let tw = text_width.max(1);
    let mut rows = Vec::new();
    for (line_idx, line) in lines.iter().enumerate() {
        if line.is_empty() {
            rows.push(VisualRow { line_idx, byte_start: 0, is_continuation: false });
            continue;
        }
        let mut byte_start = 0;
        let mut col_count = 0;
        let mut is_first = true;
        for (byte_pos, _ch) in line.char_indices() {
            if col_count == tw {
                rows.push(VisualRow { line_idx, byte_start, is_continuation: !is_first });
                byte_start = byte_pos;
                col_count = 0;
                is_first = false;
            }
            col_count += 1;
        }
        rows.push(VisualRow { line_idx, byte_start, is_continuation: !is_first });
    }
    rows
}

fn cursor_to_visual_row(visual_rows: &[VisualRow], cursor_row: usize, cursor_col: usize) -> usize {
    let mut result = 0;
    for (i, vr) in visual_rows.iter().enumerate() {
        if vr.line_idx == cursor_row && vr.byte_start <= cursor_col {
            result = i;
        }
        if vr.line_idx > cursor_row {
            break;
        }
    }
    result
}

// ── Editor state ────────────────────────────────────────────────────────────

struct EditorApp {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    scroll_row: usize,
    scroll_col: usize,
    modified: bool,
    should_quit: bool,
    save_on_quit: bool,
    mode: EditorMode,
    active_menu: Option<usize>,
    menu_item_index: usize,
    help_overlay: HelpOverlay,
    about_overlay: AboutOverlay,
    teddy_overlay: TeddyOverlay,
    secret_buf: String,
    editor_area: Rect,
    gutter_width: u16,
    highlight_save_hint: bool,
    hint_highlight_frames: u8,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    word_wrap: bool,
    scroll_vrow: usize,
    visual_rows: Vec<VisualRow>,
}

impl EditorApp {
    fn new(_filename: &str, content: &str, mode: EditorMode) -> Self {
        let lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(String::from).collect()
        };
        let lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };
        Self {
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            modified: false,
            should_quit: false,
            save_on_quit: false,
            mode,
            active_menu: None,
            menu_item_index: 0,
            help_overlay: HelpOverlay::default(),
            about_overlay: AboutOverlay::default(),
            teddy_overlay: TeddyOverlay::default(),
            secret_buf: String::new(),
            editor_area: Rect::default(),
            gutter_width: 4,
            highlight_save_hint: false,
            hint_highlight_frames: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            word_wrap: false,
            scroll_vrow: 0,
            visual_rows: Vec::new(),
        }
    }

    fn content(&self) -> String {
        let text = self.lines.join("\n");
        if self.mode == EditorMode::CommitMessage {
            hard_wrap_commit_message(&text)
        } else {
            text
        }
    }

    fn clamp_cursor(&mut self) {
        if self.cursor_row >= self.lines.len() {
            self.cursor_row = self.lines.len() - 1;
        }
        let line_len = self.lines[self.cursor_row].len();
        if self.cursor_col > line_len {
            self.cursor_col = line_len;
        }
    }

    fn ensure_cursor_visible(&mut self) {
        let visible_rows = self.editor_area.height as usize;

        if self.word_wrap {
            if self.visual_rows.is_empty() {
                return;
            }
            let cursor_vr = cursor_to_visual_row(&self.visual_rows, self.cursor_row, self.cursor_col);
            if cursor_vr < self.scroll_vrow {
                self.scroll_vrow = cursor_vr;
            } else if visible_rows > 0 && cursor_vr >= self.scroll_vrow + visible_rows {
                self.scroll_vrow = cursor_vr - visible_rows + 1;
            }
        } else {
            let visible_cols = (self.editor_area.width.saturating_sub(self.gutter_width)) as usize;
            if self.cursor_row < self.scroll_row {
                self.scroll_row = self.cursor_row;
            } else if visible_rows > 0 && self.cursor_row >= self.scroll_row + visible_rows {
                self.scroll_row = self.cursor_row - visible_rows + 1;
            }
            if self.cursor_col < self.scroll_col {
                self.scroll_col = self.cursor_col;
            } else if self.cursor_col >= self.scroll_col + visible_cols {
                self.scroll_col = self.cursor_col - visible_cols + 1;
            }
        }
    }

    /// Save current state to the undo stack before a mutation.
    fn save_undo(&mut self) {
        self.undo_stack.push(Snapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        });
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn undo(&mut self) {
        if let Some(snapshot) = self.undo_stack.pop() {
            self.redo_stack.push(Snapshot {
                lines: self.lines.clone(),
                cursor_row: self.cursor_row,
                cursor_col: self.cursor_col,
            });
            self.lines = snapshot.lines;
            self.cursor_row = snapshot.cursor_row;
            self.cursor_col = snapshot.cursor_col;
            self.modified = true;
        }
    }

    fn redo(&mut self) {
        if let Some(snapshot) = self.redo_stack.pop() {
            self.undo_stack.push(Snapshot {
                lines: self.lines.clone(),
                cursor_row: self.cursor_row,
                cursor_col: self.cursor_col,
            });
            self.lines = snapshot.lines;
            self.cursor_row = snapshot.cursor_row;
            self.cursor_col = snapshot.cursor_col;
            self.modified = true;
        }
    }

    fn insert_char(&mut self, ch: char) {
        self.save_undo();
        self.lines[self.cursor_row].insert(self.cursor_col, ch);
        self.cursor_col += ch.len_utf8();
        self.modified = true;
    }

    fn insert_newline(&mut self) {
        self.save_undo();
        let rest = self.lines[self.cursor_row][self.cursor_col..].to_string();
        self.lines[self.cursor_row].truncate(self.cursor_col);
        self.cursor_row += 1;
        self.lines.insert(self.cursor_row, rest);
        self.cursor_col = 0;
        self.modified = true;
    }

    fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.save_undo();
            let prev = self.lines[self.cursor_row][..self.cursor_col]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.lines[self.cursor_row].remove(prev);
            self.cursor_col = prev;
            self.modified = true;
        } else if self.cursor_row > 0 {
            self.save_undo();
            let line = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
            self.lines[self.cursor_row].push_str(&line);
            self.modified = true;
        }
    }

    fn delete(&mut self) {
        let line_len = self.lines[self.cursor_row].len();
        if self.cursor_col < line_len {
            self.save_undo();
            self.lines[self.cursor_row].remove(self.cursor_col);
            self.modified = true;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.save_undo();
            let next_line = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next_line);
            self.modified = true;
        }
    }

    fn execute_menu_action(&mut self, action: MenuAction) {
        self.active_menu = None;
        match action {
            MenuAction::Save => {
                self.save_on_quit = true;
            }
            MenuAction::SaveAndQuit => {
                self.save_on_quit = true;
                self.should_quit = true;
            }
            MenuAction::Cancel => {
                self.save_on_quit = false;
                self.should_quit = true;
            }
            MenuAction::Undo => {
                self.undo();
            }
            MenuAction::Redo => {
                self.redo();
            }
            MenuAction::GotoEnd => {
                self.cursor_col = self.lines[self.cursor_row].len();
            }
            MenuAction::NextLine => {
                if self.cursor_row + 1 < self.lines.len() {
                    self.cursor_row += 1;
                }
                self.clamp_cursor();
            }
            MenuAction::PrevLine => {
                if self.cursor_row > 0 {
                    self.cursor_row -= 1;
                }
                self.clamp_cursor();
            }
            MenuAction::NextParagraph => {
                self.move_next_paragraph();
            }
            MenuAction::PrevParagraph => {
                self.move_prev_paragraph();
            }
            MenuAction::DeleteParagraph => {
                self.delete_paragraph();
            }
            MenuAction::EmptyAll => {
                self.save_undo();
                self.lines = vec![String::new()];
                self.cursor_row = 0;
                self.cursor_col = 0;
                self.modified = true;
            }
            MenuAction::ToggleWrap => {
                self.word_wrap = !self.word_wrap;
                self.scroll_vrow = 0;
            }
            MenuAction::ShowHelp => {
                self.help_overlay.active = !self.help_overlay.active;
            }
            MenuAction::ShowAbout => {
                self.about_overlay.active = !self.about_overlay.active;
            }
        }
    }

    fn handle_event(&mut self, ev: Event) {
        if self.hint_highlight_frames > 0 {
            self.hint_highlight_frames = self.hint_highlight_frames.saturating_sub(1);
            if self.hint_highlight_frames == 0 {
                self.highlight_save_hint = false;
            }
        }

        if self.help_overlay.active {
            if let Event::Key(key) = ev {
                if key.kind == KeyEventKind::Press {
                    self.help_overlay.active = false;
                }
            }
            return;
        }

        if self.about_overlay.active {
            if let Event::Key(key) = ev {
                if key.kind == KeyEventKind::Press {
                    self.about_overlay.active = false;
                }
            }
            return;
        }

        if self.teddy_overlay.active {
            if let Event::Key(key) = ev {
                if key.kind == KeyEventKind::Press {
                    self.teddy_overlay.active = false;
                }
            }
            return;
        }

        if self.active_menu.is_some() {
            self.handle_menu_event(ev);
            return;
        }

        match ev {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                let alt = key.modifiers.contains(KeyModifiers::ALT);

                match key.code {
                    KeyCode::Char('z') if alt => {
                        self.execute_menu_action(MenuAction::ToggleWrap);
                    }
                    KeyCode::Char('s') if ctrl => {
                        self.execute_menu_action(MenuAction::Save);
                    }
                    KeyCode::Char('q') if ctrl => {
                        self.execute_menu_action(MenuAction::SaveAndQuit);
                    }
                    KeyCode::Char('a') if ctrl => {
                        self.execute_menu_action(MenuAction::GotoEnd);
                    }
                    KeyCode::Char('j') if ctrl => {
                        self.execute_menu_action(MenuAction::NextLine);
                    }
                    KeyCode::Char('k') if ctrl => {
                        self.execute_menu_action(MenuAction::PrevLine);
                    }
                    KeyCode::Char('l') if ctrl => {
                        self.execute_menu_action(MenuAction::NextParagraph);
                    }
                    KeyCode::Char('p') if ctrl => {
                        self.execute_menu_action(MenuAction::PrevParagraph);
                    }
                    KeyCode::Char('d') if ctrl => {
                        self.execute_menu_action(MenuAction::DeleteParagraph);
                    }
                    KeyCode::Char('e') if ctrl => {
                        self.execute_menu_action(MenuAction::EmptyAll);
                    }
                    KeyCode::Char('z') if ctrl => {
                        self.undo();
                    }
                    KeyCode::Char('y') if ctrl => {
                        self.redo();
                    }
                    KeyCode::Up => {
                        if self.cursor_row > 0 {
                            self.cursor_row -= 1;
                        }
                        self.clamp_cursor();
                    }
                    KeyCode::Down => {
                        if self.cursor_row + 1 < self.lines.len() {
                            self.cursor_row += 1;
                        }
                        self.clamp_cursor();
                    }
                    KeyCode::Left => {
                        if ctrl {
                            self.move_word_left();
                        } else if self.cursor_col > 0 {
                            self.cursor_col = self.lines[self.cursor_row][..self.cursor_col]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                        } else if self.cursor_row > 0 {
                            self.cursor_row -= 1;
                            self.cursor_col = self.lines[self.cursor_row].len();
                        }
                    }
                    KeyCode::Right => {
                        if ctrl {
                            self.move_word_right();
                        } else {
                            let line_len = self.lines[self.cursor_row].len();
                            if self.cursor_col < line_len {
                                let ch = self.lines[self.cursor_row][self.cursor_col..]
                                    .chars()
                                    .next()
                                    .unwrap();
                                self.cursor_col += ch.len_utf8();
                            } else if self.cursor_row + 1 < self.lines.len() {
                                self.cursor_row += 1;
                                self.cursor_col = 0;
                            }
                        }
                    }
                    KeyCode::Home => {
                        if ctrl {
                            self.cursor_row = 0;
                        }
                        self.cursor_col = 0;
                    }
                    KeyCode::End => {
                        if ctrl {
                            self.cursor_row = self.lines.len() - 1;
                        }
                        self.cursor_col = self.lines[self.cursor_row].len();
                    }
                    KeyCode::PageUp => {
                        let page = self.editor_area.height as usize;
                        self.cursor_row = self.cursor_row.saturating_sub(page);
                        self.clamp_cursor();
                    }
                    KeyCode::PageDown => {
                        let page = self.editor_area.height as usize;
                        self.cursor_row = (self.cursor_row + page).min(self.lines.len() - 1);
                        self.clamp_cursor();
                    }
                    KeyCode::Char(ch) => {
                        // Track secret :teddy sequence
                        if ch == ':' && self.secret_buf.is_empty() {
                            self.secret_buf.push(':');
                        } else if !self.secret_buf.is_empty() {
                            self.secret_buf.push(ch);
                            if !":teddy".starts_with(&self.secret_buf) {
                                self.secret_buf.clear();
                            }
                        }
                        self.insert_char(ch);
                    }
                    KeyCode::Enter => {
                        if self.secret_buf == ":teddy" {
                            self.teddy_overlay.active = true;
                        }
                        self.insert_newline();
                        self.secret_buf.clear();
                    }
                    KeyCode::Backspace => {
                        self.secret_buf.clear();
                        self.backspace();
                    }
                    KeyCode::Delete => self.delete(),
                    KeyCode::Tab => {
                        self.save_undo();
                        self.lines[self.cursor_row].insert_str(self.cursor_col, "    ");
                        self.cursor_col += 4;
                        self.modified = true;
                    }
                    KeyCode::Esc => {
                        self.highlight_save_hint = true;
                        self.hint_highlight_frames = 6;
                    }
                    KeyCode::F(10) => {
                        self.active_menu = Some(0);
                        self.menu_item_index = 0;
                    }
                    _ => {}
                }
            }
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            _ => {}
        }
    }

    fn move_word_left(&mut self) {
        let line = &self.lines[self.cursor_row];
        if self.cursor_col == 0 {
            if self.cursor_row > 0 {
                self.cursor_row -= 1;
                self.cursor_col = self.lines[self.cursor_row].len();
            }
            return;
        }
        let bytes = line.as_bytes();
        let mut col = self.cursor_col;
        while col > 0 && bytes[col - 1].is_ascii_whitespace() {
            col -= 1;
        }
        while col > 0 && !bytes[col - 1].is_ascii_whitespace() {
            col -= 1;
        }
        self.cursor_col = col;
    }

    fn move_word_right(&mut self) {
        let line = &self.lines[self.cursor_row];
        let len = line.len();
        if self.cursor_col >= len {
            if self.cursor_row + 1 < self.lines.len() {
                self.cursor_row += 1;
                self.cursor_col = 0;
            }
            return;
        }
        let bytes = line.as_bytes();
        let mut col = self.cursor_col;
        while col < len && !bytes[col].is_ascii_whitespace() {
            col += 1;
        }
        while col < len && bytes[col].is_ascii_whitespace() {
            col += 1;
        }
        self.cursor_col = col;
    }

    /// Move cursor to the start of the next paragraph. A paragraph
    /// boundary is an empty line (or the end of the document).
    fn move_next_paragraph(&mut self) {
        let total = self.lines.len();
        let mut row = self.cursor_row;

        // Skip non-empty lines in the current paragraph.
        while row < total && !self.lines[row].trim().is_empty() {
            row += 1;
        }
        // Skip any blank lines between paragraphs.
        while row < total && self.lines[row].trim().is_empty() {
            row += 1;
        }
        self.cursor_row = row.min(total - 1);
        self.cursor_col = 0;
        self.clamp_cursor();
    }

    /// Move cursor to the start of the previous paragraph.
    fn move_prev_paragraph(&mut self) {
        let mut row = self.cursor_row;

        // If we're on a blank line, skip blank lines upward first.
        while row > 0 && self.lines[row].trim().is_empty() {
            row -= 1;
        }
        // Skip non-empty lines upward through the current paragraph.
        while row > 0 && !self.lines[row - 1].trim().is_empty() {
            row -= 1;
        }
        self.cursor_row = row;
        self.cursor_col = 0;
        self.clamp_cursor();
    }

    /// Delete all lines in the current paragraph (contiguous non-empty
    /// lines around the cursor). Leaves the cursor on the next content.
    fn delete_paragraph(&mut self) {
        self.save_undo();
        // Find the start of this paragraph.
        let mut start = self.cursor_row;
        while start > 0 && !self.lines[start - 1].trim().is_empty() {
            start -= 1;
        }
        // Find the end (exclusive).
        let mut end = self.cursor_row;
        while end < self.lines.len() && !self.lines[end].trim().is_empty() {
            end += 1;
        }
        // Also consume trailing blank lines.
        while end < self.lines.len() && self.lines[end].trim().is_empty() {
            end += 1;
        }

        if start == end {
            // Cursor was on a blank line — just delete it.
            if self.lines.len() > 1 {
                self.lines.remove(start);
            } else {
                self.lines[0].clear();
            }
        } else {
            self.lines.drain(start..end);
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }
        }

        self.cursor_row = start.min(self.lines.len() - 1);
        self.cursor_col = 0;
        self.modified = true;
        self.clamp_cursor();
    }

    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if mouse.row == 0 {
                    self.handle_menu_bar_click(mouse.column);
                    return;
                }
                if mouse.row > 0
                    && mouse.row < self.editor_area.y + self.editor_area.height
                    && mouse.column >= self.editor_area.x + self.gutter_width
                {
                    let click_col = (mouse.column - self.editor_area.x - self.gutter_width) as usize;
                    if self.word_wrap {
                        let vr_idx = (mouse.row as usize - 1) + self.scroll_vrow;
                        if vr_idx < self.visual_rows.len() {
                            let vr = &self.visual_rows[vr_idx];
                            self.cursor_row = vr.line_idx;
                            let line = &self.lines[vr.line_idx];
                            let mut byte_col = vr.byte_start;
                            let mut chars_counted = 0;
                            for ch in line[vr.byte_start..].chars() {
                                if chars_counted >= click_col { break; }
                                byte_col += ch.len_utf8();
                                chars_counted += 1;
                            }
                            self.cursor_col = byte_col.min(line.len());
                        }
                    } else {
                        let row = (mouse.row as usize - 1) + self.scroll_row;
                        let col = click_col + self.scroll_col;
                        self.cursor_row = row.min(self.lines.len() - 1);
                        self.cursor_col = col.min(self.lines[self.cursor_row].len());
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if self.word_wrap {
                    self.scroll_vrow = self.scroll_vrow.saturating_sub(3);
                } else {
                    self.scroll_row = self.scroll_row.saturating_sub(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if self.word_wrap {
                    let max = self.visual_rows.len().saturating_sub(1);
                    self.scroll_vrow = (self.scroll_vrow + 3).min(max);
                } else {
                    let max = self.lines.len().saturating_sub(1);
                    self.scroll_row = (self.scroll_row + 3).min(max);
                }
            }
            _ => {}
        }
    }

    fn handle_menu_bar_click(&mut self, col: u16) {
        let mut x = 1u16;
        for (i, title) in MENU_TITLES.iter().enumerate() {
            let w = title.len() as u16 + 2;
            if col >= x && col < x + w {
                if self.active_menu == Some(i) {
                    self.active_menu = None;
                } else {
                    self.active_menu = Some(i);
                    self.menu_item_index = 0;
                }
                return;
            }
            x += w;
        }
        self.active_menu = None;
    }

    fn handle_menu_event(&mut self, ev: Event) {
        match ev {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => self.active_menu = None,
                KeyCode::Up if self.menu_item_index > 0 => {
                    self.menu_item_index -= 1;
                }
                KeyCode::Down => {
                    if let Some(mi) = self.active_menu {
                        let items = menu_items(mi);
                        if self.menu_item_index + 1 < items.len() {
                            self.menu_item_index += 1;
                        }
                    }
                }
                KeyCode::Left => {
                    if let Some(mi) = self.active_menu {
                        self.active_menu = Some(if mi == 0 {
                            MENU_TITLES.len() - 1
                        } else {
                            mi - 1
                        });
                        self.menu_item_index = 0;
                    }
                }
                KeyCode::Right => {
                    if let Some(mi) = self.active_menu {
                        self.active_menu = Some((mi + 1) % MENU_TITLES.len());
                        self.menu_item_index = 0;
                    }
                }
                KeyCode::Enter => {
                    if let Some(mi) = self.active_menu {
                        let items = menu_items(mi);
                        if self.menu_item_index < items.len() {
                            let action = items[self.menu_item_index].action;
                            self.execute_menu_action(action);
                        }
                    }
                }
                _ => {}
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                if mouse.row == 0 {
                    self.handle_menu_bar_click(mouse.column);
                    return;
                }
                if let Some(mi) = self.active_menu {
                    let dropdown_rect = self.dropdown_rect(mi);
                    if mouse.row >= dropdown_rect.y
                        && mouse.row < dropdown_rect.y + dropdown_rect.height
                        && mouse.column >= dropdown_rect.x
                        && mouse.column < dropdown_rect.x + dropdown_rect.width
                    {
                        let item_idx = (mouse.row - dropdown_rect.y - 1) as usize;
                        let items = menu_items(mi);
                        if item_idx < items.len() {
                            let action = items[item_idx].action;
                            self.execute_menu_action(action);
                        }
                        return;
                    }
                }
                self.active_menu = None;
                self.handle_mouse(mouse);
            }
            _ => {}
        }
    }

    fn dropdown_rect(&self, menu_index: usize) -> Rect {
        let items = menu_items(menu_index);
        let width = items
            .iter()
            .map(|it| it.label.len() + it.shortcut.len() + 6)
            .max()
            .unwrap_or(20) as u16
            + 2;
        let height = items.len() as u16 + 2;
        let mut x = 1u16;
        for title in MENU_TITLES.iter().take(menu_index) {
            x += title.len() as u16 + 2;
        }
        Rect::new(x, 1, width, height)
    }
}

// ── Rendering ───────────────────────────────────────────────────────────────

fn render(frame: &mut ratatui::Frame, app: &mut EditorApp) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_menu_bar(frame, app, chunks[0]);
    render_editor(frame, app, chunks[1]);
    render_status_bar(frame, app, chunks[2]);

    if let Some(mi) = app.active_menu {
        render_dropdown(frame, app, mi);
    }

    if app.help_overlay.active {
        render_help_overlay(frame);
    }

    if app.about_overlay.active {
        render_about_overlay(frame);
    }

    if app.teddy_overlay.active {
        render_teddy_overlay(frame);
    }
}

fn render_menu_bar(frame: &mut ratatui::Frame, app: &EditorApp, area: Rect) {
    let bar_bg = Paragraph::new("").style(Style::default().bg(MENU_BAR_BG));
    frame.render_widget(bar_bg, area);

    let mut spans = Vec::new();
    spans.push(Span::styled(" ", Style::default().bg(MENU_BAR_BG)));
    for (i, title) in MENU_TITLES.iter().enumerate() {
        let style = if app.active_menu == Some(i) {
            Style::default().fg(MENU_ACTIVE_FG).bg(MENU_ACTIVE_BG)
        } else {
            Style::default().fg(MENU_BAR_FG).bg(MENU_BAR_BG)
        };
        spans.push(Span::styled(title.to_string(), style));
        spans.push(Span::styled("  ", Style::default().bg(MENU_BAR_BG)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_editor(frame: &mut ratatui::Frame, app: &mut EditorApp, area: Rect) {
    app.editor_area = area;

    let line_count = app.lines.len();
    let digits = format!("{line_count}").len().max(2);
    app.gutter_width = digits as u16 + 2;

    let visible_rows = area.height as usize;
    let text_width = area.width.saturating_sub(app.gutter_width) as usize;

    if app.word_wrap {
        app.visual_rows = build_visual_rows(&app.lines, text_width);
        app.ensure_cursor_visible();
        render_editor_wrapped(frame, app, area, visible_rows, text_width, digits);
        return;
    }

    app.ensure_cursor_visible();

    let guide_col = if app.mode == EditorMode::CommitMessage {
        Some(72)
    } else {
        None
    };

    for row_offset in 0..visible_rows {
        let line_idx = app.scroll_row + row_offset;
        let y = area.y + row_offset as u16;

        let gutter_area = Rect::new(area.x, y, app.gutter_width, 1);
        if line_idx < app.lines.len() {
            let num_str = format!("{:>width$} ", line_idx + 1, width = digits);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    num_str,
                    Style::default().fg(LINE_NUM_FG).bg(LINE_NUM_BG),
                )),
                gutter_area,
            );
        } else {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    " ".repeat(app.gutter_width as usize),
                    Style::default().bg(LINE_NUM_BG),
                )),
                gutter_area,
            );
        }

        let text_area = Rect::new(area.x + app.gutter_width, y, text_width as u16, 1);

        if line_idx < app.lines.len() {
            let line = &app.lines[line_idx];
            let display_start = app.scroll_col;

            let mut spans = Vec::new();
            let mut byte_col = 0usize;
            let mut vis_col = 0usize;
            let mut rendered = 0usize;

            for ch in line.chars() {
                let tab_w = if ch == '\t' { 4 - (vis_col % 4) } else { 1 };

                if byte_col < display_start {
                    byte_col += ch.len_utf8();
                    vis_col += tab_w;
                    continue;
                }

                if rendered >= text_width {
                    break;
                }

                let is_cursor = line_idx == app.cursor_row && byte_col == app.cursor_col;

                if ch == '\t' {
                    let spaces = tab_w.min(text_width - rendered);
                    for i in 0..spaces {
                        let is_guide = guide_col.is_some_and(|g| vis_col + i == g);
                        let style = if is_cursor && i == 0 {
                            Style::default().fg(CURSOR_FG).bg(CURSOR_BG)
                        } else if is_guide {
                            Style::default().fg(EDITOR_FG).bg(Color::Rgb(60, 62, 74))
                        } else {
                            Style::default().fg(EDITOR_FG).bg(EDITOR_BG)
                        };
                        spans.push(Span::styled(" ", style));
                    }
                    rendered += spaces;
                } else {
                    let is_guide = guide_col == Some(vis_col);
                    let style = if is_cursor {
                        Style::default().fg(CURSOR_FG).bg(CURSOR_BG)
                    } else if is_guide {
                        Style::default().fg(EDITOR_FG).bg(Color::Rgb(60, 62, 74))
                    } else {
                        Style::default().fg(EDITOR_FG).bg(EDITOR_BG)
                    };
                    spans.push(Span::styled(ch.to_string(), style));
                    rendered += 1;
                }

                byte_col += ch.len_utf8();
                vis_col += tab_w;
            }

            if line_idx == app.cursor_row
                && app.cursor_col >= byte_col
                && app.cursor_col <= line.len()
                && rendered < text_width
            {
                spans.push(Span::styled(
                    " ",
                    Style::default().fg(CURSOR_FG).bg(CURSOR_BG),
                ));
                rendered += 1;
                vis_col += 1;
            }

            while rendered < text_width {
                let is_guide = guide_col == Some(vis_col);
                let bg = if is_guide {
                    Color::Rgb(60, 62, 74)
                } else {
                    EDITOR_BG
                };
                spans.push(Span::styled(" ", Style::default().bg(bg)));
                rendered += 1;
                vis_col += 1;
            }

            frame.render_widget(Paragraph::new(Line::from(spans)), text_area);
        } else {
            let mut spans = Vec::new();
            for col in 0..text_width {
                let is_guide = guide_col == Some(col);
                let bg = if is_guide {
                    Color::Rgb(60, 62, 74)
                } else {
                    EDITOR_BG
                };
                spans.push(Span::styled(" ", Style::default().bg(bg)));
            }
            frame.render_widget(Paragraph::new(Line::from(spans)), text_area);
        }
    }
}

fn render_editor_wrapped(
    frame: &mut ratatui::Frame,
    app: &EditorApp,
    area: Rect,
    visible_rows: usize,
    text_width: usize,
    digits: usize,
) {
    let guide_col = if app.mode == EditorMode::CommitMessage {
        Some(72)
    } else {
        None
    };

    for row_offset in 0..visible_rows {
        let vr_idx = app.scroll_vrow + row_offset;
        let y = area.y + row_offset as u16;

        let gutter_area = Rect::new(area.x, y, app.gutter_width, 1);
        let text_area = Rect::new(area.x + app.gutter_width, y, text_width as u16, 1);

        if vr_idx >= app.visual_rows.len() {
            // Empty row past end of document.
            frame.render_widget(
                Paragraph::new(Span::styled(
                    " ".repeat(app.gutter_width as usize),
                    Style::default().bg(LINE_NUM_BG),
                )),
                gutter_area,
            );
            let bg_spans: Vec<Span> = (0..text_width)
                .map(|col| {
                    let bg = if guide_col == Some(col) { Color::Rgb(60, 62, 74) } else { EDITOR_BG };
                    Span::styled(" ", Style::default().bg(bg))
                })
                .collect();
            frame.render_widget(Paragraph::new(Line::from(bg_spans)), text_area);
            continue;
        }

        let vr = &app.visual_rows[vr_idx];
        let line = &app.lines[vr.line_idx];

        // ── Gutter ──
        if vr.is_continuation {
            // Show wrap indicator instead of line number.
            let pad = app.gutter_width as usize - 2;
            let indicator = format!("{:>width$}\u{21AA} ", "", width = pad);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    indicator,
                    Style::default().fg(WRAP_INDICATOR_FG).bg(LINE_NUM_BG),
                )),
                gutter_area,
            );
        } else {
            let num_str = format!("{:>width$} ", vr.line_idx + 1, width = digits);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    num_str,
                    Style::default().fg(LINE_NUM_FG).bg(LINE_NUM_BG),
                )),
                gutter_area,
            );
        }

        // ── Text ──
        // Check if this visual row's line continues on the next visual row.
        let has_next_vr = vr_idx + 1 < app.visual_rows.len()
            && app.visual_rows[vr_idx + 1].line_idx == vr.line_idx;

        let mut spans = Vec::new();
        let mut byte_col = vr.byte_start;
        let mut rendered = 0usize;

        for ch in line[vr.byte_start..].chars() {
            if rendered >= text_width {
                break;
            }

            let is_cursor = vr.line_idx == app.cursor_row && byte_col == app.cursor_col;
            let is_guide = guide_col == Some(rendered);

            let style = if is_cursor {
                Style::default().fg(CURSOR_FG).bg(CURSOR_BG)
            } else if is_guide {
                Style::default().fg(EDITOR_FG).bg(Color::Rgb(60, 62, 74))
            } else {
                Style::default().fg(EDITOR_FG).bg(EDITOR_BG)
            };
            spans.push(Span::styled(ch.to_string(), style));
            rendered += 1;
            byte_col += ch.len_utf8();
        }

        // Cursor at end of line (only on the last visual row of the line).
        if !has_next_vr
            && vr.line_idx == app.cursor_row
            && app.cursor_col >= byte_col
            && app.cursor_col <= line.len()
            && rendered < text_width
        {
            spans.push(Span::styled(
                " ",
                Style::default().fg(CURSOR_FG).bg(CURSOR_BG),
            ));
            rendered += 1;
        }

        // Fill remaining columns.
        while rendered < text_width {
            let is_guide = guide_col == Some(rendered);
            let bg = if is_guide { Color::Rgb(60, 62, 74) } else { EDITOR_BG };
            spans.push(Span::styled(" ", Style::default().bg(bg)));
            rendered += 1;
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), text_area);
    }
}

fn render_status_bar(frame: &mut ratatui::Frame, app: &EditorApp, area: Rect) {
    let modified_indicator = if app.modified { "*" } else { "" };

    let mode_name = match app.mode {
        EditorMode::CommitMessage => "Commit Message",
        EditorMode::PlainText => "Plain",
    };

    let wrap_indicator = if app.word_wrap { " Wrap" } else { "" };

    let left = format!(
        " [{}{}] {}:{} {}",
        mode_name,
        wrap_indicator,
        app.cursor_row + 1,
        app.cursor_col + 1,
        modified_indicator,
    );

    let ctrl_q_style = if app.highlight_save_hint {
        Style::default()
            .fg(Color::Rgb(255, 255, 100))
            .bg(Color::Rgb(100, 100, 50))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(STATUS_BAR_FG).bg(STATUS_BAR_BG)
    };

    let ctrl_q_hint = "Ctrl-Q to save and exit ";

    let padding = area
        .width
        .saturating_sub(left.len() as u16 + ctrl_q_hint.len() as u16);

    let line = Line::from(vec![
        Span::styled(&left, Style::default().fg(STATUS_BAR_FG).bg(STATUS_BAR_BG)),
        Span::styled(
            " ".repeat(padding as usize),
            Style::default().bg(STATUS_BAR_BG),
        ),
        Span::styled(ctrl_q_hint, ctrl_q_style),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_dropdown(frame: &mut ratatui::Frame, app: &EditorApp, menu_index: usize) {
    let items = menu_items(menu_index);
    let rect = app.dropdown_rect(menu_index);

    let screen = frame.area();
    let rect = Rect::new(
        rect.x.min(screen.width.saturating_sub(rect.width)),
        rect.y,
        rect.width.min(screen.width),
        rect.height.min(screen.height.saturating_sub(rect.y)),
    );

    frame.render_widget(Clear, rect);

    let inner_width = rect.width.saturating_sub(2) as usize;
    let mut lines = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let shortcut_len = item.shortcut.len();
        let label_space = inner_width.saturating_sub(shortcut_len + 4);
        let text = format!(
            "  {:<width$}  {}",
            item.label,
            item.shortcut,
            width = label_space,
        );
        let style = if i == app.menu_item_index {
            Style::default().fg(DROPDOWN_FG).bg(DROPDOWN_HIGHLIGHT_BG)
        } else {
            Style::default().fg(DROPDOWN_FG).bg(DROPDOWN_BG)
        };
        lines.push(Line::styled(text, style));
    }

    let dropdown = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(
                Style::default()
                    .fg(Color::Rgb(140, 140, 180))
                    .bg(DROPDOWN_BG),
            )
            .style(Style::default().bg(DROPDOWN_BG)),
    );
    frame.render_widget(dropdown, rect);
}

fn render_help_overlay(frame: &mut ratatui::Frame) {
    let screen = frame.area();
    let width = 50u16.min(screen.width.saturating_sub(4));
    let height = 25u16.min(screen.height.saturating_sub(4));
    let x = (screen.width.saturating_sub(width)) / 2;
    let y = (screen.height.saturating_sub(height)) / 2;
    let rect = Rect::new(x, y, width, height);

    frame.render_widget(Clear, rect);

    let shortcut = |s: &'static str| Line::styled(s, Style::default().fg(DROPDOWN_FG));
    let heading = |s: &'static str| {
        Line::styled(
            s,
            Style::default()
                .fg(MENU_ACTIVE_BG)
                .add_modifier(Modifier::BOLD),
        )
    };

    let help_text = vec![
        heading("  File"),
        shortcut("  Ctrl+S        Save"),
        shortcut("  Ctrl+Q        Save & Quit"),
        shortcut("  Esc           Highlight save hint"),
        shortcut("  F10           Open Menu"),
        Line::raw(""),
        heading("  Edit"),
        shortcut("  Ctrl+Z        Undo"),
        shortcut("  Ctrl+Y        Redo"),
        shortcut("  Ctrl+A        Go to end of line"),
        shortcut("  Ctrl+J        Next line"),
        shortcut("  Ctrl+K        Prev line"),
        shortcut("  Ctrl+L        Next paragraph"),
        shortcut("  Ctrl+P        Prev paragraph"),
        shortcut("  Ctrl+D        Delete paragraph"),
        shortcut("  Ctrl+E        Empty (delete all)"),
        shortcut("  Alt+Z         Toggle word wrap"),
        Line::raw(""),
        Line::styled(
            "  Press any key to close",
            Style::default().fg(Color::DarkGray),
        ),
    ];

    let help = Paragraph::new(help_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Help ")
            .border_style(
                Style::default()
                    .fg(Color::Rgb(140, 140, 180))
                    .bg(DROPDOWN_BG),
            )
            .style(Style::default().bg(DROPDOWN_BG)),
    );
    frame.render_widget(help, rect);
}

fn render_about_overlay(frame: &mut ratatui::Frame) {
    let screen = frame.area();
    let width = 52u16.min(screen.width.saturating_sub(4));
    let height = 13u16.min(screen.height.saturating_sub(4));
    let x = (screen.width.saturating_sub(width)) / 2;
    let y = (screen.height.saturating_sub(height)) / 2;
    let rect = Rect::new(x, y, width, height);

    frame.render_widget(Clear, rect);

    let title_style = Style::default()
        .fg(Color::Rgb(120, 200, 120))
        .add_modifier(Modifier::BOLD);
    let info_style = Style::default().fg(Color::Rgb(120, 200, 120));
    let charity_style = Style::default()
        .fg(Color::Rgb(200, 200, 100))
        .add_modifier(Modifier::BOLD);
    let hint_style = Style::default().fg(Color::Rgb(120, 200, 120));
    let dim_style = Style::default().fg(Color::DarkGray);

    let about_text = vec![
        Line::raw(""),
        Line::from(vec![Span::styled("SOE - Scott's Own Editor", title_style)])
            .alignment(ratatui::layout::Alignment::Center),
        Line::raw(""),
        Line::from(vec![Span::styled("by Scott Chacon et al.", info_style)])
            .alignment(ratatui::layout::Alignment::Center),
        Line::from(vec![Span::styled(
            "SOE is open source and freely distributable",
            info_style,
        )])
        .alignment(ratatui::layout::Alignment::Center),
        Line::raw(""),
        Line::from(vec![Span::styled(
            "Help poor doggies in Germany!",
            charity_style,
        )])
        .alignment(ratatui::layout::Alignment::Center),
        Line::from(vec![Span::styled(
            "type :teddy for more information",
            hint_style,
        )])
        .alignment(ratatui::layout::Alignment::Center),
        Line::raw(""),
        Line::raw(""),
        Line::from(vec![Span::styled("Press any key to close", dim_style)])
            .alignment(ratatui::layout::Alignment::Center),
    ];

    let about = Paragraph::new(about_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(
                Style::default()
                    .fg(Color::Rgb(140, 140, 180))
                    .bg(DROPDOWN_BG),
            )
            .style(Style::default().bg(DROPDOWN_BG)),
    );
    frame.render_widget(about, rect);
}

fn render_teddy_overlay(frame: &mut ratatui::Frame) {
    let screen = frame.area();
    let width = 54u16.min(screen.width.saturating_sub(4));
    let height = 14u16.min(screen.height.saturating_sub(4));
    let x = (screen.width.saturating_sub(width)) / 2;
    let y = (screen.height.saturating_sub(height)) / 2;
    let rect = Rect::new(x, y, width, height);

    frame.render_widget(Clear, rect);

    let title_style = Style::default()
        .fg(Color::Rgb(200, 200, 100))
        .add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(Color::Rgb(200, 200, 200));
    let url_style = Style::default()
        .fg(Color::Rgb(100, 180, 255))
        .add_modifier(Modifier::UNDERLINED);
    let dim_style = Style::default().fg(Color::DarkGray);

    let teddy_text = vec![
        Line::raw(""),
        Line::from(vec![Span::styled(
            "Help poor doggies in Germany!",
            title_style,
        )])
        .alignment(ratatui::layout::Alignment::Center),
        Line::raw(""),
        Line::from(vec![Span::styled(
            "Teddy Farms rescues and rehabilitates",
            text_style,
        )])
        .alignment(ratatui::layout::Alignment::Center),
        Line::from(vec![Span::styled(
            "dogs in need across Germany.",
            text_style,
        )])
        .alignment(ratatui::layout::Alignment::Center),
        Line::raw(""),
        Line::from(vec![Span::styled(
            "Learn more and donate at:",
            text_style,
        )])
        .alignment(ratatui::layout::Alignment::Center),
        Line::raw(""),
        Line::from(vec![Span::styled(
            "https://teddyfarms.com",
            url_style,
        )])
        .alignment(ratatui::layout::Alignment::Center),
        Line::raw(""),
        Line::raw(""),
        Line::from(vec![Span::styled("Press any key to close", dim_style)])
            .alignment(ratatui::layout::Alignment::Center),
    ];

    let teddy = Paragraph::new(teddy_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Teddy Farms ")
            .border_style(
                Style::default()
                    .fg(Color::Rgb(200, 200, 100))
                    .bg(DROPDOWN_BG),
            )
            .style(Style::default().bg(DROPDOWN_BG)),
    );
    frame.render_widget(teddy, rect);
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Opens the built-in TUI text editor.
///
/// Returns `Some(content)` if the user saved, or `None` if they cancelled.
pub fn run_builtin_editor(
    filename: &str,
    initial_content: &str,
    mode: EditorMode,
) -> anyhow::Result<Option<String>> {
    let mut guard = crate::terminal::TerminalGuard::new(true)?;
    let mut app = EditorApp::new(filename, initial_content, mode);

    loop {
        guard.terminal_mut().draw(|frame| render(frame, &mut app))?;

        if event::poll(Duration::from_millis(50))? {
            let ev = event::read()?;
            app.handle_event(ev);
        }

        if app.should_quit {
            break;
        }
    }

    if app.save_on_quit {
        Ok(Some(app.content()))
    } else {
        Ok(None)
    }
}

/// Opens the built-in editor for a file on disk.
/// Reads the file, lets the user edit, and writes it back on save.
pub fn edit_file(path: &std::path::Path) -> anyhow::Result<()> {
    let content = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };

    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "untitled".to_string());

    if let Some(new_content) = run_builtin_editor(&filename, &content, EditorMode::PlainText)? {
        std::fs::write(path, new_content)?;
    }

    Ok(())
}
