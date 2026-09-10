//! Ratatui UI rendering functions for TUI client.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Wrap},
};

use super::app::*;

pub fn render_ui(f: &mut Frame, app: &TuiApp) {
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
