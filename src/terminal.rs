//! RAII terminal guard for crossterm + ratatui.

use std::io;
use std::sync::{Arc, Mutex};

use crossterm::event::{DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync>;

/// RAII guard that ensures the terminal is restored to its original state,
/// even if an error occurs or a panic is caught.
#[must_use]
pub(crate) struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    mouse_captured: bool,
    original_hook: Arc<Mutex<Option<PanicHook>>>,
}

impl TerminalGuard {
    /// Enter raw mode, alternate screen, and optionally enable mouse capture.
    pub fn new(enable_mouse: bool) -> anyhow::Result<Self> {
        let original_hook: Arc<Mutex<Option<PanicHook>>> =
            Arc::new(Mutex::new(Some(std::panic::take_hook())));

        let hook_ref = Arc::clone(&original_hook);
        let mouse = enable_mouse;
        std::panic::set_hook(Box::new(move |panic_info| {
            let _ = disable_raw_mode();
            if mouse {
                let _ = crossterm::execute!(
                    io::stdout(),
                    DisableMouseCapture,
                    DisableFocusChange,
                    LeaveAlternateScreen
                );
            } else {
                let _ = crossterm::execute!(io::stdout(), DisableFocusChange, LeaveAlternateScreen);
            }
            if let Some(hook) = hook_ref.lock().ok().and_then(|mut h| h.take()) {
                hook(panic_info);
            }
        }));

        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if enable_mouse {
            crossterm::execute!(
                stdout,
                EnterAlternateScreen,
                EnableMouseCapture,
                EnableFocusChange
            )?;
        } else {
            crossterm::execute!(stdout, EnterAlternateScreen, EnableFocusChange)?;
        }
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            mouse_captured: enable_mouse,
            original_hook,
        })
    }

    pub fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        if self.mouse_captured {
            let _ = crossterm::execute!(
                self.terminal.backend_mut(),
                DisableMouseCapture,
                DisableFocusChange,
                LeaveAlternateScreen
            );
        } else {
            let _ = crossterm::execute!(
                self.terminal.backend_mut(),
                DisableFocusChange,
                LeaveAlternateScreen
            );
        }
        let _ = self.terminal.show_cursor();

        if let Some(hook) = self.original_hook.lock().ok().and_then(|mut h| h.take()) {
            std::panic::set_hook(hook);
        }
    }
}
