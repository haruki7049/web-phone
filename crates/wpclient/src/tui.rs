//! Terminal User Interface (TUI) module for `wpclient` powered by Ratatui & Crossterm.

pub mod app;
pub mod event;
pub mod ui;

pub use app::*;
pub use event::*;
pub use ui::*;

use anyhow::Result;
use crossterm::{
    event::{Event as CrossEvent, EventStream, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io::stdout, time::Duration};
use tokio::sync::mpsc;
use wpapi::Configuration;

/// Run the main TUI loop.
pub async fn run_tui(config: Configuration) -> Result<()> {
    run_tui_with_keypair(config, None).await
}

/// RAII guard struct to guarantee terminal restoration on error or panic.
pub struct TerminalGuard;

impl TerminalGuard {
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen)?;

        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let _ = disable_raw_mode();
            let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
            original_hook(panic_info);
        }));

        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    }
}

/// Run the main TUI loop with an optional pre-loaded UserKeypair.
pub async fn run_tui_with_keypair(
    config: Configuration,
    keypair: Option<wpapi::UserKeypair>,
) -> Result<()> {
    let _guard = TerminalGuard::new()?;
    let stdout = stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (event_tx, mut event_rx) = mpsc::channel::<AppEvent>(100);
    let mut app = TuiApp::with_keypair(config, event_tx.clone(), keypair);

    let mut event_stream = EventStream::new();
    let mut tick_interval = tokio::time::interval(Duration::from_millis(50));

    start_standby(&mut app);

    loop {
        terminal.draw(|f| render_ui(f, &app))?;

        tokio::select! {
            _ = tick_interval.tick() => {
                if let Some(ref session) = app.session {
                    app.input_level = session.get_input_level();
                    app.output_level = session.get_output_level();
                } else {
                    app.input_level = (app.input_level * 0.85).max(0.0);
                    app.output_level = (app.output_level * 0.85).max(0.0);
                }
            }
            Some(evt) = event_rx.recv() => {
                handle_app_event(&mut app, evt);
            }
            maybe_event = event_stream.next() => {
                match maybe_event {
                    Some(Ok(CrossEvent::Key(key))) if key.kind == KeyEventKind::Press => {
                        if handle_key_input(&mut app, key).await? {
                            break;
                        }
                    }
                    Some(Ok(CrossEvent::Resize(_, _))) => {}
                    _ => {}
                }
            }
        }
    }

    app.hangup();
    if let Some(tx) = app.cancel_tx.take() {
        let _ = tx.send(());
    }
    terminal.show_cursor()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn test_auto_accept_toggle() {
        let (tx, _rx) = mpsc::channel(10);
        let mut app = TuiApp::new(Configuration::default(), tx);
        assert!(!app.config.auto_accept);

        let key = crossterm::event::KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let res = handle_key_input(&mut app, key).await.unwrap();
        assert!(!res);
        assert!(app.config.auto_accept);

        let key_cap = crossterm::event::KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE);
        let res = handle_key_input(&mut app, key_cap).await.unwrap();
        assert!(!res);
        assert!(!app.config.auto_accept);
    }

    #[tokio::test]
    async fn test_incoming_call_manual_accept_and_reject() {
        let (tx, _rx) = mpsc::channel(10);
        let mut app = TuiApp::new(Configuration::default(), tx);
        assert!(!app.config.auto_accept);

        let (resp_tx, resp_rx) = oneshot::channel::<bool>();
        handle_app_event(
            &mut app,
            AppEvent::IncomingCall {
                from: "test_caller".to_string(),
                responder: resp_tx,
            },
        );

        assert_eq!(app.input_mode, InputMode::IncomingCall);
        assert_eq!(app.incoming_from.as_deref(), Some("test_caller"));

        let key_y = crossterm::event::KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let quit = handle_key_input(&mut app, key_y).await.unwrap();
        assert!(!quit);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(resp_rx.await.unwrap());
    }

    #[test]
    fn test_parse_key_event() {
        let key_ctrl_c = crossterm::event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(
            parse_key_event(&InputMode::Normal, key_ctrl_c),
            AppAction::Quit
        );

        let key_q = crossterm::event::KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(parse_key_event(&InputMode::Normal, key_q), AppAction::Quit);

        let key_m = crossterm::event::KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE);
        assert_eq!(
            parse_key_event(&InputMode::Normal, key_m),
            AppAction::ToggleMute
        );

        let key_c = crossterm::event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
        assert_eq!(
            parse_key_event(&InputMode::Normal, key_c),
            AppAction::EnterCallInput
        );

        let key_x = crossterm::event::KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(
            parse_key_event(&InputMode::CallInput, key_x),
            AppAction::InputChar('x')
        );

        let key_esc = crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(
            parse_key_event(&InputMode::CallInput, key_esc),
            AppAction::CancelInputMode
        );
    }
}
