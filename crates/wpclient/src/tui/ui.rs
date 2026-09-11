//! Ratatui UI rendering functions for TUI client.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
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
            format!("{}:{}", app.config.host_str(), app.config.server_port),
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
        Span::raw(" | Echoback: "),
        Span::styled(
            if app.config.allow_echoback {
                "ON"
            } else {
                "OFF"
            },
            Style::default()
                .fg(if app.config.allow_echoback {
                    Color::Green
                } else {
                    Color::DarkGray
                })
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
    match &app.call_state {
        CallState::InCall(target) => {
            super::views::call::render_call_view(f, app, target, area);
        }
        CallState::InRoom(room) => {
            super::views::room::render_room_view(f, app, room, area);
        }
        CallState::Idle | CallState::Standby | CallState::Connecting(_) => {
            super::views::standby::render_standby_view(f, app, area);
        }
    }
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
                " [e]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Echoback |"),
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
