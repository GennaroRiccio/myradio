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

/// Durata massima del banner di avvio (si chiude anche al primo tasto).
const SPLASH_DURATION: Duration = Duration::from_secs(10);

fn main() -> Result<()> {
    let _log_guard = init_tracing();
    let terminal = ratatui::init();
    let result = run_app(terminal);
    ratatui::restore();
    result
}

/// Inizializza il logging verso `logs/` senza toccare il terminale TUI.
///
/// I file ruotano ogni giorno e ne vengono conservati al massimo 7. Il guard
/// restituito mantiene vivo il worker di scrittura per tutta la durata del
/// processo: al drop viene scaricato il buffer rimanente.
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
    let splash_start = Instant::now();

    while !app.should_exit() {
        terminal.draw(|frame| ui::render(frame, &mut app))?;

        if app.splash && splash_start.elapsed() >= SPLASH_DURATION {
            app.dismiss_splash();
        }

        if event::poll(Duration::from_millis(30))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if app.splash {
                        app.dismiss_splash();
                    } else {
                        app.handle_input(key);
                    }
                }
                Event::Mouse(mouse) if !app.splash => app.handle_mouse(mouse),
                _ => {}
            }
        }
        app.process_messages();
    }

    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    Ok(())
}
