pub mod app;
pub mod view;

use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event};
use ratatui::{init, restore, DefaultTerminal};

use crate::cli::Args;

use self::app::AppState;

pub fn run(args: Args) -> Result<()> {
    let mut terminal = init();
    let result = run_app(&mut terminal, args);
    restore();
    result
}

fn run_app(terminal: &mut DefaultTerminal, args: Args) -> Result<()> {
    let mut app = AppState::new(args)?;
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|frame| view::render(frame, &mut app))?;
        app.check_encoding_result()?;

        let timeout = app.poll_timeout(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                app.on_key(key)?;
            }
        }

        if app.should_quit {
            break;
        }

        if app.should_advance(last_tick.elapsed()) {
            app.advance_playback()?;
            last_tick = Instant::now();
        }
    }

    Ok(())
}

fn _frame_duration(fps: u16) -> Duration {
    Duration::from_secs_f32(1.0 / fps.max(1) as f32)
}
