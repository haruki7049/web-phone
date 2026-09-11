//! SFU group room view rendering for TUI client.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, Paragraph},
};

use crate::tui::app::TuiApp;

/// Render SFU group room view.
pub fn render_room_view(f: &mut Frame, app: &TuiApp, room: &str, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Room status header
            Constraint::Length(3), // Mic Level
            Constraint::Length(3), // Speaker Level
            Constraint::Min(2),    // Room details / active speakers
        ])
        .split(area);

    let room_header = vec![
        Line::from(vec![
            Span::styled(
                " SFU Room Address: ",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                room,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw(" Mode: "),
            Span::styled(
                "SFU MULTI-PARTY GROUP CALL",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let header_block = Block::default()
        .title(" SFU Group Voice Room ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Magenta));
    f.render_widget(Paragraph::new(room_header).block(header_block), chunks[0]);

    // Mic Level
    let (mic_title, mic_color) = if app.is_muted {
        (" Mic Level (MUTED - Press [m] to unmute) ", Color::Red)
    } else {
        (" Mic Level ", Color::Green)
    };
    let mic_gauge = Gauge::default()
        .block(Block::default().title(mic_title).borders(Borders::ALL))
        .gauge_style(Style::default().fg(mic_color))
        .ratio(app.input_level.clamp(0.0, 1.0) as f64);
    f.render_widget(mic_gauge, chunks[1]);

    // Speaker Level
    let spk_gauge = Gauge::default()
        .block(
            Block::default()
                .title(" Room Audio Level ")
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(Color::Magenta))
        .ratio(app.output_level.clamp(0.0, 1.0) as f64);
    f.render_widget(spk_gauge, chunks[2]);

    let details_text = vec![
        Line::from(format!(
            "Mute: {}",
            if app.is_muted {
                "MUTED (Press [m] to unmute)"
            } else {
                "ACTIVE (Press [m] to mute)"
            }
        )),
        Line::from(format!(
            "Echo Back: {} (Press [e] to toggle)",
            if app.config.allow_echoback {
                "ENABLED"
            } else {
                "DISABLED"
            }
        )),
        Line::from(format!(
            "Auto Accept: {} (Press [a] to toggle)",
            if app.config.auto_accept {
                "ENABLED"
            } else {
                "DISABLED"
            }
        )),
        Line::from("Press [h] to leave room."),
    ];
    let details_block = Block::default()
        .title(" Room Info & Active Speakers ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(details_text).block(details_block), chunks[3]);
}
