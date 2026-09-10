//! Key event handling and action mapping for TUI client.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};

use super::app::*;

/// User actions triggered via TUI keyboard input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppAction {
    Quit,
    StartStandby,
    StopStandby,
    RenewIdentity,
    EnterCallInput,
    EnterRoomInput,
    ToggleMute,
    ToggleAutoAccept,
    Hangup,
    FetchRegisteredAddresses,
    SubmitCallInput,
    SubmitRoomInput,
    CancelInputMode,
    InputChar(char),
    BackspaceInput,
    AcceptIncomingCall,
    RejectIncomingCall,
    None,
}

/// Parse a raw keyboard event into a high-level `AppAction` depending on the current input mode.
pub fn parse_key_event(input_mode: &InputMode, key: crossterm::event::KeyEvent) -> AppAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return AppAction::Quit;
    }

    match input_mode {
        InputMode::Normal => match key.code {
            KeyCode::Char('q') => AppAction::Quit,
            KeyCode::Char('s') => AppAction::StartStandby,
            KeyCode::Char('i') | KeyCode::Char('I') => AppAction::StopStandby,
            KeyCode::Char('u') | KeyCode::Char('k') | KeyCode::Char('U') | KeyCode::Char('K') => {
                AppAction::RenewIdentity
            }
            KeyCode::Char('c') => AppAction::EnterCallInput,
            KeyCode::Char('r') => AppAction::EnterRoomInput,
            KeyCode::Char('m') => AppAction::ToggleMute,
            KeyCode::Char('a') | KeyCode::Char('A') => AppAction::ToggleAutoAccept,
            KeyCode::Char('h') | KeyCode::Char('x') => AppAction::Hangup,
            KeyCode::Char('l') => AppAction::FetchRegisteredAddresses,
            _ => AppAction::None,
        },
        InputMode::CallInput => match key.code {
            KeyCode::Enter => AppAction::SubmitCallInput,
            KeyCode::Esc => AppAction::CancelInputMode,
            KeyCode::Char(c) => AppAction::InputChar(c),
            KeyCode::Backspace => AppAction::BackspaceInput,
            _ => AppAction::None,
        },
        InputMode::RoomInput => match key.code {
            KeyCode::Enter => AppAction::SubmitRoomInput,
            KeyCode::Esc => AppAction::CancelInputMode,
            KeyCode::Char(c) => AppAction::InputChar(c),
            KeyCode::Backspace => AppAction::BackspaceInput,
            _ => AppAction::None,
        },
        InputMode::IncomingCall => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                AppAction::AcceptIncomingCall
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => AppAction::RejectIncomingCall,
            _ => AppAction::None,
        },
    }
}

/// Execute an `AppAction` against the `TuiApp` state. Returns `Ok(true)` if application exit is requested.
pub async fn execute_action(app: &mut TuiApp, action: AppAction) -> Result<bool> {
    match action {
        AppAction::Quit => return Ok(true),
        AppAction::StartStandby => start_standby(app),
        AppAction::StopStandby => stop_standby(app),
        AppAction::RenewIdentity => renew_identity(app),
        AppAction::EnterCallInput => {
            app.input_buffer.clear();
            app.input_mode = InputMode::CallInput;
        }
        AppAction::EnterRoomInput => {
            app.input_buffer.clear();
            app.input_mode = InputMode::RoomInput;
        }
        AppAction::ToggleMute => {
            app.is_muted = !app.is_muted;
            let status = if app.is_muted { "Muted" } else { "Unmuted" };
            app.add_log(format!("Microphone is now {}", status));
        }
        AppAction::ToggleAutoAccept => {
            app.config.auto_accept = !app.config.auto_accept;
            let status = if app.config.auto_accept {
                "enabled"
            } else {
                "disabled"
            };
            app.add_log(format!("Auto Accept is now {}", status));
        }
        AppAction::Hangup => {
            app.hangup();
        }
        AppAction::FetchRegisteredAddresses => {
            fetch_registered_addresses(app);
        }
        AppAction::SubmitCallInput => {
            let target = app.input_buffer.trim().to_string();
            app.input_mode = InputMode::Normal;
            if !target.is_empty() {
                start_call(app, target);
            }
        }
        AppAction::SubmitRoomInput => {
            let room = app.input_buffer.trim().to_string();
            app.input_mode = InputMode::Normal;
            if !room.is_empty() {
                start_room(app, room);
            }
        }
        AppAction::CancelInputMode => {
            app.input_mode = InputMode::Normal;
        }
        AppAction::InputChar(c) => {
            app.input_buffer.push(c);
        }
        AppAction::BackspaceInput => {
            app.input_buffer.pop();
        }
        AppAction::AcceptIncomingCall => {
            if let Some(resp) = app.incoming_responder.take() {
                let _ = resp.send(true);
                app.add_log("Accepted incoming call.".to_string());
            }
            if let Some(from) = app.incoming_from.take() {
                app.call_state = CallState::InCall(from);
            }
            app.input_mode = InputMode::Normal;
        }
        AppAction::RejectIncomingCall => {
            if let Some(resp) = app.incoming_responder.take() {
                let _ = resp.send(false);
                app.add_log("Rejected incoming call.".to_string());
            }
            if let Some(ref session) = app.session {
                session.set_target_address(None);
            }
            app.input_mode = InputMode::Normal;
            app.incoming_from = None;
        }
        AppAction::None => {}
    }

    Ok(false)
}

pub(crate) async fn handle_key_input(
    app: &mut TuiApp,
    key: crossterm::event::KeyEvent,
) -> Result<bool> {
    let action = parse_key_event(&app.input_mode, key);
    execute_action(app, action).await
}
