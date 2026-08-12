//! Entry point dell'applicazione myradio.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::execute;
use ratatui::DefaultTerminal;

use myradio::app::{App, Msg};
use myradio::audio::{self, EngineHandle};
use myradio::ui;

/// Maximum duration of the startup banner (closes on first key press too).
const SPLASH_DURATION: Duration = Duration::from_secs(10);

/// Fast tick for animated/active states (splash, visualizer, loading).
const ACTIVE_TICK: Duration = Duration::from_millis(30);

/// Slower tick when the UI is stable, to reduce CPU usage.
const IDLE_TICK: Duration = Duration::from_millis(120);

fn main() -> Result<()> {
    let _log_guard = init_tracing();
    let terminal = ratatui::init();
    let result = run_app(terminal);
    ratatui::restore();
    result
}

/// Initialize logging to `logs/` without touching the TUI terminal.
///
/// Files rotate daily and at most 7 are kept. The returned guard keeps the
/// write worker alive for the entire process: on drop the remaining buffer
/// is flushed.
fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_appender::rolling::{Builder, Rotation};
    use tracing_subscriber::filter::LevelFilter;

    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("myradio")
        .filename_suffix("log")
        .max_log_files(7)
        .build("logs");
    let Ok(appender) = appender else {
        return None;
    };

    let (non_blocking, guard) = tracing_appender::non_blocking(appender);
    let _ = tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_max_level(LevelFilter::WARN)
        .with_ansi(false)
        .try_init();
    Some(guard)
}

fn run_app(mut terminal: DefaultTerminal) -> Result<()> {
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let (msg_tx, msg_rx) = mpsc::channel();

    let engine = match audio::spawn(msg_tx.clone()) {
        Ok(handle) => handle,
        Err(error) => {
            let _ = msg_tx.send(Msg::PlaybackError(error.to_string()));
            EngineHandle::broken()
        }
    };

    let mut app = App::new(msg_tx, msg_rx, engine);
    app.load_favorites();
    let splash_start = Instant::now();

    while !app.should_exit() {
        terminal.draw(|frame| ui::render(frame, &mut app))?;

        if app.splash && splash_start.elapsed() >= SPLASH_DURATION {
            app.dismiss_splash();
        }

        let tick = if app.splash || app.playback.is_active() || app.loading {
            ACTIVE_TICK
        } else {
            IDLE_TICK
        };

        if event::poll(tick)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if app.splash {
                        app.dismiss_splash();
                    } else {
                        app.handle_input(key);
                    }
                }
                Event::Mouse(mouse) if !app.splash && !app.menu_open => app.handle_mouse(mouse),
                _ => {}
            }
        }
        app.process_messages();
    }

    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    Ok(())
}
