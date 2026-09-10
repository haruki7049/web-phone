//! Terminal User Interface (TUI) module for `wpclient` powered by Ratatui & Crossterm.

use anyhow::Result;
use crossterm::{
    event::{Event as CrossEvent, EventStream, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Wrap},
};
use std::{collections::VecDeque, io::stdout, time::Duration};
use tokio::sync::{mpsc, oneshot};
use wpapi::{Configuration, UserAddress, session::ClientSession};

/// Background events sent to the TUI event loop.
#[derive(Debug)]
pub enum AppEvent {
    ServerAddressAssigned(UserAddress),
    SessionEnded {
        session_id: u64,
        result: Result<String, String>,
    },
    CallAccepted {
        session_id: u64,
        target: String,
    },
    CallRejected {
        session_id: u64,
        target: String,
        reason: String,
    },
    CallEnded {
        target: String,
    },
    IncomingCall {
        from: String,
        responder: oneshot::Sender<bool>,
    },
    RegisteredAddresses(Vec<UserAddress>),
    AudioLevels {
        input: f32,
        output: f32,
    },
    Log(String),
}

/// UI Interaction Modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    CallInput,
    RoomInput,
    IncomingCall,
}

/// Connection and call state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallState {
    Idle,
    Standby,
    Connecting(String),
    InCall(String),
    InRoom(String),
}

/// Application state for the TUI client.
pub struct TuiApp {
    pub config: Configuration,
    pub input_mode: InputMode,
    pub call_state: CallState,
    pub current_session_id: u64,
    pub my_address: Option<UserAddress>,
    pub input_buffer: String,
    pub logs: VecDeque<String>,
    pub registered_addresses: Vec<UserAddress>,
    pub is_muted: bool,
    pub input_level: f32,
    pub output_level: f32,
    pub incoming_from: Option<String>,
    pub incoming_responder: Option<oneshot::Sender<bool>>,
    pub cancel_tx: Option<oneshot::Sender<()>>,
    pub session: Option<ClientSession>,
    pub event_tx: mpsc::Sender<AppEvent>,
}

impl TuiApp {
    pub fn new(config: Configuration, event_tx: mpsc::Sender<AppEvent>) -> Self {
        Self::with_keypair(config, event_tx, None)
    }

    pub fn with_keypair(
        config: Configuration,
        event_tx: mpsc::Sender<AppEvent>,
        keypair: Option<wpapi::UserKeypair>,
    ) -> Self {
        let session = if let Some(kp) = keypair {
            ClientSession::with_keypair(kp)
        } else {
            ClientSession::new()
        };
        let mut app = Self {
            config,
            input_mode: InputMode::Normal,
            call_state: CallState::Idle,
            current_session_id: 0,
            my_address: None,
            input_buffer: String::new(),
            logs: VecDeque::with_capacity(100),
            registered_addresses: Vec::new(),
            is_muted: false,
            input_level: 0.0,
            output_level: 0.0,
            incoming_from: None,
            incoming_responder: None,
            cancel_tx: None,
            session: Some(session),
            event_tx,
        };
        app.add_log(
            "TUI Application initialized. Press [s] for standby, [c] to call, [r] for room."
                .to_string(),
        );
        app
    }

    pub fn add_log(&mut self, msg: String) {
        if self.logs.len() >= 100 {
            self.logs.pop_front();
        }
        self.logs.push_back(msg);
    }

    pub fn next_session_id(&mut self) -> u64 {
        self.current_session_id += 1;
        self.current_session_id
    }

    /// Hangup current call/session if active.
    pub fn hangup(&mut self) {
        if let Some(ref session) = self.session {
            if let Some(target) = session.get_target_address() {
                let hangup_pkt = wpapi::protocol::ProtocolPacket::CallHangup {
                    target_address: target,
                };
                let session_clone = session.clone();
                tokio::spawn(async move {
                    let _ = session_clone.send_packet(&hangup_pkt).await;
                });
                session.set_target_address(None);
            }
            if let Some(room) = session.get_room_address() {
                let leave_pkt =
                    wpapi::protocol::ProtocolPacket::RoomLeaveRequest { room_address: room };
                let session_clone = session.clone();
                tokio::spawn(async move {
                    let _ = session_clone.send_packet(&leave_pkt).await;
                });
                session.set_room_address(None);
            }
        }
        self.input_level = 0.0;
        self.output_level = 0.0;
        if self.cancel_tx.is_some() {
            self.call_state = CallState::Standby;
            self.add_log("Returned to Standby Mode.".to_string());
        } else {
            self.call_state = CallState::Idle;
            self.add_log("Returned to IDLE state.".to_string());
        }
    }
}

/// Run the main TUI loop.
pub async fn run_tui(config: Configuration) -> Result<()> {
    run_tui_with_keypair(config, None).await
}

/// Run the main TUI loop with an optional pre-loaded UserKeypair.
pub async fn run_tui_with_keypair(
    config: Configuration,
    keypair: Option<wpapi::UserKeypair>,
) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Custom panic hook to restore terminal on crash
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    let (event_tx, mut event_rx) = mpsc::channel::<AppEvent>(100);
    let mut app = TuiApp::with_keypair(config, event_tx.clone(), keypair);

    let mut event_stream = EventStream::new();
    let mut tick_interval = tokio::time::interval(Duration::from_millis(50));

    // Auto-start Standby mode on launch
    start_standby(&mut app);

    loop {
        terminal.draw(|f| render_ui(f, &app))?;

        tokio::select! {
            _ = tick_interval.tick() => {
                // Audio level decay for visual responsiveness
                app.input_level = (app.input_level * 0.85).max(0.0);
                app.output_level = (app.output_level * 0.85).max(0.0);
            }
            Some(evt) = event_rx.recv() => {
                handle_app_event(&mut app, evt);
            }
            maybe_event = event_stream.next() => {
                match maybe_event {
                    Some(Ok(CrossEvent::Key(key))) if key.kind == KeyEventKind::Press => {
                        if handle_key_input(&mut app, key).await? {
                            break; // Quit requested
                        }
                    }
                    Some(Ok(CrossEvent::Resize(_, _))) => {
                        // Terminal resized, naturally redrawn in next iteration
                    }
                    _ => {}
                }
            }
        }
    }

    // Cleanup and restore terminal
    app.hangup();
    if let Some(tx) = app.cancel_tx.take() {
        let _ = tx.send(());
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn setup_incoming_call_handler(session: &ClientSession, event_tx: mpsc::Sender<AppEvent>) {
    let (inc_tx, mut inc_rx) = mpsc::channel::<(UserAddress, oneshot::Sender<bool>)>(10);
    session.set_incoming_call_handler(inc_tx);

    tokio::spawn(async move {
        while let Some((caller_addr, responder)) = inc_rx.recv().await {
            let _ = event_tx
                .send(AppEvent::IncomingCall {
                    from: caller_addr.short_id().to_string(),
                    responder,
                })
                .await;
        }
    });
}

fn setup_call_notification_handler(
    session: &ClientSession,
    event_tx: mpsc::Sender<AppEvent>,
    session_id: u64,
) {
    let (notif_tx, mut notif_rx) = mpsc::channel::<wpapi::session::CallNotification>(10);
    session.set_call_notification_handler(notif_tx);

    tokio::spawn(async move {
        while let Some(notif) = notif_rx.recv().await {
            match notif {
                wpapi::session::CallNotification::Accepted(target_addr) => {
                    let _ = event_tx
                        .send(AppEvent::CallAccepted {
                            session_id,
                            target: target_addr.short_id().to_string(),
                        })
                        .await;
                }
                wpapi::session::CallNotification::Rejected(target_addr) => {
                    let _ = event_tx
                        .send(AppEvent::CallRejected {
                            session_id,
                            target: target_addr.short_id().to_string(),
                            reason: "Connection rejected by target user".to_string(),
                        })
                        .await;
                }
                wpapi::session::CallNotification::Error(target_addr, reason) => {
                    let _ = event_tx
                        .send(AppEvent::CallRejected {
                            session_id,
                            target: target_addr.short_id().to_string(),
                            reason,
                        })
                        .await;
                }
                wpapi::session::CallNotification::Hangup(target_addr) => {
                    let _ = event_tx
                        .send(AppEvent::CallEnded {
                            target: target_addr.short_id().to_string(),
                        })
                        .await;
                }
            }
        }
    });
}

fn stop_standby(app: &mut TuiApp) {
    if let Some(tx) = app.cancel_tx.take() {
        let _ = tx.send(());
    }
    if let Some(ref session) = app.session {
        session.set_target_address(None);
        session.set_room_address(None);
    }
    app.call_state = CallState::Idle;
    app.add_log("Standby mode stopped. Entered IDLE state.".to_string());
}

fn start_standby(app: &mut TuiApp) {
    if app.cancel_tx.is_some() {
        stop_standby(app);
        return;
    }

    let session_id = app.next_session_id();
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let (assigned_tx, mut assigned_rx) = mpsc::channel::<UserAddress>(1);
    let event_tx = app.event_tx.clone();
    let config = app.config.clone();

    app.cancel_tx = Some(cancel_tx);
    app.call_state = CallState::Standby;
    app.add_log("Starting standby mode (listening for incoming calls)...".to_string());

    let session = app.session.clone().unwrap_or_default();
    setup_incoming_call_handler(&session, event_tx.clone());
    setup_call_notification_handler(&session, event_tx.clone(), session_id);

    let session_clone = session.clone();
    tokio::spawn(async move {
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if let Some(addr) = session_clone.get_user_address() {
                let _ = assigned_tx.send(addr).await;
                break;
            }
        }
    });

    let event_tx_addr = event_tx.clone();
    tokio::spawn(async move {
        if let Some(addr) = assigned_rx.recv().await {
            let _ = event_tx_addr
                .send(AppEvent::ServerAddressAssigned(addr))
                .await;
        }
    });

    let event_tx_session = event_tx;
    tokio::spawn(async move {
        let res = wpapi::call::start_call_with_session(&config, None, Some(cancel_rx), &session)
            .await
            .map(|_| "Standby ended".to_string())
            .map_err(|e| e.to_string());
        let _ = event_tx_session
            .send(AppEvent::SessionEnded {
                session_id,
                result: res,
            })
            .await;
    });
}

fn start_call(app: &mut TuiApp, target_id: String) {
    let clean_id = target_id.trim().to_string();
    if clean_id.len() < 12 {
        app.add_log(format!(
            "Target Short ID must be at least 12 characters (got {}).",
            clean_id.len()
        ));
        app.call_state = CallState::Standby;
        return;
    }
    let target_addr = UserAddress::new(clean_id.clone());
    app.call_state = CallState::Connecting(clean_id.clone());
    app.add_log(format!("Initiating call to target User ID: {}", clean_id));

    if let Some(ref session) = app.session {
        session.set_target_address(Some(target_addr.clone()));
        let init_packet = wpapi::protocol::ProtocolPacket::ClientTargetedAudio {
            target_address: target_addr,
            codec_id: wpapi::protocol::CODEC_OPUS,
            audio_data: vec![],
        };
        let session_clone = session.clone();
        tokio::spawn(async move {
            if let Err(e) = session_clone.send_packet(&init_packet).await {
                tracing::error!("Failed to send call initiation packet: {}", e);
            }
        });
    }
}

fn start_room(app: &mut TuiApp, room_id: String) {
    let room_addr = UserAddress::new(room_id.clone());
    app.call_state = CallState::InRoom(room_id.clone());
    app.add_log(format!("Joining SFU group room: {}", room_id));

    if let Some(ref session) = app.session {
        session.set_room_address(Some(room_addr.clone()));
        let join_packet = wpapi::protocol::ProtocolPacket::RoomJoinRequest {
            room_address: room_addr,
        };
        let session_clone = session.clone();
        tokio::spawn(async move {
            if let Err(e) = session_clone.send_packet(&join_packet).await {
                tracing::error!("Failed to send room join packet: {}", e);
            }
        });
    }
}

fn fetch_registered_addresses(app: &mut TuiApp) {
    let event_tx = app.event_tx.clone();
    let config = app.config.clone();
    app.add_log("Fetching registered user addresses from daemon...".to_string());

    tokio::spawn(async move {
        let server_url = format!("http://{}:{}", config.server_ip, config.server_port);
        let endpoint = format!("{}/addresses", server_url);
        let client = reqwest::Client::new();
        match client.get(&endpoint).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<Vec<UserAddress>>().await {
                        Ok(addrs) => {
                            let _ = event_tx.send(AppEvent::RegisteredAddresses(addrs)).await;
                        }
                        Err(e) => {
                            let _ = event_tx
                                .send(AppEvent::Log(format!("Failed to parse response: {}", e)))
                                .await;
                        }
                    }
                } else {
                    let _ = event_tx
                        .send(AppEvent::Log(format!(
                            "Server returned HTTP status {}",
                            resp.status()
                        )))
                        .await;
                }
            }
            Err(e) => {
                let _ = event_tx
                    .send(AppEvent::Log(format!("Failed to connect to daemon: {}", e)))
                    .await;
            }
        }
    });
}

fn handle_app_event(app: &mut TuiApp, evt: AppEvent) {
    match evt {
        AppEvent::ServerAddressAssigned(addr) => {
            app.add_log(format!("Server assigned Short ID: {}", addr.short_id()));
            app.my_address = Some(addr);
        }
        AppEvent::SessionEnded { session_id, result } => {
            match result {
                Ok(target) => app.add_log(format!("Session ended: {}", target)),
                Err(err) => app.add_log(format!("Session error: {}", err)),
            }
            if app.current_session_id == session_id {
                app.call_state = CallState::Idle;
                app.cancel_tx = None;
            }
        }
        AppEvent::CallAccepted {
            session_id: _,
            target,
        } => {
            app.add_log(format!("Call connected with target {}!", target));
            app.call_state = CallState::InCall(target);
        }
        AppEvent::CallRejected {
            session_id: _,
            target,
            reason,
        } => {
            app.add_log(format!("Call to {} failed: {}", target, reason));
            app.call_state = if app.cancel_tx.is_some() {
                CallState::Standby
            } else {
                CallState::Idle
            };
            if let Some(ref session) = app.session {
                session.set_target_address(None);
            }
        }
        AppEvent::CallEnded { target } => {
            app.add_log(format!("Call with target {} ended.", target));
            app.call_state = if app.cancel_tx.is_some() {
                CallState::Standby
            } else {
                CallState::Idle
            };
            if let Some(ref session) = app.session {
                session.set_target_address(None);
                session.set_room_address(None);
            }
        }
        AppEvent::IncomingCall { from, responder } => {
            app.add_log(format!("Incoming call request from: {}", from));
            if app.config.auto_accept {
                let _ = responder.send(true);
                app.add_log("Auto-accepted incoming call.".to_string());
                app.call_state = CallState::InCall(from);
            } else {
                app.incoming_from = Some(from);
                app.incoming_responder = Some(responder);
                app.input_mode = InputMode::IncomingCall;
            }
        }
        AppEvent::RegisteredAddresses(addrs) => {
            app.add_log(format!("Retrieved {} registered address(es).", addrs.len()));
            app.registered_addresses = addrs;
        }
        AppEvent::AudioLevels { input, output } => {
            app.input_level = input;
            app.output_level = output;
        }
        AppEvent::Log(msg) => {
            app.add_log(msg);
        }
    }
}

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

fn renew_identity(app: &mut TuiApp) {
    if let Some(tx) = app.cancel_tx.take() {
        let _ = tx.send(());
    }
    app.call_state = CallState::Idle;
    app.session = Some(ClientSession::with_keypair(wpapi::UserKeypair::generate()));
    app.my_address = None;
    app.add_log("Generated new Ed25519 identity keypair. Reconnecting to daemon...".to_string());
    start_standby(app);
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

async fn handle_key_input(app: &mut TuiApp, key: crossterm::event::KeyEvent) -> Result<bool> {
    let action = parse_key_event(&app.input_mode, key);
    execute_action(app, action).await
}

fn render_ui(f: &mut Frame, app: &TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main body
            Constraint::Length(3), // Footer / Controls
        ])
        .split(f.area());

    render_header(f, app, chunks[0]);
    render_body(f, app, chunks[1]);
    render_footer(f, app, chunks[2]);

    match app.input_mode {
        InputMode::CallInput => render_input_modal(f, app, "Enter Target User ID / Short ID:"),
        InputMode::RoomInput => render_input_modal(f, app, "Enter SFU Room ID:"),
        InputMode::IncomingCall => render_incoming_modal(f, app),
        InputMode::Normal => {}
    }
}

fn render_header(f: &mut Frame, app: &TuiApp, area: Rect) {
    let my_id_str = app
        .my_address
        .as_ref()
        .map(|a| a.short_id())
        .unwrap_or("Assigning...");

    let (status_text, status_color) = match &app.call_state {
        CallState::Idle => ("IDLE", Color::DarkGray),
        CallState::Standby => ("STANDBY (Waiting)", Color::Green),
        CallState::Connecting(_target) => ("CONNECTING...", Color::Yellow),
        CallState::InCall(_target) => ("IN CALL", Color::Cyan),
        CallState::InRoom(_room) => ("IN ROOM", Color::Magenta),
    };

    let title_line = vec![
        Span::styled(
            " web-phone TUI ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | Server: "),
        Span::styled(
            format!("{}:{}", app.config.server_ip, app.config.server_port),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(" | My ID: "),
        Span::styled(
            my_id_str,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | Status: "),
        Span::styled(
            status_text,
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    let header_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));

    let header_para = Paragraph::new(Line::from(title_line)).block(header_block);
    f.render_widget(header_para, area);
}

fn render_body(f: &mut Frame, app: &TuiApp, area: Rect) {
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Left: Audio Controls & Meters
            Constraint::Percentage(50), // Right: Logs & Addresses
        ])
        .split(area);

    render_left_panel(f, app, body_chunks[0]);
    render_right_panel(f, app, body_chunks[1]);
}

fn render_left_panel(f: &mut Frame, app: &TuiApp, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Mute & Info
            Constraint::Length(3), // Input Gauge (Mic)
            Constraint::Length(3), // Output Gauge (Speaker)
            Constraint::Min(3),    // Configuration summary
        ])
        .split(area);

    // Mute Status & Overview
    let mute_str = if app.is_muted {
        " [MUTED] "
    } else {
        " [ACTIVE] "
    };
    let mute_style = if app.is_muted {
        Style::default()
            .fg(Color::White)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    };

    let info_text = vec![
        Line::from(vec![
            Span::raw("Microphone: "),
            Span::styled(mute_str, mute_style),
            Span::raw(" (Press [m] to toggle)"),
        ]),
        Line::from(vec![
            Span::raw("Auto Accept: "),
            Span::styled(
                if app.config.auto_accept { "ON" } else { "OFF" },
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" (Press [a] to toggle)"),
        ]),
    ];

    let info_block = Block::default()
        .title(" Audio Control ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(info_text).block(info_block), chunks[0]);

    // Mic VU Meter
    let mic_gauge = Gauge::default()
        .block(
            Block::default()
                .title(" Mic Input Level ")
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(app.input_level.clamp(0.0, 1.0) as f64);
    f.render_widget(mic_gauge, chunks[1]);

    // Speaker VU Meter
    let spk_gauge = Gauge::default()
        .block(
            Block::default()
                .title(" Speaker Output Level ")
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(Color::Cyan))
        .ratio(app.output_level.clamp(0.0, 1.0) as f64);
    f.render_widget(spk_gauge, chunks[2]);

    // Extra Settings
    let settings_text = vec![
        Line::from(format!("STUN Server: {}", app.config.stun_server)),
        Line::from(format!("Sample Rate: {} Hz", app.config.sample_rate)),
        Line::from(format!("Channels: {}", app.config.channels)),
    ];
    let settings_block = Block::default()
        .title(" System Info ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(
        Paragraph::new(settings_text).block(settings_block),
        chunks[3],
    );
}

fn render_right_panel(f: &mut Frame, app: &TuiApp, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40), // Registered Addresses
            Constraint::Percentage(60), // Event Logs
        ])
        .split(area);

    // Address List
    let addr_items: Vec<Line> = if app.registered_addresses.is_empty() {
        vec![Line::from(Span::styled(
            "No addresses cached. Press [l] to fetch.",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.registered_addresses
            .iter()
            .map(|a| {
                Line::from(vec![
                    Span::styled(
                        format!(" • {}", a.short_id()),
                        Style::default().fg(Color::Green),
                    ),
                    Span::styled(
                        format!(" ({})", &a.to_string()[..16]),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            })
            .collect()
    };

    let addr_block = Block::default()
        .title(format!(
            " Registered Peers ({}) - [l] Fetch ",
            app.registered_addresses.len()
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(addr_items).block(addr_block), chunks[0]);

    // Log View
    let log_items: Vec<Line> = app
        .logs
        .iter()
        .rev()
        .take(chunks[1].height.saturating_sub(2) as usize)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|msg| {
            Line::from(Span::styled(
                format!("> {}", msg),
                Style::default().fg(Color::Gray),
            ))
        })
        .collect();

    let log_block = Block::default()
        .title(" Activity Logs ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(
        Paragraph::new(log_items)
            .block(log_block)
            .wrap(Wrap { trim: true }),
        chunks[1],
    );
}

fn render_footer(f: &mut Frame, app: &TuiApp, area: Rect) {
    let help_spans = match app.input_mode {
        InputMode::Normal => vec![
            Span::styled(
                " [s]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Standby |"),
            Span::styled(
                " [i]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Idle |"),
            Span::styled(
                " [c]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Call |"),
            Span::styled(
                " [r]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Room |"),
            Span::styled(
                " [m]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Mute |"),
            Span::styled(
                " [a]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Auto Accept |"),
            Span::styled(
                " [h]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Hangup |"),
            Span::styled(
                " [l]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" List Peers |"),
            Span::styled(
                " [u]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" New ID |"),
            Span::styled(
                " [q]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Quit"),
        ],
        InputMode::CallInput | InputMode::RoomInput => vec![
            Span::styled(
                " [Enter]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Submit |"),
            Span::styled(
                " [Esc]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Cancel"),
        ],
        InputMode::IncomingCall => vec![
            Span::styled(
                " [y]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Accept |"),
            Span::styled(
                " [n]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Reject"),
        ],
    };

    let footer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    let footer_para = Paragraph::new(Line::from(help_spans))
        .alignment(Alignment::Center)
        .block(footer_block);
    f.render_widget(footer_para, area);
}

fn render_input_modal(f: &mut Frame, app: &TuiApp, prompt: &str) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);

    let modal_block = Block::default()
        .title(Span::styled(
            " Input Prompt ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Cyan));

    let content = vec![
        Line::from(prompt),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Yellow)),
            Span::styled(
                &app.input_buffer,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "_",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::RAPID_BLINK),
            ),
        ]),
    ];

    let para = Paragraph::new(content).block(modal_block);
    f.render_widget(para, area);
}

fn render_incoming_modal(f: &mut Frame, app: &TuiApp) {
    let area = centered_rect(60, 25, f.area());
    f.render_widget(Clear, area);

    let from_id = app.incoming_from.as_deref().unwrap_or("Unknown");

    let modal_block = Block::default()
        .title(Span::styled(
            " INCOMING CALL ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Green));

    let content = vec![
        Line::from(Span::styled(
            "Incoming audio call request!",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("Caller ID: "),
            Span::styled(
                from_id,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press [y] to Accept, [n] to Reject",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]),
    ];

    let para = Paragraph::new(content)
        .alignment(Alignment::Center)
        .block(modal_block);
    f.render_widget(para, area);
}

/// Helper function to create a centered rect popup.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

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

        // Simulate pressing 'y' to accept incoming call
        let key_y = crossterm::event::KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let quit = handle_key_input(&mut app, key_y).await.unwrap();
        assert!(!quit);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(resp_rx.await.unwrap(), true);
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
