use crate::{render, theme::Theme, App};
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
};
use ratatui::{backend::Backend, Terminal};
use std::{io, path::Path, time::Duration};

pub fn run<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    run_with_context(terminal, "mock")
}

pub fn run_with_context<B: Backend>(
    terminal: &mut Terminal<B>,
    context_location: impl AsRef<Path>,
) -> io::Result<()> {
    let _paste = PasteGuard::enable()?;
    let app = App::open(context_location).map_err(|error| io::Error::other(error.to_string()))?;
    run_loop(terminal, app)
}

struct PasteGuard;

impl PasteGuard {
    fn enable() -> io::Result<Self> {
        execute!(io::stdout(), EnableBracketedPaste)?;
        Ok(Self)
    }
}

impl Drop for PasteGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableBracketedPaste);
    }
}

fn run_loop<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    loop {
        app.tick();
        terminal.draw(|frame| render::draw(frame, &app, Theme::detect()))?;
        if app.should_quit {
            return Ok(());
        }
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    if !app.interrupt_active() {
                        app.quit_cleanly();
                    }
                    continue;
                }
                app.handle_key(key);
            }
            Event::Resize(_, _) => {}
            Event::Paste(value) if app.overlay == crate::Overlay::Command => {
                app.paste_command(&value);
            }
            _ => {}
        }
    }
}
