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
use std::{
    collections::VecDeque,
    io::stdout,
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use wpapi::{Configuration, UserAddress, session::ClientSession};

/// Background events sent to the TUI event loop.
#[derive(Debug)]
pub enum AppEvent {
    ServerAddressAssigned(UserAddress),
    SessionEnded(Result<String, String>),
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
        let mut app = Self {
            config,
            input_mode: InputMode::Normal,
            call_state: CallState::Idle,
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
            session: None,
            event_tx,
        };
        app.add_log("TUI Application initialized. Press [s] for standby, [c] to call, [r] for room.".to_string());
        app
    }

    pub fn add_log(&mut self, msg: String) {
        if self.logs.len() >= 100 {
            self.logs.pop_front();
        }
        self.logs.push_back(msg);
    }

    /// Hangup current call/session if active.
    pub fn hangup(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
            self.add_log("Hangup command sent.".to_string());
        }
        self.call_state = CallState::Idle;
        self.session = None;
        self.input_level = 0.0;
        self.output_level = 0.0;
    }
}

/// Run the main TUI loop.
pub async fn run_tui(config: Configuration) -> Result<()> {
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
    let mut app = TuiApp::new(config, event_tx.clone());

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
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn start_standby(app: &mut TuiApp) {
    app.hangup();
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let (assigned_tx, mut assigned_rx) = mpsc::channel::<UserAddress>(1);
    let event_tx = app.event_tx.clone();
    let config = app.config.clone();

    app.cancel_tx = Some(cancel_tx);
    app.call_state = CallState::Standby;
    app.add_log("Starting standby mode (listening for incoming calls)...".to_string());

    let session = ClientSession::new();
    let session_clone = session.clone();
    app.session = Some(session.clone());

    // Task to monitor address assignment
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
            let _ = event_tx_addr.send(AppEvent::ServerAddressAssigned(addr)).await;
        }
    });

    let event_tx_session = event_tx;
    tokio::spawn(async move {
        let res = wpapi::call::start_call_with_session(&config, None, Some(cancel_rx), &session)
            .await
            .map(|_| "Standby ended".to_string())
            .map_err(|e| e.to_string());
        let _ = event_tx_session.send(AppEvent::SessionEnded(res)).await;
    });
}

fn start_call(app: &mut TuiApp, target_id: String) {
    app.hangup();
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let event_tx = app.event_tx.clone();
    let config = app.config.clone();

    app.cancel_tx = Some(cancel_tx);
    app.call_state = CallState::Connecting(target_id.clone());
    app.add_log(format!("Initiating call to target User ID: {}", target_id));

    let session = ClientSession::new();
    app.session = Some(session.clone());

    let event_tx_session = event_tx;
    let target_addr = UserAddress::new(target_id.clone());
    tokio::spawn(async move {
        let res = wpapi::call::start_call_with_session(&config, Some(target_addr), Some(cancel_rx), &session)
            .await
            .map(|_| format!("Call with {} ended", target_id))
            .map_err(|e| e.to_string());
        let _ = event_tx_session.send(AppEvent::SessionEnded(res)).await;
    });
}

fn start_room(app: &mut TuiApp, room_id: String) {
    app.hangup();
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let event_tx = app.event_tx.clone();
    let config = app.config.clone();

    app.cancel_tx = Some(cancel_tx);
    app.call_state = CallState::Connecting(format!("Room {}", room_id));
    app.add_log(format!("Joining SFU group room: {}", room_id));

    let session = ClientSession::new();
    app.session = Some(session.clone());

    let event_tx_session = event_tx;
    let room_addr = UserAddress::new(room_id.clone());
    tokio::spawn(async move {
        let res = wpapi::call::start_room_call_with_session(&config, room_addr, Some(cancel_rx), &session)
            .await
            .map(|_| format!("Room {} session ended", room_id))
            .map_err(|e| e.to_string());
        let _ = event_tx_session.send(AppEvent::SessionEnded(res)).await;
    });
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
                            let _ = event_tx.send(AppEvent::Log(format!("Failed to parse response: {}", e))).await;
                        }
                    }
                } else {
                    let _ = event_tx.send(AppEvent::Log(format!("Server returned HTTP status {}", resp.status()))).await;
                }
            }
            Err(e) => {
                let _ = event_tx.send(AppEvent::Log(format!("Failed to connect to daemon: {}", e))).await;
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
        AppEvent::SessionEnded(res) => {
            match res {
                Ok(target) => app.add_log(format!("Session ended normally: {}", target)),
                Err(err) => app.add_log(format!("Session error: {}", err)),
            }
            app.call_state = CallState::Idle;
            app.cancel_tx = None;
        }
        AppEvent::IncomingCall { from, responder } => {
            app.add_log(format!("Incoming call request from: {}", from));
            if app.config.auto_accept {
                let _ = responder.send(true);
                app.add_log("Auto-accepted incoming call.".to_string());
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

async fn handle_key_input(app: &mut TuiApp, key: crossterm::event::KeyEvent) -> Result<bool> {
    // Global quit with Ctrl+C
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Ok(true);
    }

    match app.input_mode {
        InputMode::Normal => match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('s') => start_standby(app),
            KeyCode::Char('c') => {
                app.input_buffer.clear();
                app.input_mode = InputMode::CallInput;
            }
            KeyCode::Char('r') => {
                app.input_buffer.clear();
                app.input_mode = InputMode::RoomInput;
            }
            KeyCode::Char('m') => {
                app.is_muted = !app.is_muted;
                let status = if app.is_muted { "Muted" } else { "Unmuted" };
                app.add_log(format!("Microphone is now {}", status));
            }
            KeyCode::Char('h') | KeyCode::Char('x') => {
                app.hangup();
            }
            KeyCode::Char('l') => {
                fetch_registered_addresses(app);
            }
            _ => {}
        },
        InputMode::CallInput => match key.code {
            KeyCode::Enter => {
                let target = app.input_buffer.trim().to_string();
                app.input_mode = InputMode::Normal;
                if !target.is_empty() {
                    start_call(app, target);
                }
            }
            KeyCode::Esc => {
                app.input_mode = InputMode::Normal;
            }
            KeyCode::Char(c) => {
                app.input_buffer.push(c);
            }
            KeyCode::Backspace => {
                app.input_buffer.pop();
            }
            _ => {}
        },
        InputMode::RoomInput => match key.code {
            KeyCode::Enter => {
                let room = app.input_buffer.trim().to_string();
                app.input_mode = InputMode::Normal;
                if !room.is_empty() {
                    start_room(app, room);
                }
            }
            KeyCode::Esc => {
                app.input_mode = InputMode::Normal;
            }
            KeyCode::Char(c) => {
                app.input_buffer.push(c);
            }
            KeyCode::Backspace => {
                app.input_buffer.pop();
            }
            _ => {}
        },
        InputMode::IncomingCall => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if let Some(resp) = app.incoming_responder.take() {
                    let _ = resp.send(true);
                    app.add_log("Accepted incoming call.".to_string());
                }
                app.input_mode = InputMode::Normal;
                app.incoming_from = None;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                if let Some(resp) = app.incoming_responder.take() {
                    let _ = resp.send(false);
                    app.add_log("Rejected incoming call.".to_string());
                }
                app.input_mode = InputMode::Normal;
                app.incoming_from = None;
            }
            _ => {}
        },
    }

    Ok(false)
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
    let my_id_str = app.my_address.as_ref()
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
        Span::styled(" web-phone TUI ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" | Server: "),
        Span::styled(format!("{}:{}", app.config.server_ip, app.config.server_port), Style::default().fg(Color::Yellow)),
        Span::raw(" | My ID: "),
        Span::styled(my_id_str, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::raw(" | Status: "),
        Span::styled(status_text, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
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
    let mute_str = if app.is_muted { " [MUTED] " } else { " [ACTIVE] " };
    let mute_style = if app.is_muted {
        Style::default().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
    };

    let info_text = vec![
        Line::from(vec![
            Span::raw("Microphone: "),
            Span::styled(mute_str, mute_style),
            Span::raw(" (Press [m] to toggle)"),
        ]),
        Line::from(vec![
            Span::raw("Auto Accept: "),
            Span::styled(if app.config.auto_accept { "ON" } else { "OFF" }, Style::default().fg(Color::Yellow)),
        ]),
    ];

    let info_block = Block::default()
        .title(" Audio Control ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(info_text).block(info_block), chunks[0]);

    // Mic VU Meter
    let mic_gauge = Gauge::default()
        .block(Block::default().title(" Mic Input Level ").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(app.input_level.clamp(0.0, 1.0) as f64);
    f.render_widget(mic_gauge, chunks[1]);

    // Speaker VU Meter
    let spk_gauge = Gauge::default()
        .block(Block::default().title(" Speaker Output Level ").borders(Borders::ALL))
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
    f.render_widget(Paragraph::new(settings_text).block(settings_block), chunks[3]);
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
        vec![Line::from(Span::styled("No addresses cached. Press [l] to fetch.", Style::default().fg(Color::DarkGray)))]
    } else {
        app.registered_addresses
            .iter()
            .map(|a| Line::from(vec![
                Span::styled(format!(" • {}", a.short_id()), Style::default().fg(Color::Green)),
                Span::styled(format!(" ({})", &a.to_string()[..16]), Style::default().fg(Color::DarkGray)),
            ]))
            .collect()
    };

    let addr_block = Block::default()
        .title(format!(" Registered Peers ({}) - [l] Fetch ", app.registered_addresses.len()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(addr_items).block(addr_block), chunks[0]);

    // Log View
    let log_items: Vec<Line> = app.logs
        .iter()
        .rev()
        .take(chunks[1].height.saturating_sub(2) as usize)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|msg| Line::from(Span::styled(format!("> {}", msg), Style::default().fg(Color::Gray))))
        .collect();

    let log_block = Block::default()
        .title(" Activity Logs ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(log_items).block(log_block).wrap(Wrap { trim: true }), chunks[1]);
}

fn render_footer(f: &mut Frame, app: &TuiApp, area: Rect) {
    let help_spans = match app.input_mode {
        InputMode::Normal => vec![
            Span::styled(" [s]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw(" Standby |"),
            Span::styled(" [c]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw(" Call |"),
            Span::styled(" [r]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw(" Room |"),
            Span::styled(" [m]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw(" Mute |"),
            Span::styled(" [h]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw(" Hangup |"),
            Span::styled(" [l]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw(" List Peers |"),
            Span::styled(" [q]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), Span::raw(" Quit"),
        ],
        InputMode::CallInput | InputMode::RoomInput => vec![
            Span::styled(" [Enter]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)), Span::raw(" Submit |"),
            Span::styled(" [Esc]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), Span::raw(" Cancel"),
        ],
        InputMode::IncomingCall => vec![
            Span::styled(" [y]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)), Span::raw(" Accept |"),
            Span::styled(" [n]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), Span::raw(" Reject"),
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
        .title(Span::styled(" Input Prompt ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Cyan));

    let content = vec![
        Line::from(prompt),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Yellow)),
            Span::styled(&app.input_buffer, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("_", Style::default().fg(Color::Yellow).add_modifier(Modifier::RAPID_BLINK)),
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
        .title(Span::styled(" INCOMING CALL ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Green));

    let content = vec![
        Line::from(Span::styled("Incoming audio call request!", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(vec![
            Span::raw("Caller ID: "),
            Span::styled(from_id, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Press [y] to Accept, [n] to Reject", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
    ];

    let para = Paragraph::new(content).alignment(Alignment::Center).block(modal_block);
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
