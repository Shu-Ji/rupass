use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::storage::AppPaths;
use crate::tui_app::App;
use crate::tui_view::draw;

type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

pub(crate) fn run(paths: AppPaths) -> Result<()> {
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

    let result = run_loop(&mut terminal, App::new(paths)?);

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    result
}

fn run_loop(terminal: &mut TuiTerminal, mut app: App) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, &app))?;
        if event::poll(Duration::from_millis(120)).context("failed to poll terminal events")?
            && let Event::Key(key) = event::read().context("failed to read terminal event")?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            let should_quit = match app.handle_key(key) {
                Ok(quit) => quit,
                Err(err) => {
                    app.show_error(err.to_string());
                    false
                }
            };
            if should_quit {
                return Ok(());
            }
        }
    }
}
